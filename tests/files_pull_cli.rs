#![cfg(unix)]

use std::{
    ffi::OsString,
    fs,
    os::unix::{ffi::OsStringExt, fs::PermissionsExt, fs::symlink},
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
        .timeout(Duration::from_secs(10));
    command
}

fn private(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

#[test]
fn pull_preserves_empty_unchanged_consent_and_safety_behavior() {
    {
        let flag = "--files-pull";
        let root = support::tempdir();
        let root = root.path();
        fs::write(root.join("custom.json"), "{}").unwrap();
        let output = command(root)
            .arg(flag)
            .assert()
            .success()
            .get_output()
            .clone();
        assert!(String::from_utf8_lossy(&output.stderr).contains("No managed files"));

        private(&root.join("same"), b"SAME_PRIVATE");
        private(&root.join("replace"), b"OLD_PRIVATE");
        private(&root.join("target"), b"TARGET_PRIVATE");
        symlink(root.join("target"), root.join("unsafe")).unwrap();
        fs::write(
            root.join("custom.json"),
            json!({
                "ENV": {"source":"shell", "value":"touch env-fetched"},
                "same": {"type":"file", "source":"manual", "value":"SAME_PRIVATE"},
                "replace": {"type":"file", "source":"manual", "value":"NEW_PRIVATE"},
                "new/deep": {"type":"file", "source":"manual", "value":"NEW_PRIVATE"},
                "empty": {"type":"file", "source":"shell", "value":"printf ''"},
                "unsafe": {"type":"file", "source":"manual", "value":"UNSAFE_PRIVATE"},
                "failed": {"type":"file", "source":"unknown", "value":"HIDDEN_PRIVATE"}
            })
            .to_string(),
        )
        .unwrap();
        let output = command(root)
            .arg(flag)
            .assert()
            .failure()
            .get_output()
            .clone();
        let report = String::from_utf8_lossy(&output.stderr);
        assert!(report.contains("Replacement requires interactive consent or --yes"));
        assert_eq!(fs::read(root.join("replace")).unwrap(), b"OLD_PRIVATE");
        assert!(!root.join("new").exists() && !root.join("empty").exists());

        let output = command(root)
            .args([flag, "--yes"])
            .assert()
            .failure()
            .get_output()
            .clone();
        let report = String::from_utf8_lossy(&output.stderr);
        for outcome in [
            "same: unchanged",
            "replace: written",
            "deep: written",
            "empty: written",
            "unsafe: failed",
            "failed: failed",
        ] {
            assert!(report.contains(outcome), "{flag}: {report}");
        }
        assert!(!report.contains("PRIVATE"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE"));
        assert_eq!(fs::read(root.join("replace")).unwrap(), b"NEW_PRIVATE");
        assert_eq!(fs::read(root.join("new/deep")).unwrap(), b"NEW_PRIVATE");
        assert_eq!(fs::read(root.join("empty")).unwrap(), b"");
        assert_eq!(fs::read(root.join("target")).unwrap(), b"TARGET_PRIVATE");
        assert!(!root.join("failed").exists() && !root.join("env-fetched").exists());
        assert_eq!(
            fs::metadata(root.join("new")).unwrap().permissions().mode() & 0o777,
            0o700
        );
        for path in ["replace", "new/deep", "empty"] {
            assert_eq!(
                fs::metadata(root.join(path)).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }

        // An entirely unchanged batch succeeds without approval.
        fs::write(
            root.join("custom.json"),
            json!({
                "same": {"type":"file", "source":"manual", "value":"SAME_PRIVATE"}
            })
            .to_string(),
        )
        .unwrap();
        let output = command(root)
            .arg(flag)
            .assert()
            .success()
            .get_output()
            .clone();
        let report = String::from_utf8_lossy(&output.stderr);
        assert!(report.contains("same: unchanged") && !report.contains("Replace all?"));
        assert!(fs::read_dir(root).unwrap().all(|entry| {
            let name = entry.unwrap().file_name();
            !name.to_string_lossy().starts_with("psst-files-")
        }));
    }
}

#[test]
fn pull_keeps_op_trimming_and_empty_read_failures() {
    {
        let flag = "--files-pull";
        let root = support::tempdir();
        let root = root.path();
        let bin = root.join("bin");
        fs::create_dir(&bin).unwrap();
        let op = bin.join("op");
        fs::write(&op, "#!/bin/sh\n[ \"$1\" = read ] || exit 9\ncase \"$2\" in\n op://vault/item/text) printf '  TRIMMED_PRIVATE \\n\\n';;\n op://vault/item/empty) printf ' \\n';;\n *) exit 8;;\nesac\n").unwrap();
        fs::set_permissions(&op, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(
            root.join("custom.json"),
            json!({
                "trimmed": {"type":"file", "source":"op", "value":"op://vault/item/text"},
                "empty": {"type":"file", "source":"op", "value":"op://vault/item/empty"},
                "failure": {"type":"file", "source":"op", "value":"op://vault/item/missing"},
                "independent": {"type":"file", "source":"manual", "value":"INDEPENDENT_PRIVATE"}
            })
            .to_string(),
        )
        .unwrap();
        let output = command(root)
            .env("PATH", &bin)
            .arg(flag)
            .assert()
            .failure()
            .get_output()
            .clone();
        let report = String::from_utf8_lossy(&output.stderr);
        for outcome in [
            "trimmed: written",
            "empty: failed",
            "failure: failed",
            "independent: written",
        ] {
            assert!(report.contains(outcome), "{report}");
        }
        assert!(!report.contains("PRIVATE"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE"));
        assert_eq!(fs::read(root.join("trimmed")).unwrap(), b"TRIMMED_PRIVATE");
        assert_eq!(
            fs::read(root.join("independent")).unwrap(),
            b"INDEPENDENT_PRIVATE"
        );
        assert!(!root.join("empty").exists() && !root.join("failure").exists());
    }
}

#[test]
fn help_and_invalid_combinations_are_directional_and_do_not_load_config() {
    let root = support::tempdir();
    // Invalid configuration proves help and usage errors precede any provider work.
    fs::write(root.path().join("custom.json"), "invalid JSON").unwrap();
    {
        let flag = "--files-pull";
        for argv in [vec![flag, "--help"], vec!["--help", flag]] {
            let output = command(root.path())
                .args(argv)
                .assert()
                .success()
                .get_output()
                .clone();
            let help = String::from_utf8_lossy(&output.stdout);
            for text in ["--files-pull [--yes]", "provider → local"] {
                assert!(help.contains(text), "{help}");
            }
            assert!(!help.contains("--write-files"));
            assert!(help.contains("--files-push [--yes]") && help.contains("local → provider"));
        }
        for argv in [
            vec![flag, "--write"],
            vec!["--write", flag],
            vec![flag, "--copy", "NAME"],
            vec!["--copy", "NAME", flag],
            vec![flag, "--copy=NAME"],
            vec!["--copy=NAME", flag],
            vec![flag, "--files-pull"],
            vec![flag, "--write-files"],
            vec![flag, "command"],
            vec![flag, "--", "command"],
        ] {
            let output = command(root.path())
                .args(&argv)
                .assert()
                .failure()
                .get_output()
                .clone();
            let report = String::from_utf8_lossy(&output.stderr);
            assert!(
                report.contains("Usage: psst --files-pull [--yes]"),
                "{argv:?}: {report}"
            );
            assert!(report.contains("provider → local"));
            assert!(report.contains("cannot be combined"));
            assert!(!report.contains("JSON"));
        }
    }
}

#[test]
fn wrapped_flags_and_non_utf8_native_argv_are_untouched() {
    let root = support::tempdir();
    fs::write(root.path().join("custom.json"), "{}").unwrap();
    for separator in [false, true] {
        let mut cmd = command(root.path());
        if separator {
            cmd.arg("--");
        }
        let output = cmd
            .args([
                "/bin/sh",
                "-c",
                "printf '%s\\000' \"$@\"",
                "sh",
                "--files-pull",
                "--write-files",
                "--yes",
                "--copy",
                "--write",
                "--help",
                "--version",
            ])
            .arg(OsString::from_vec(b"native-\xff".to_vec()))
            .assert()
            .success()
            .get_output()
            .clone();
        assert_eq!(output.stdout, b"--files-pull\0--write-files\0--yes\0--copy\0--write\0--help\0--version\0native-\xff\0");
    }
}
