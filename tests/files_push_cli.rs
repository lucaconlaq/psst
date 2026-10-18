#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
    time::Duration,
};

use assert_cmd::Command;
use serde_json::{Value, json};

#[allow(dead_code)]
mod support;

fn command(root: &Path) -> Command {
    let tools = root.join("fake-tools");
    fs::create_dir_all(&tools).unwrap();
    for name in ["op", "vault", "terraform"] {
        let path = tools.join(name);
        fs::write(&path, "#!/bin/sh\ntouch PROVIDER_EXECUTED\nexit 1\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("psst"));
    command
        .current_dir(root)
        .env("PSST_CONFIG", root.join("custom.json"))
        .env("TMPDIR", root)
        .env("PATH", format!("{}:/usr/bin:/bin", tools.display()))
        .env_remove("SSH_AUTH_SOCK")
        .timeout(Duration::from_secs(30));
    command
}

fn file(source: &str, value: &str) -> Value {
    json!({"type":"file", "source":source, "value":value})
}

fn config(root: &Path, value: Value) -> Vec<u8> {
    let bytes = serde_json::to_vec(&value).unwrap();
    fs::write(root.join("custom.json"), &bytes).unwrap();
    bytes
}

fn report(output: &std::process::Output) -> String {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!text.contains("PRIVATE"), "{text}");
    text
}

fn no_staging(root: &Path) {
    for entry in fs::read_dir(root).unwrap() {
        let name = entry.unwrap().file_name();
        assert!(!name.to_string_lossy().starts_with("psst-files-"));
        assert!(!name.to_string_lossy().starts_with(".psst-write-"));
    }
}

#[test]
fn compact_push_output_uses_configured_paths_and_stays_plain_when_redirected() {
    let root = support::tempdir();
    let root = root.path();
    config(root, json!({"one":file("manual", "OLD_PRIVATE")}));
    fs::write(root.join("one"), "NEW_PRIVATE").unwrap();
    let output = command(root)
        .args(["--files-push", "--yes"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert_eq!(
        report(&output),
        "Push local → provider:\n  one [manual]\n⚠️  Other references may change; only this config was checked.\n✅ one: saved\n"
    );
}

#[test]
fn empty_unchanged_and_mixed_batches_are_content_free_and_ordered() {
    let root = support::tempdir();
    let root = root.path();
    config(
        root,
        json!({"ENV":{"source":"shell", "value":"touch ENV_EXECUTED"}}),
    );
    let output = command(root)
        .arg("--files-push")
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(report(&output).contains("No managed files"));
    assert!(!root.join("ENV_EXECUTED").exists());
    fs::write(root.join("same"), " \nSAME_PRIVATE 🔑\t").unwrap();
    let original = config(root, json!({"same":file("manual", " \nSAME_PRIVATE 🔑\t")}));
    let output = command(root)
        .arg("--files-push")
        .assert()
        .success()
        .get_output()
        .clone();
    let text = report(&output);
    assert_eq!(text, "⏭️  same: unchanged\n");
    assert_eq!(fs::read(root.join("custom.json")).unwrap(), original);
    assert!(!root.join("custom.json.lock").exists());

    fs::write(root.join("changed"), " \nNEW_PRIVATE 🔑\t\n").unwrap();
    fs::set_permissions(root.join("changed"), fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(root.join("empty"), "").unwrap();
    fs::write(root.join("unsupported"), "LOCAL_PRIVATE").unwrap();
    config(
        root,
        json!({
            "ENV":{"source":"manual", "value":"ENV_PRIVATE"},
            "same":file("manual", " \nSAME_PRIVATE 🔑\t"),
            "changed":file("manual", "OLD_PRIVATE"),
            "empty":file("manual", "OLD_PRIVATE"),
            "unsupported":file("shell", "touch SHELL_EXECUTED"),
            "missing":file("manual", "OLD_PRIVATE")
        }),
    );
    let output = command(root)
        .args(["--files-push", "--yes"])
        .assert()
        .failure()
        .get_output()
        .clone();
    let text = report(&output);
    assert_eq!(text.matches("Push local → provider:").count(), 1);
    for outcome in [
        "same: unchanged",
        "changed: saved",
        "empty: saved",
        "unsupported: failed",
        "missing: failed",
        "partially completed",
    ] {
        assert!(text.contains(outcome), "{text}");
    }
    assert!(text.find("changed: saved").unwrap() < text.find("empty: saved").unwrap());
    assert!(!text.contains("ENV: "));
    let loaded: Value =
        serde_json::from_slice(&fs::read(root.join("custom.json")).unwrap()).unwrap();
    assert_eq!(loaded["changed"]["value"], " \nNEW_PRIVATE 🔑\t\n");
    assert_eq!(loaded["empty"]["value"], "");
    assert_eq!(loaded["ENV"]["value"], "ENV_PRIVATE");
    assert_eq!(
        fs::read(root.join("changed")).unwrap(),
        " \nNEW_PRIVATE 🔑\t\n".as_bytes()
    );
    assert_eq!(
        fs::metadata(root.join("changed"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o644
    );
    assert!(!root.join("SHELL_EXECUTED").exists());
    no_staging(root);
}

#[test]
fn unsupported_and_invalid_references_are_not_executed_and_fail_independently() {
    let root = support::tempdir();
    let root = root.path();
    let mut mappings = json!({});
    for source in [
        "op",
        "local-storage",
        "vault",
        "terraform",
        "shell",
        "op 2FA",
        "unknown",
    ] {
        fs::write(root.join(source), "LOCAL_PRIVATE").unwrap();
        mappings[source] = file(source, "touch EXECUTED_PRIVATE");
    }
    fs::write(root.join("manual"), "NEW_PRIVATE").unwrap();
    mappings["manual"] = file("manual", "old");
    config(root, mappings);
    let output = command(root)
        .args(["--files-push", "--yes"])
        .assert()
        .failure()
        .get_output()
        .clone();
    let text = report(&output);
    assert_eq!(text.matches("provider does not support push").count(), 5);
    assert!(text.contains("op: failed: cannot resolve existing eligible op target"));
    assert!(text.contains("local-storage: failed: cannot resolve existing encrypted target"));
    assert!(text.contains("manual: saved"));
    assert!(!root.join("EXECUTED_PRIVATE").exists());
    assert!(!root.join("PROVIDER_EXECUTED").exists());
}

#[test]
fn unavailable_approval_leaves_config_and_inputs_exactly_unchanged() {
    let root = support::tempdir();
    let root = root.path();
    fs::write(root.join("one"), "NEW_PRIVATE").unwrap();
    let original = config(root, json!({"one":file("manual", "OLD_PRIVATE")}));
    let output = command(root)
        .arg("--files-push")
        .write_stdin("yes\n")
        .assert()
        .failure()
        .get_output()
        .clone();
    let text = report(&output);
    assert!(text.contains("requires interactive consent or --yes"));
    assert!(text.contains("one: failed"));
    assert_eq!(text.matches("Push local → provider:").count(), 1);
    assert_eq!(fs::read(root.join("custom.json")).unwrap(), original);
    assert_eq!(fs::read(root.join("one")).unwrap(), b"NEW_PRIVATE");
    assert!(!root.join("custom.json.lock").exists());
    no_staging(root);
}

#[test]
fn literal_paths_allow_other_cwd_absolute_and_outside_project_inputs() {
    let root = support::tempdir();
    let root = root.path();
    fs::create_dir(root.join("project")).unwrap();
    fs::create_dir(root.join("caller")).unwrap();
    fs::write(root.join("outside"), "OUTSIDE_PRIVATE").unwrap();
    fs::write(root.join("absolute"), "ABS_PRIVATE").unwrap();
    fs::write(root.join("project/$literal"), "LITERAL_PRIVATE").unwrap();
    let absolute = root.join("absolute");
    config(
        &root.join("project"),
        json!({
            "../outside":file("manual", "old"),
            absolute.to_str().unwrap():file("manual", "old"),
            "$literal":file("manual", "old")
        }),
    );
    let output = command(root)
        .current_dir(root.join("caller"))
        .env("PSST_CONFIG", root.join("project/custom.json"))
        .args(["--files-push", "--yes"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert_eq!(report(&output).matches(": saved").count(), 3);
    let loaded: Value =
        serde_json::from_slice(&fs::read(root.join("project/custom.json")).unwrap()).unwrap();
    assert_eq!(loaded["../outside"]["value"], "OUTSIDE_PRIVATE");
    assert_eq!(loaded[absolute.to_str().unwrap()]["value"], "ABS_PRIVATE");
}

#[test]
fn unsafe_nonregular_invalid_and_protected_aliases_fail_individually() {
    let root = support::tempdir();
    let root = root.path();
    fs::write(root.join("good"), "NEW_PRIVATE").unwrap();
    fs::write(root.join("invalid"), [0xff]).unwrap();
    fs::write(root.join("nul"), b"PRIVATE\0").unwrap();
    fs::create_dir(root.join("directory")).unwrap();
    symlink(root.join("good"), root.join("link")).unwrap();
    symlink(root, root.join("ancestor")).unwrap();
    fs::create_dir(root.join(".psst")).unwrap();
    fs::write(root.join(".psst/record"), "STORAGE_PRIVATE").unwrap();
    fs::hard_link(root.join(".psst/record"), root.join("storage-alias")).unwrap();
    fs::write(root.join("fifo-placeholder"), "").unwrap();
    let fifo = std::ffi::CString::new(root.join("fifo").to_str().unwrap()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    let mut value = json!({"good":file("manual", "old")});
    for name in [
        "invalid",
        "nul",
        "directory",
        "link",
        "ancestor/good",
        "ancestor/../good",
        ".psst/record",
        "storage-alias",
        "config-alias",
        "custom.json",
        "missing",
        "fifo",
    ] {
        value[name] = file("manual", "old");
    }
    config(root, value);
    fs::hard_link(root.join("custom.json"), root.join("config-alias")).unwrap();
    let output = command(root)
        .args(["--files-push", "--yes"])
        .assert()
        .failure()
        .get_output()
        .clone();
    let text = report(&output);
    assert!(text.contains("good: saved"));
    assert_eq!(text.matches(": failed").count(), 12);
    assert_eq!(
        fs::read(root.join(".psst/record")).unwrap(),
        b"STORAGE_PRIVATE"
    );
    no_staging(root);
}

#[test]
fn input_limit_accepts_exact_boundary_and_refuses_one_byte_over() {
    let root = support::tempdir();
    let root = root.path();
    for len in [4_194_304, 4_194_305] {
        fs::write(root.join("one"), vec![b'x'; len]).unwrap();
        let original = config(root, json!({"one":file("manual", "old")}));
        let output = command(root)
            .args(["--files-push", "--yes"])
            .assert()
            .failure()
            .get_output()
            .clone();
        let text = report(&output);
        // Exact input boundary passes planning but exceeds persisted JSON size.
        assert_eq!(text.contains("Push local → provider:"), len == 4_194_304);
        assert_eq!(fs::read(root.join("custom.json")).unwrap(), original);
        no_staging(root);
    }
}

#[test]
fn aggregate_limit_counts_aliases_before_deduplication_and_aborts_without_staging() {
    let root = support::tempdir();
    let root = root.path();
    fs::write(root.join("shared"), vec![b'x'; 4_194_304]).unwrap();
    for count in [16, 17] {
        let mut mappings = json!({});
        for index in 0..count {
            let name = format!("alias{index}");
            if !root.join(&name).exists() {
                fs::hard_link(root.join("shared"), root.join(&name)).unwrap();
            }
            mappings[&name] = file("manual", "old");
        }
        let original = config(root, mappings);
        let output = command(root)
            .args(["--files-push", "--yes"])
            .assert()
            .failure()
            .get_output()
            .clone();
        let text = report(&output);
        assert_eq!(text.contains("Push local → provider:"), count == 16);
        assert_eq!(text.contains("batch exceeds 64 MiB"), count == 17);
        assert_eq!(text.matches(": failed").count(), count);
        assert_eq!(fs::read(root.join("custom.json")).unwrap(), original);
        no_staging(root);
    }
}

#[test]
fn push_rejects_incompatible_modes_but_preserves_wrapped_arguments() {
    let root = support::tempdir();
    let root = root.path();
    for other in ["--files-pull", "--write-files", "--copy=NAME", "--write"] {
        for argv in [["--files-push", other], [other, "--files-push"]] {
            command(root).args(argv).assert().failure();
        }
    }
    for argv in [
        vec!["--files-push", "sh"],
        vec!["--files-push", "--", "sh"],
        vec!["--files-push", "--copy", "NAME"],
    ] {
        command(root).args(argv).assert().failure();
    }
    config(root, json!({}));
    for prefix in [vec!["sh"], vec!["--", "sh"]] {
        command(root)
            .args(prefix)
            .args([
                "-c",
                "test \"$1\" = --files-push && test \"$2\" = --yes",
                "sh",
                "--files-push",
                "--yes",
            ])
            .assert()
            .success();
    }
    let output = command(root)
        .args(["--files-push", "--help"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(report(&output).contains("local → provider"));
}
