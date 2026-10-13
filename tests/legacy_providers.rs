//! User-facing equivalents of main's injection and provider assertions.
#![cfg(unix)]
use psst::{config::Entry, providers};
use serde_json::json;
use std::{fs, os::unix::fs::PermissionsExt};

#[test]
fn legacy_source_identifiers() {
    assert_eq!(
        &providers::sources()[..5],
        &["op", "op 2FA", "vault", "shell", "manual"]
    );
}

fn entry(source: &str, value: &str) -> Entry {
    Entry {
        source: source.into(),
        value: value.into(),
    }
}
#[tokio::test]
async fn manual_nonempty_is_unchanged_and_empty_fails() {
    let dir = tempfile::tempdir().unwrap();
    for value in ["secret123", "manual-secret", " whitespace \n stays "] {
        assert_eq!(
            providers::fetch(&entry("manual", value), dir.path())
                .await
                .unwrap()
                .as_str(),
            value
        );
    }
    assert!(
        providers::fetch(&entry("manual", ""), dir.path())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn shell_echo_and_missing_command() {
    let dir = tempfile::tempdir().unwrap();
    for (command, expected) in [("echo 1", "1"), ("echo 'shell-secret'", "shell-secret")] {
        assert_eq!(
            providers::fetch(&entry("shell", command), dir.path())
                .await
                .unwrap()
                .as_str(),
            expected
        );
    }
    assert!(
        providers::fetch(
            &entry("shell", "invalid-command-that-does-not-exist"),
            dir.path()
        )
        .await
        .is_err()
    );
}
#[test]
fn injected_op_shell_manual_and_partial_failure() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("psst.json");
    let op = dir.path().join("op");
    fs::write(
        &op,
        "#!/bin/sh\n[ \"$1\" = read ] || exit 2\nprintf '%s' \"$2\"\n",
    )
    .unwrap();
    fs::set_permissions(&op, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(
        &config,
        json!({
            "OP_SECRET": {"source":"op", "value":"op://vault/item/field"},
            "SHELL_SECRET": {"source":"shell", "value":"echo 'shell-secret'"},
            "MANUAL_SECRET": {"source":"manual", "value":"manual-secret"},
            "BAD_SECRET": {"source":"op", "value":"invalid-op-reference"}
        })
        .to_string(),
    )
    .unwrap();
    let mut cmd = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("psst"));
    cmd.env_remove("SSH_AUTH_SOCK").env("PSST_CONFIG", &config).env("PATH", dir.path()).env_remove("BAD_SECRET").args(["/bin/sh", "-c", "[ \"${BAD_SECRET+x}\" != x ] || exit 8; printf '%s|%s|%s' \"$OP_SECRET\" \"$SHELL_SECRET\" \"$MANUAL_SECRET\""]).assert().success().stdout("op://vault/item/field|shell-secret|manual-secret");
    // Empty config overlays nothing and preserves the inherited environment.
    fs::write(&config, "{}").unwrap();
    let mut cmd = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("psst"));
    cmd.env_remove("SSH_AUTH_SOCK")
        .env("PSST_CONFIG", &config)
        .env("UNCHANGED", "inherited")
        .args(["/bin/sh", "-c", "printf '%s' \"$UNCHANGED\""])
        .assert()
        .success()
        .stdout("inherited");
}
#[test]
fn op_echo_read_argv_missing_executable_and_invalid_reference() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("psst.json");
    let op = dir.path().join("op");
    fs::write(
        &config,
        json!({"SECRET":{"source":"op","value":"op://vault/item/field"}}).to_string(),
    )
    .unwrap();
    // main's internal executable override was echo; emulate that with a fake op.
    fs::write(&op, "#!/bin/sh\nprintf '%s' \"$*\"\n").unwrap();
    fs::set_permissions(&op, fs::Permissions::from_mode(0o700)).unwrap();
    let mut cmd = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("psst"));
    cmd.env_remove("SSH_AUTH_SOCK")
        .env("PSST_CONFIG", &config)
        .env("PATH", dir.path())
        .args(["/bin/sh", "-c", "printf '%s' \"$SECRET\""])
        .assert()
        .success()
        .stdout("read op://vault/item/field");
    fs::remove_file(op).unwrap();
    let mut cmd = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("psst"));
    let result = cmd
        .env_remove("SSH_AUTH_SOCK")
        .env("PSST_CONFIG", &config)
        .env("PATH", dir.path())
        .env_remove("SECRET")
        .args(["/bin/sh", "-c", "[ \"${SECRET+x}\" != x ]"])
        .assert()
        .success();
    assert!(String::from_utf8_lossy(&result.get_output().stderr).contains("Cannot start op"));
    fs::write(
        &config,
        json!({"SECRET":{"source":"op","value":"invalid-path"}}).to_string(),
    )
    .unwrap();
    let mut cmd = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("psst"));
    let result = cmd
        .env_remove("SSH_AUTH_SOCK")
        .env("PSST_CONFIG", &config)
        .env("PATH", dir.path())
        .env_remove("SECRET")
        .args(["/bin/sh", "-c", "[ \"${SECRET+x}\" != x ]"])
        .assert()
        .success();
    assert!(String::from_utf8_lossy(&result.get_output().stderr).contains("must start with op://"));
}
