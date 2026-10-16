#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::Path,
    time::Duration,
};

use assert_cmd::Command;
use serde_json::json;

#[allow(dead_code)]
mod support;

fn command(root: &Path) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("psst"));
    command
        .current_dir(root)
        .env("PSST_CONFIG", root.join("custom.json"))
        .env("TMPDIR", root)
        .env_remove("SSH_AUTH_SOCK")
        .args(["--write-files"])
        .timeout(Duration::from_secs(10));
    command
}

fn private(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

fn no_temporary_files(root: &Path) {
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        assert!(!name.starts_with("psst-files-") && !name.starts_with(".psst-write-"));
        if entry.file_type().unwrap().is_dir() {
            no_temporary_files(&entry.path());
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fake_agent_failures_are_best_effort_only_for_materialization() {
    use support::agent::{FakeAgent, Fault};
    let agent = FakeAgent::new().await;
    for fault in [Fault::Refuse, Fault::InvalidSignature] {
        agent.fault(fault);
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        fs::create_dir(root.join(".psst")).unwrap();
        fs::set_permissions(root.join(".psst"), fs::Permissions::from_mode(0o700)).unwrap();
        let record = root.join(".psst").join(agent.key.filename());
        private(&record, include_bytes!("fixtures/local-storage-v1.json"));
        fs::write(root.join("custom.json"), json!({
            "ENV": {"source":"local-storage", "value":"fixture"},
            "before": {"type":"file", "source":"manual", "value":"BEFORE_PRIVATE"},
            "local": {"type":"file", "source":"local-storage", "value":"fixture"},
            "after": {"type":"file", "source":"shell", "value":"touch fetched; printf AFTER_PRIVATE"}
        }).to_string()).unwrap();
        let output = command(root)
            .env("SSH_AUTH_SOCK", agent.socket())
            .arg("--yes")
            .assert()
            .failure()
            .get_output()
            .clone();
        let report = String::from_utf8_lossy(&output.stderr);
        assert!(
            report.contains("local: failed: Provider resolution failed"),
            "{report}"
        );
        assert!(report.contains("before: written") && report.contains("after: written"));
        assert_eq!(fs::read(root.join("before")).unwrap(), b"BEFORE_PRIVATE");
        assert_eq!(fs::read(root.join("after")).unwrap(), b"AFTER_PRIVATE");
        assert!(!root.join("local").exists());
        fs::remove_file(root.join("fetched")).unwrap();
        private(&root.join(".env"), b"ORIGINAL_PRIVATE");
        for args in [vec!["--write"], vec!["/bin/sh", "-c", "touch launched"]] {
            let output = Command::new(assert_cmd::cargo::cargo_bin!("psst"))
                .current_dir(root)
                .env("PSST_CONFIG", root.join("custom.json"))
                .env("TMPDIR", root)
                .env("SSH_AUTH_SOCK", agent.socket())
                .args(args)
                .timeout(Duration::from_secs(10))
                .assert()
                .failure()
                .get_output()
                .clone();
            let report = String::from_utf8_lossy(&output.stderr);
            assert!(
                report.contains("Local-storage resolution failed; operation stopped"),
                "{report}"
            );
            assert!(!report.contains("BEFORE_PRIVATE") && !report.contains("AFTER_PRIVATE"));
            assert!(!root.join("launched").exists() && !root.join("fetched").exists());
            assert_eq!(fs::read(root.join(".env")).unwrap(), b"ORIGINAL_PRIVATE");
        }
        assert!(!report.contains("PRIVATE") && !report.contains("🔑"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE"));
        assert_eq!(
            fs::read(&record).unwrap(),
            include_bytes!("fixtures/local-storage-v1.json")
        );
        no_temporary_files(root);
    }
}

#[test]
fn invalid_directory_containing_staging_does_not_abort_independent_files() {
    for directory_first in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        let bad = (
            root.to_str().unwrap(),
            json!({"type":"file", "source":"shell", "value":"touch bad-fetch; printf BAD_PRIVATE"}),
        );
        let good = (
            "independent",
            json!({"type":"file", "source":"shell", "value":"touch good-fetch; printf GOOD_PRIVATE"}),
        );
        let mappings = if directory_first {
            [bad, good]
        } else {
            [good, bad]
        };
        let config: serde_json::Map<_, _> = mappings
            .into_iter()
            .map(|(name, entry)| (name.to_owned(), entry))
            .collect();
        fs::write(
            root.join("custom.json"),
            serde_json::to_vec(&config).unwrap(),
        )
        .unwrap();
        let output = command(root)
            .arg("--yes")
            .assert()
            .failure()
            .get_output()
            .clone();
        let report = String::from_utf8_lossy(&output.stderr);
        assert!(report.contains("not a regular file"), "{report}");
        assert!(report.contains("independent: written"), "{report}");
        assert!(!report.contains("GOOD_PRIVATE") && !report.contains("BAD_PRIVATE"));
        assert!(!root.join("bad-fetch").exists());
        assert!(root.join("good-fetch").exists());
        assert_eq!(fs::read(root.join("independent")).unwrap(), b"GOOD_PRIVATE");
        no_temporary_files(root);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn unknown_linux_mount_rejects_whole_batch_before_fetch() {
    for name in ["Output", "é", "e\u{301}", "K"] {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        private(&root.join("existing"), b"ORIGINAL_PRIVATE");
        let before = fs::metadata(root.join("existing")).unwrap();
        let unsupported = format!("/proc/psst-absent-{}/{name}", std::process::id());
        fs::write(root.join("custom.json"), json!({
            "existing": {"type":"file", "source":"shell", "value":"touch fetched; printf CHANGED_PRIVATE"},
            "new/independent": {"type":"file", "source":"shell", "value":"touch fetched; printf NEW_PRIVATE"},
            unsupported.clone(): {"type":"file", "source":"shell", "value":"touch fetched; printf OTHER_PRIVATE"}
        }).to_string()).unwrap();
        let config_before = fs::read(root.join("custom.json")).unwrap();
        let output = command(root).arg("--yes").output().unwrap();
        let report = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{report}");
        assert!(
            report.contains("Cannot establish destination filesystem name equivalence"),
            "{report}"
        );
        assert!(!root.join("fetched").exists());
        assert!(!root.join("new").exists());
        assert!(!Path::new(&unsupported).exists());
        assert_eq!(
            fs::read(root.join("existing")).unwrap(),
            b"ORIGINAL_PRIVATE"
        );
        let after = fs::metadata(root.join("existing")).unwrap();
        assert_eq!(
            (before.ino(), before.mode(), before.mtime(), before.ctime()),
            (after.ino(), after.mode(), after.mtime(), after.ctime())
        );
        assert_eq!(fs::read(root.join("custom.json")).unwrap(), config_before);
        assert_eq!(fs::read_dir(root).unwrap().count(), 2);
        no_temporary_files(root);
    }
}

#[test]
fn absent_case_aliases_are_preflighted_using_filesystem_semantics() {
    for (first, second) in [("Output", "output"), ("Missing/Output", "missing/output")] {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        // Fixture setup detects native volume behavior without command-time
        // destination probes. Both candidate destinations remain absent.
        fs::create_dir(root.join("CaseDirectory")).unwrap();
        let insensitive = root.join("casedirectory").is_dir();
        let first = format!("CaseDirectory/{first}");
        let second = format!("CaseDirectory/{second}");
        fs::write(root.join("custom.json"), json!({
            first.clone(): {"type":"file", "source":"shell", "value":"touch first-fetch; printf FIRST_PRIVATE"},
            second.clone(): {"type":"file", "source":"shell", "value":"touch second-fetch; printf SECOND_PRIVATE"}
        }).to_string()).unwrap();
        let output = command(root).arg("--yes").output().unwrap();
        let report = String::from_utf8_lossy(&output.stderr);
        if insensitive {
            assert!(!output.status.success(), "{report}");
            assert!(report.contains("alias one another"), "{report}");
            assert!(!root.join("first-fetch").exists());
            assert!(!root.join("second-fetch").exists());
            assert!(!root.join(&first).exists());
            assert!(!root.join(&second).exists());
            assert_eq!(fs::read_dir(root.join("CaseDirectory")).unwrap().count(), 0);
        } else {
            assert!(output.status.success(), "{report}");
            assert_eq!(fs::read(root.join(&first)).unwrap(), b"FIRST_PRIVATE");
            assert_eq!(fs::read(root.join(&second)).unwrap(), b"SECOND_PRIVATE");
        }
        assert!(!report.contains("FIRST_PRIVATE") && !report.contains("SECOND_PRIVATE"));
        no_temporary_files(root);
    }
}

#[test]
fn directory_only_suffixes_fail_independently_without_modifying_targets() {
    for suffix in ["/", "/."] {
        for existing in [None, Some("OLD_PRIVATE"), Some("NEW_PRIVATE")] {
            let root = tempfile::tempdir().unwrap();
            let root = root.path();
            let destination = format!("output{suffix}");
            if let Some(contents) = existing {
                private(&root.join("output"), contents.as_bytes());
            }
            let before = fs::metadata(root.join("output")).ok();
            fs::write(
                root.join("custom.json"),
                json!({
                    destination.clone(): {"type":"file", "source":"manual", "value":"NEW_PRIVATE"},
                    "independent": {"type":"file", "source":"manual", "value":"GOOD_PRIVATE"}
                })
                .to_string(),
            )
            .unwrap();
            let output = command(root)
                .arg("--yes")
                .assert()
                .failure()
                .get_output()
                .clone();
            let report = String::from_utf8_lossy(&output.stderr);
            assert!(
                report.contains(&format!("{destination}: failed:")),
                "{report}"
            );
            assert!(report.contains("directory-only suffix"), "{report}");
            assert!(
                !report.contains(&format!("{destination}: written")),
                "{report}"
            );
            assert!(
                !report.contains(&format!("{destination}: unchanged")),
                "{report}"
            );
            assert!(report.contains("independent: written"), "{report}");
            assert!(!report.contains("NEW_PRIVATE") && !report.contains("GOOD_PRIVATE"));
            assert_eq!(fs::read(root.join("independent")).unwrap(), b"GOOD_PRIVATE");
            if let Some(contents) = existing {
                assert_eq!(fs::read(root.join("output")).unwrap(), contents.as_bytes());
                let after = fs::metadata(root.join("output")).unwrap();
                let before = before.unwrap();
                assert_eq!(after.ino(), before.ino());
                assert_eq!(after.mode(), before.mode());
                assert_eq!(
                    (after.mtime(), after.mtime_nsec()),
                    (before.mtime(), before.mtime_nsec())
                );
                assert_eq!(
                    (after.ctime(), after.ctime_nsec()),
                    (before.ctime(), before.ctime_nsec())
                );
            } else {
                assert!(!root.join("output").exists());
            }
            assert!(fs::read(root.join(&destination)).is_err());
            no_temporary_files(root);
        }
    }
}

#[test]
fn empty_managed_sets_succeed_without_usable_system_temp() {
    for config in [
        json!({}),
        json!({"TOKEN": {"source":"manual", "value":"PRIVATE"}}),
    ] {
        for missing in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let root = root.path();
            fs::write(root.join("custom.json"), config.to_string()).unwrap();
            let bad_temp = root.join("bad-temp");
            if !missing {
                fs::write(&bad_temp, b"not a directory").unwrap();
            }
            let output = command(root)
                .env("TMPDIR", &bad_temp)
                .assert()
                .success()
                .get_output()
                .clone();
            let report = String::from_utf8_lossy(&output.stderr);
            assert!(report.contains("No managed files"), "{report}");
            assert!(report.contains("Files view"), "{report}");
            assert!(!report.contains("PRIVATE"));
            if missing {
                assert!(!bad_temp.exists());
            } else {
                assert_eq!(fs::read(&bad_temp).unwrap(), b"not a directory");
            }
            no_temporary_files(root);
        }
    }
}

#[test]
fn missing_prefix_parent_traversal_requires_consent_for_exposed_replacements() {
    for existing in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        if existing {
            private(&root.join("output"), b"OLD_PRIVATE");
        }
        fs::write(
            root.join("custom.json"),
            json!({
                "new/../output": {"type":"file", "source":"manual", "value":"NEW_PRIVATE"},
                "independent": {"type":"file", "source":"manual", "value":"GOOD_PRIVATE"}
            })
            .to_string(),
        )
        .unwrap();
        if existing {
            let output = command(root).assert().failure().get_output().clone();
            let report = String::from_utf8_lossy(&output.stderr);
            assert!(report.contains("new/../output"));
            assert!(report.contains("Replacement requires interactive consent or --yes"));
            assert!(!root.join("new").exists());
            assert!(!root.join("independent").exists());
            assert_eq!(fs::read(root.join("output")).unwrap(), b"OLD_PRIVATE");
            no_temporary_files(root);
        }
        let mut cmd = command(root);
        if existing {
            cmd.arg("--yes");
        }
        let output = cmd.assert().success().get_output().clone();
        let report = String::from_utf8_lossy(&output.stderr);
        assert!(report.contains("new/../output: written"), "{report}");
        assert!(report.contains("independent: written"), "{report}");
        assert!(!report.contains("NEW_PRIVATE") && !report.contains("GOOD_PRIVATE"));
        assert_eq!(fs::read(root.join("output")).unwrap(), b"NEW_PRIVATE");
        assert_eq!(fs::read(root.join("independent")).unwrap(), b"GOOD_PRIVATE");
        assert_eq!(
            fs::metadata(root.join("new")).unwrap().mode() & 0o7777,
            0o700
        );
        assert_eq!(
            fs::metadata(root.join("output")).unwrap().mode() & 0o7777,
            0o600
        );
        no_temporary_files(root);
    }
}

#[test]
fn identical_missing_prefix_traversal_is_reachable_without_replacement() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    private(&root.join("output"), b"SAME_PRIVATE");
    let before = fs::metadata(root.join("output")).unwrap();
    fs::write(
        root.join("custom.json"),
        json!({
            "new/../output": {"type":"file", "source":"manual", "value":"SAME_PRIVATE"}
        })
        .to_string(),
    )
    .unwrap();
    let output = command(root).assert().success().get_output().clone();
    let report = String::from_utf8_lossy(&output.stderr);
    assert!(report.contains("new/../output: unchanged"), "{report}");
    assert!(!report.contains("Replace all?") && !report.contains("SAME_PRIVATE"));
    assert_eq!(
        fs::read(root.join("new/../output")).unwrap(),
        b"SAME_PRIVATE"
    );
    assert_eq!(
        fs::metadata(root.join("output")).unwrap().ino(),
        before.ino()
    );
    assert_eq!(
        fs::metadata(root.join("new")).unwrap().mode() & 0o7777,
        0o700
    );
    no_temporary_files(root);
}

#[test]
fn missing_prefix_hardlink_aliases_stop_before_fetch_or_writes() {
    for names in [
        ["missing/../one", "other/../two"],
        ["one", "missing/../two"],
        ["missing/../one", "two"],
    ] {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        private(&root.join("one"), b"OLD_PRIVATE");
        fs::hard_link(root.join("one"), root.join("two")).unwrap();
        let before = fs::metadata(root.join("one")).unwrap();
        fs::write(
            root.join("custom.json"),
            json!({
                names[0]: {"type":"file", "source":"shell", "value":"touch fetched; printf NEW_PRIVATE"},
                names[1]: {"type":"file", "source":"shell", "value":"touch fetched; printf NEW_PRIVATE"},
                "independent": {"type":"file", "source":"shell", "value":"touch fetched; printf NEW_PRIVATE"}
            })
            .to_string(),
        )
        .unwrap();
        let output = command(root)
            .arg("--yes")
            .assert()
            .failure()
            .get_output()
            .clone();
        let report = String::from_utf8_lossy(&output.stderr);
        assert!(report.contains("alias one another"), "{report}");
        for path in ["fetched", "independent", "missing", "other"] {
            assert!(!root.join(path).exists());
        }
        for path in ["one", "two"] {
            assert_eq!(fs::read(root.join(path)).unwrap(), b"OLD_PRIVATE");
            assert_eq!(fs::metadata(root.join(path)).unwrap().ino(), before.ino());
        }
        no_temporary_files(root);
    }
}

#[test]
fn missing_prefix_protected_hardlinks_stop_before_fetch_or_writes() {
    for protected in [
        "custom.json",
        "custom.json.lock",
        ".psst/key.json",
        ".psst/key.lock",
    ] {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        fs::write(
            root.join("custom.json"),
            json!({
                "missing/../alias": {"type":"file", "source":"shell", "value":"touch fetched; printf NEW_PRIVATE"},
                "independent": {"type":"file", "source":"shell", "value":"touch fetched; printf NEW_PRIVATE"}
            })
            .to_string(),
        )
        .unwrap();
        if protected != "custom.json" {
            fs::create_dir_all(root.join(protected).parent().unwrap()).unwrap();
            private(&root.join(protected), b"PROTECTED_PRIVATE");
        }
        fs::hard_link(root.join(protected), root.join("alias")).unwrap();
        let bytes = fs::read(root.join(protected)).unwrap();
        let before = fs::metadata(root.join(protected)).unwrap();
        let output = command(root)
            .arg("--yes")
            .assert()
            .failure()
            .get_output()
            .clone();
        let report = String::from_utf8_lossy(&output.stderr);
        assert!(report.contains("protected psst artifacts"), "{report}");
        for path in ["fetched", "independent", "missing"] {
            assert!(!root.join(path).exists());
        }
        for path in [protected, "alias"] {
            assert_eq!(fs::read(root.join(path)).unwrap(), bytes);
            assert_eq!(fs::metadata(root.join(path)).unwrap().ino(), before.ino());
        }
        no_temporary_files(root);
    }
}

#[test]
fn restrictive_umask_still_creates_exact_private_parents() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("existing")).unwrap();
    fs::set_permissions(
        root.path().join("existing"),
        fs::Permissions::from_mode(0o750),
    )
    .unwrap();
    fs::write(
        root.path().join("custom.json"),
        json!({
            "new/deep/one": {"type":"file", "source":"manual", "value":"PRIVATE"},
            "new/two": {"type":"file", "source":"manual", "value":"PRIVATE"},
            "existing/three": {"type":"file", "source":"manual", "value":"PRIVATE"}
        })
        .to_string(),
    )
    .unwrap();
    // Set process-global umask only in an isolated child, never in this test
    // process where other tests run concurrently.
    let mut child = std::process::Command::new("sh");
    child
        .args(["-c", "umask 0200; exec \"$1\" --write-files", "sh"])
        .arg(assert_cmd::cargo::cargo_bin!("psst"))
        .current_dir(root.path())
        .env("PSST_CONFIG", root.path().join("custom.json"))
        .env("TMPDIR", root.path());
    Command::from_std(child)
        .timeout(Duration::from_secs(10))
        .assert()
        .success();
    for path in ["new", "new/deep"] {
        assert_eq!(
            fs::metadata(root.path().join(path)).unwrap().mode() & 0o7777,
            0o700
        );
    }
    assert_eq!(
        fs::metadata(root.path().join("existing")).unwrap().mode() & 0o7777,
        0o750
    );
    for path in ["new/deep/one", "new/two", "existing/three"] {
        assert_eq!(
            fs::metadata(root.path().join(path)).unwrap().mode() & 0o7777,
            0o600
        );
        assert_eq!(fs::read(root.path().join(path)).unwrap(), b"PRIVATE");
    }
    no_temporary_files(root.path());
}

#[test]
fn mixed_batch_requires_consent_before_new_files_then_best_effort_private_writes() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    fs::create_dir(root.join("existing")).unwrap();
    fs::set_permissions(root.join("existing"), fs::Permissions::from_mode(0o755)).unwrap();
    private(&root.join("same"), b"SAME_PRIVATE");
    let same_inode = fs::metadata(root.join("same")).unwrap().ino();
    private(&root.join("different"), b"OLD_PRIVATE");
    private(&root.join("permissions"), b"SAME_PRIVATE");
    fs::set_permissions(root.join("permissions"), fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(
        root.join("custom.json"),
        json!({
            "new/deep/first": {"type":"file", "source":"manual", "value":"NEW_PRIVATE"},
            "new/deep/second": {"type":"file", "source":"manual", "value":"NEW_PRIVATE"},
            "existing/child": {"type":"file", "source":"shell", "value":"printf ''"},
            "same": {"type":"file", "source":"manual", "value":"SAME_PRIVATE"},
            "different": {"type":"file", "source":"manual", "value":"REPLACED_PRIVATE"},
            "permissions": {"type":"file", "source":"manual", "value":"SAME_PRIVATE"},
            "local-error": {"type":"file", "source":"local-storage", "value":"invalid reference"},
            "unknown-error": {"type":"file", "source":"unknown", "value":"NEVER_SHOW_PRIVATE"}
        })
        .to_string(),
    )
    .unwrap();
    let output = command(root).assert().failure().get_output().clone();
    let report = String::from_utf8_lossy(&output.stderr);
    assert!(report.contains("Replacement requires interactive consent or --yes"));
    assert!(report.contains("permissions: failed"));
    assert!(!root.join("new").exists());
    assert!(!root.join("existing/child").exists());
    assert_eq!(fs::read(root.join("different")).unwrap(), b"OLD_PRIVATE");
    no_temporary_files(root);
    let output = command(root)
        .arg("--yes")
        .assert()
        .failure()
        .get_output()
        .clone();
    let report = String::from_utf8_lossy(&output.stderr);
    for needle in [
        "first: written",
        "second: written",
        "child: written",
        "same: unchanged",
        "different: written",
        "permissions: failed",
        "local-error: failed",
        "unknown-error: failed",
    ] {
        assert!(report.contains(needle), "{report}");
    }
    for contents in [
        "NEW_PRIVATE",
        "SAME_PRIVATE",
        "REPLACED_PRIVATE",
        "NEVER_SHOW_PRIVATE",
    ] {
        assert!(!report.contains(contents));
        assert!(!String::from_utf8_lossy(&output.stdout).contains(contents));
    }
    assert_eq!(
        fs::read(root.join("different")).unwrap(),
        b"REPLACED_PRIVATE"
    );
    assert_eq!(fs::metadata(root.join("same")).unwrap().ino(), same_inode);
    assert_eq!(fs::read(root.join("existing/child")).unwrap(), b"");
    for path in ["new", "new/deep"] {
        assert_eq!(
            fs::metadata(root.join(path)).unwrap().mode() & 0o7777,
            0o700
        );
    }
    for path in [
        "new/deep/first",
        "new/deep/second",
        "different",
        "existing/child",
    ] {
        let metadata = fs::metadata(root.join(path)).unwrap();
        assert_eq!(metadata.mode() & 0o7777, 0o600);
        // SAFETY: geteuid has no arguments or pointers.
        assert_eq!(metadata.uid(), unsafe { libc::geteuid() });
    }
    assert_eq!(
        fs::metadata(root.join("existing")).unwrap().mode() & 0o7777,
        0o755
    );
    assert_eq!(
        fs::metadata(root.join("permissions")).unwrap().mode() & 0o7777,
        0o644
    );
    no_temporary_files(root);
}

#[test]
fn empty_new_and_no_change_runs_need_no_prompt_and_help_exposes_command() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    fs::write(root.join("custom.json"), "{}").unwrap();
    let output = command(root).assert().success().get_output().clone();
    assert!(String::from_utf8_lossy(&output.stderr).contains("Files"));
    fs::write(
        root.join("custom.json"),
        json!({
            "new": {"type":"file", "source":"manual", "value":"PRIVATE"}
        })
        .to_string(),
    )
    .unwrap();
    for (expected, emoji) in [("new: written", "📄"), ("new: unchanged", "⏭️")] {
        let output = command(root).assert().success().get_output().clone();
        let report = String::from_utf8_lossy(&output.stderr);
        assert!(report.contains(expected));
        assert!(report.starts_with(emoji), "missing outcome emoji: {report}");
        assert!(!report.contains("Replace all?"));
        no_temporary_files(root);
    }
    let output = command(root)
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .clone();
    let help = String::from_utf8_lossy(&output.stdout);
    for guidance in ["--write-files [--yes]", "Open the TUI"] {
        assert!(help.contains(guidance), "missing help: {guidance}");
    }
    command(root).arg("--dry-run").assert().failure();
    command(root).arg("subset").assert().failure();
}

#[test]
fn yes_does_not_bypass_symlinks_or_preflight_and_dispatch_preserves_wrapped_argv() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    private(&root.join("target"), b"OLD_PRIVATE");
    symlink(root.join("target"), root.join("link")).unwrap();
    fs::write(
        root.join("custom.json"),
        json!({
            "link": {"type":"file", "source":"manual", "value":"NEW_PRIVATE"},
            "valid": {"type":"file", "source":"manual", "value":"NEW_PRIVATE"}
        })
        .to_string(),
    )
    .unwrap();
    command(root).arg("--yes").assert().failure();
    assert_eq!(fs::read(root.join("target")).unwrap(), b"OLD_PRIVATE");
    assert_eq!(fs::read(root.join("valid")).unwrap(), b"NEW_PRIVATE");
    fs::remove_file(root.join("valid")).unwrap();
    fs::write(root.join("custom.json"), json!({
        "custom.json": {"type":"file", "source":"shell", "value":"touch fetched; printf PRIVATE"},
        "valid": {"type":"file", "source":"manual", "value":"NEW_PRIVATE"}
    }).to_string()).unwrap();
    command(root).arg("--yes").assert().failure();
    assert!(!root.join("fetched").exists());
    assert!(!root.join("valid").exists());
    let mut wrapped = Command::new(assert_cmd::cargo::cargo_bin!("psst"));
    wrapped
        .current_dir(root)
        .env("PSST_CONFIG", root.join("custom.json"))
        .args([
            "/bin/sh",
            "-c",
            "printf '%s|%s' \"$1\" \"$2\"",
            "sh",
            "--write-files",
            "--yes",
        ])
        .timeout(Duration::from_secs(10))
        .assert()
        .success()
        .stdout("--write-files|--yes");
    no_temporary_files(root);
}
