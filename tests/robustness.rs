#![cfg(unix)]
use psst::{config::Config, process};
use std::{
    ffi::OsString,
    fs,
    os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    time::Duration,
};

#[tokio::test]
async fn provider_output_is_bounded_utf8_and_nul_free() {
    let dir = tempfile::tempdir().unwrap();
    for command in [
        "printf '\\377'",
        "printf '\\000'",
        "head -c 4194305 /dev/zero | tr '\\000' x",
        "exit 4",
    ] {
        assert!(
            process::output("/bin/sh", &["-c", command], dir.path())
                .await
                .is_err(),
            "accepted {command}"
        );
    }
    assert_eq!(
        process::output("/bin/sh", &["-c", "printf '  trimmed \\n'"], dir.path())
            .await
            .unwrap()
            .as_str(),
        "trimmed"
    );
}
#[test]
fn malformed_config_diagnostics_never_contain_literal_values() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("psst.json");
    for data in [
        "{\"X\":\"DO_NOT_LOG_THIS_SECRET\"}",
        "{\"X\":{\"source\":\"manual\",\"value\":\"a\"},\"X\":{\"source\":\"manual\",\"value\":\"b\"}}",
    ] {
        fs::write(&path, data).unwrap();
        let e = match Config::load(path.clone()) {
            Ok(_) => panic!("accepted invalid config"),
            Err(e) => e,
        };
        assert!(!format!("{e:#}").contains("DO_NOT_LOG_THIS_SECRET"));
    }
}
#[test]
fn child_receives_native_argv_stdin_and_overridden_path() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("psst.json");
    fs::write(
        &config,
        format!(
            "{{\"PATH\":{{\"source\":\"manual\",\"value\":{:?}}}}}",
            dir.path().to_str().unwrap()
        ),
    )
    .unwrap();
    let program = dir.path().join("fixture-child");
    fs::write(
        &program,
        "#!/bin/sh\nprintf '%s' \"$1\"\nwhile IFS= read -r line; do printf '|%s' \"$line\"; done\n",
    )
    .unwrap();
    fs::set_permissions(program, fs::Permissions::from_mode(0o700)).unwrap();
    let arg = OsString::from_vec(vec![b'x', 0xff, b'y']);
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("psst"));
    command
        .env("PSST_CONFIG", config)
        .arg("fixture-child")
        .arg(arg)
        .write_stdin("input bytes\n")
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .stdout(vec![
            b'x', 0xff, b'y', b'|', b'i', b'n', b'p', b'u', b't', b' ', b'b', b'y', b't', b'e',
            b's',
        ]);
}
