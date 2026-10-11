#![cfg(unix)]
use assert_cmd::Command;
use serde_json::json;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command as StdCommand, Stdio},
    time::Duration,
};

fn binary() -> PathBuf {
    assert_cmd::cargo::cargo_bin!("psst").to_owned()
}
struct Fixture {
    temp: tempfile::TempDir,
    bin: PathBuf,
    config: PathBuf,
}
impl Fixture {
    fn new(config: serde_json::Value) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let bin = temp.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let path = temp.path().join("psst.json");
        fs::write(&path, config.to_string()).unwrap();
        Self {
            temp,
            bin,
            config: path,
        }
    }
    fn script(&self, name: &str, text: &str) {
        let path = self.bin.join(name);
        fs::write(&path, format!("#!/bin/sh\n{text}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    fn command(&self) -> Command {
        let mut c = Command::new(binary());
        c.current_dir(self.temp.path())
            .env("PSST_CONFIG", &self.config)
            .env(
                "PATH",
                format!("{}:{}", self.bin.display(), std::env::var("PATH").unwrap()),
            )
            .env_remove("WAYLAND_DISPLAY");
        c.timeout(Duration::from_secs(10));
        c
    }
}
#[test]
fn overlay_partial_failures_argv_and_exit() {
    let f = Fixture::new(json!({
        "OK": {"source":"manual", "value":"literal $() ' \""},
        "FAILED": {"source":"shell", "value":"exit 9"},
        "EMPTY": {"source":"shell", "value":"printf '  '"},
        "UNKNOWN": {"source":"what", "value":"not printed"}
    }));
    f.command()
        .env("FAILED", "inherited")
        .args([
            "/bin/sh",
            "-c",
            "printf '%s|%s|%s|%s' \"$OK\" \"$FAILED\" \"$EMPTY\" \"$1\"; exit 7",
            "_",
            "a b; --write --help",
        ])
        .assert()
        .code(7)
        .stdout("literal $() ' \"|inherited||a b; --write --help");
    f.command()
        .args(["this-executable-does-not-exist"])
        .assert()
        .code(1);
}
#[test]
fn config_cwd_original_environment_and_caller_cwd() {
    let f = Fixture::new(json!({
        "A": {"source":"manual", "value":"injected"},
        "B": {"source":"shell", "value":"printf '%s/%s' \"$A\" \"$PWD\""}
    }));
    let caller = f.temp.path().join("caller");
    fs::create_dir(&caller).unwrap();
    // Shell PWD resolves symlinks such as macOS /var -> /private/var.
    let expected = format!(
        "original/{}|{}",
        f.temp.path().canonicalize().unwrap().display(),
        caller.canonicalize().unwrap().display()
    );
    f.command()
        .current_dir(&caller)
        .env("A", "original")
        .args(["/bin/sh", "-c", "printf '%s|%s' \"$B\" \"$PWD\""])
        .assert()
        .success()
        .stdout(expected);
}
#[test]
fn four_fetches_are_concurrent_through_the_cli() {
    let mut entries = serde_json::Map::new();
    for n in 0..4 {
        entries.insert(format!("S{n}"), json!({"source":"shell", "value":format!("printf x > ready{n}; while [ ! -f ready0 ] || [ ! -f ready1 ] || [ ! -f ready2 ] || [ ! -f ready3 ]; do sleep 0.01; done; printf result{n}")}));
    }
    let f = Fixture::new(entries.into());
    f.command()
        .env("PSST_TIMEOUT", "3")
        .args([
            "/bin/sh",
            "-c",
            "printf '%s,%s,%s,%s' \"$S0\" \"$S1\" \"$S2\" \"$S3\"",
        ])
        .assert()
        .success()
        .stdout("result0,result1,result2,result3");
}
#[test]
fn op_vault_and_terraform_use_literal_argv() {
    let f = Fixture::new(json!({
        "OP": {"source":"op", "value":"op://v/$(touch BAD)/field"},
        "VAULT": {"source":"vault", "value":"/secret/a#object"},
        "OTP": {"source":"op 2FA", "value":"op://v/otp/seed"},
        "TF": {"source":"terraform", "value":"tf#zero"}
    }));
    fs::create_dir(f.temp.path().join("tf")).unwrap();
    f.script("op", "[ \"$1\" = read ] || exit 9\ncase \"$2\" in\n 'op://v/$(touch BAD)/field') printf opvalue;;\n 'op://v/otp/seed') printf 'otpauth://totp/demo?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ';;\n *) exit 10;; esac");
    f.script("vault", "[ \"$1|$2|$3|$4\" = 'kv|get|-format=json|/secret/a' ] || exit 9\nprintf '%s' '{\"data\":{\"data\":{\"object\":{\"a\":true}}}}'");
    f.script("terraform", "[ \"$1|$2\" = 'output|-json' ] || exit 9\n[ \"${PWD##*/}\" = tf ] || exit 8\nprintf '%s' '{\"zero\":{\"value\":0}}'");
    f.command()
        .args([
            "/bin/sh",
            "-c",
            "[ ${#OTP} = 6 ] || exit 6; printf '%s|%s|%s' \"$OP\" \"$VAULT\" \"$TF\"",
        ])
        .assert()
        .success()
        .stdout("opvalue|{\"a\":true}|0");
    assert!(!f.temp.path().join("BAD").exists());
}
#[test]
fn terraform_is_available_by_default() {
    let f = Fixture::new(json!({
        "TF": {"source": "terraform", "value": ".#key"}
    }));
    f.script(
        "terraform",
        "printf '%s' '{\"key\":{\"value\":\"resolved\"}}'",
    );
    f.command()
        .env("TF", "inherited")
        .args(["/bin/sh", "-c", "printf '%s' \"$TF\""])
        .assert()
        .success()
        .stdout("resolved");
}
#[test]
fn clipboard_preserves_literal_bytes_without_executing() {
    let secret = "quotes \" ' $HOME $(touch BAD) `touch BAD`\nsecond line";
    let f = Fixture::new(json!({"KEY":{"source":"manual", "value":secret}}));
    if cfg!(target_os = "macos") {
        f.script("pbcopy", "[ $# = 0 ] || exit 9\ncat > \"$CAPTURE\"");
    } else {
        f.script(
            "xclip",
            "[ \"$1|$2\" = '-selection|clipboard' ] || exit 9\ncat > \"$CAPTURE\"",
        );
    }
    let capture = f.temp.path().join("copied");
    f.command()
        .env("CAPTURE", &capture)
        .args(["--copy", "KEY"])
        .assert()
        .success()
        .stdout("");
    assert_eq!(fs::read_to_string(capture).unwrap(), secret);
    assert!(!f.temp.path().join("BAD").exists());
    f.command().args(["--copy", "MISSING"]).assert().failure();
}
#[test]
fn export_is_private_ordered_and_escaped() {
    let f = Fixture::new(
        json!({"A":{"source":"manual","value":"a\n\"b\\c $HOME #literal"},"B":{"source":"manual","value":"plain"},"EMPTY":{"source":"shell","value":"printf ''"}}),
    );
    let caller = f.temp.path().join("caller");
    fs::create_dir(&caller).unwrap();
    f.command()
        .current_dir(&caller)
        .arg("--write")
        .assert()
        .success()
        .stdout("");
    let path = caller.join(".env");
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("A=\"a\\n\\\"b\\\\c"));
    assert!(text.ends_with("B=plain\n"));
    assert!(!text.contains("EMPTY="));
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(!f.temp.path().join(".env").exists());
}
#[test]
fn help_version_and_downstream_options() {
    let f = Fixture::new(json!({}));
    f.command()
        .env("PSST_CONFIG", "/unreadable/missing")
        .arg("--help")
        .assert()
        .success();
    f.command()
        .arg("--version")
        .assert()
        .success()
        .stdout("psst 2.0.0-beta.1\n");
    f.script("capture", "printf '%s|%s|%s' \"$1\" \"$2\" \"$3\"");
    f.command()
        .args(["capture", "--copy", "--write", "--help"])
        .assert()
        .success()
        .stdout("--copy|--write|--help");
    f.command()
        .args(["--version", "--copy"])
        .assert()
        .success()
        .stdout("psst 2.0.0-beta.1\n");
    f.command().arg("--copy").assert().failure();
    f.command().assert().failure(); // non-terminal UI fails clearly instead of hanging
}
#[test]
fn timeout_kills_descendants_and_keeps_best_effort_behavior() {
    let f = Fixture::new(
        json!({"BAD":{"source":"shell", "value":"sleep 20 & printf '%s' $! > descendant; wait"},"GOOD":{"source":"manual", "value":"ok"}}),
    );
    f.command()
        .env("PSST_TIMEOUT", "1")
        .args(["/bin/sh", "-c", "printf '%s' \"$GOOD\""])
        .assert()
        .success()
        .stdout("ok");
    let pid: i32 = fs::read_to_string(f.temp.path().join("descendant"))
        .unwrap()
        .parse()
        .unwrap();
    assert_process_dead(pid);
}
fn assert_process_dead(pid: i32) {
    // Orphaned grandchildren may briefly be zombies until PID 1 reaps them.
    for _ in 0..100 {
        if unsafe { libc::kill(pid, 0) } < 0 {
            return;
        }
        if fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|s| {
            s.split(')')
                .nth(1)
                .is_some_and(|s| s.trim_start().starts_with('Z'))
        }) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("descendant {pid} survived cancellation");
}
fn wait_for(path: &Path) {
    for _ in 0..200 {
        if path.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("fixture did not start");
}
#[test]
fn signals_are_preserved_after_exec_and_cancel_fetches() {
    let f = Fixture::new(json!({}));
    let ready = f.temp.path().join("ready");
    let mut child = StdCommand::new(binary())
        .env("PSST_CONFIG", &f.config)
        .args(["/bin/sh", "-c", "printf ready > \"$READY\"; exec sleep 20"])
        .env("READY", &ready)
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for(&ready);
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(child.wait().unwrap().signal(), Some(libc::SIGTERM));
    fs::write(
        &f.config,
        json!({"A":{"source":"shell","value":"printf '%s' $$ > fetching; sleep 20"}}).to_string(),
    )
    .unwrap();
    let mut child = StdCommand::new(binary())
        .current_dir(f.temp.path())
        .env("PSST_CONFIG", &f.config)
        .arg("true")
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for(&f.temp.path().join("fetching"));
    let pid = fs::read_to_string(f.temp.path().join("fetching"))
        .unwrap()
        .parse()
        .unwrap();
    unsafe {
        libc::kill(child.id() as i32, libc::SIGINT);
    }
    assert_eq!(child.wait().unwrap().code(), Some(130));
    assert_process_dead(pid);
}
#[test]
fn decoded_json_nul_is_a_fetch_failure_not_a_launch_failure() {
    let f = Fixture::new(json!({
        "VAULT_BAD": {"source":"vault", "value":"/secret#key"},
        "TF_BAD": {"source":"terraform", "value":".#key"},
        "GOOD": {"source":"manual", "value":"ok"}
    }));
    f.script(
        "vault",
        "printf '%s' '{\"data\":{\"data\":{\"key\":\"a\\u0000b\"}}}'",
    );
    f.script(
        "terraform",
        "printf '%s' '{\"key\":{\"value\":\"a\\u0000b\"}}'",
    );
    let output = f
        .command()
        .env("VAULT_BAD", "inherited-vault")
        .env("TF_BAD", "inherited-tf")
        .args([
            "/bin/sh",
            "-c",
            "printf '%s|%s|%s' \"$GOOD\" \"$VAULT_BAD\" \"$TF_BAD\"",
        ])
        .assert()
        .success()
        .stdout("ok|inherited-vault|inherited-tf");
    assert_eq!(
        String::from_utf8_lossy(&output.get_output().stderr)
            .matches("Resolved secret contains a NUL byte")
            .count(),
        2
    );
}
#[test]
fn clipboard_timeout_and_cancellation_kill_descendants() {
    for cancel in [false, true] {
        let f = Fixture::new(json!({"KEY":{"source":"manual", "value":"literal"}}));
        let helper = if cfg!(target_os = "macos") {
            "pbcopy"
        } else {
            "xclip"
        };
        f.script(helper, "sleep 20 & printf '%s' $! > \"$PIDFILE\"; wait");
        let pidfile = f.temp.path().join("clipboard-descendant");
        if cancel {
            let mut child = StdCommand::new(binary())
                .env("PSST_CONFIG", &f.config)
                .env(
                    "PATH",
                    format!("{}:{}", f.bin.display(), std::env::var("PATH").unwrap()),
                )
                .env_remove("WAYLAND_DISPLAY")
                .env("PSST_TIMEOUT", "30")
                .env("PIDFILE", &pidfile)
                .args(["--copy", "KEY"])
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            wait_for(&pidfile);
            unsafe {
                libc::kill(child.id() as i32, libc::SIGINT);
            }
            assert_eq!(child.wait().unwrap().code(), Some(130));
        } else {
            f.command()
                .env("PIDFILE", &pidfile)
                .env("PSST_TIMEOUT", "1")
                .args(["--copy", "KEY"])
                .assert()
                .failure();
        }
        let pid = fs::read_to_string(pidfile).unwrap().parse().unwrap();
        assert_process_dead(pid);
    }
}
#[test]
fn successful_clipboard_helper_retains_its_owner() {
    let f = Fixture::new(json!({"KEY":{"source":"manual", "value":"literal"}}));
    let helper = if cfg!(target_os = "macos") {
        "pbcopy"
    } else {
        "xclip"
    };
    f.script(
        helper,
        "cat > /dev/null; (sleep 0.2; printf ready > \"$MARKER\") &",
    );
    let marker = f.temp.path().join("clipboard-owner");
    f.command()
        .env("MARKER", &marker)
        .args(["--copy", "KEY"])
        .assert()
        .success();
    wait_for(&marker);
}
#[test]
fn malformed_configs_are_errors_not_empty_environments() {
    let f = Fixture::new(json!({}));
    for bytes in [
        "{broken",
        "[]",
        "null",
        "{\"BAD-NAME\":{\"source\":\"manual\",\"value\":\"x\"}}",
        "{\"X\":{\"source\":2,\"value\":false}}",
    ] {
        fs::write(&f.config, bytes).unwrap();
        f.command().arg("true").assert().failure();
    }
}
