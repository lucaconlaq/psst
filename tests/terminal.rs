#![cfg(unix)]
use std::{
    fs::{self, File},
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{fs::PermissionsExt, process::CommandExt},
    },
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

#[allow(dead_code)]
mod support;

struct Pty {
    master: File,
    _slave: File,
    child: Child,
    output: String,
    history: String,
    before: libc::termios,
    parser: vt100::Parser,
}
impl Pty {
    fn start(config: &Path, bin: Option<&Path>) -> Self {
        Self::start_with_agent(config, bin, None)
    }
    fn start_with_agent(config: &Path, bin: Option<&Path>, socket: Option<&Path>) -> Self {
        Self::start_with_timeout(config, bin, socket, 30)
    }
    fn start_with_timeout(
        config: &Path,
        bin: Option<&Path>,
        socket: Option<&Path>,
        timeout: u64,
    ) -> Self {
        let mut master = -1;
        let mut slave = -1;
        let mut size = libc::winsize {
            ws_row: 30,
            ws_col: 120,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &raw mut size,
                )
            },
            0
        );
        let master = unsafe { File::from_raw_fd(master) };
        // Poll readiness does not guarantee that a subsequent PTY read won't block.
        let flags = unsafe { libc::fcntl(master.as_raw_fd(), libc::F_GETFL) };
        assert!(flags >= 0);
        assert_eq!(
            unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) },
            0
        );
        let slave = unsafe { File::from_raw_fd(slave) };
        let before = attrs(&master);
        let mut command = Command::new(assert_cmd::cargo::cargo_bin!("psst"));
        command
            .env_remove("SSH_AUTH_SOCK")
            .env("PSST_CONFIG", config)
            .env("PSST_TIMEOUT", timeout.to_string())
            .env("TERM", "xterm-256color")
            .stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap()))
            .stderr(Stdio::from(slave.try_clone().unwrap()));
        if let Some(socket) = socket {
            command.env("SSH_AUTH_SOCK", socket);
        }
        if let Some(bin) = bin {
            command.env(
                "PATH",
                format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
            );
        }
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().unwrap();
        Self {
            master,
            _slave: slave,
            child,
            output: String::new(),
            history: String::new(),
            before,
            parser: vt100::Parser::new(30, 120, 0),
        }
    }
    fn encrypted_draft(&mut self, id: &str) {
        self.send(b"a");
        self.wait_text("Environment name");
        // The normal draft starts with manual; advance to local-storage.
        self.send(b"\t\x1b[C\x1b[C\x0e");
        self.wait_text("Create encrypted value");
        self.wait_text("Masked exact text");
        self.send(format!("\t\x1b[200~{id}\x1b[201~\t").as_bytes());
        self.wait_text(id);
    }
    fn read(&mut self) -> usize {
        let mut poll = libc::pollfd {
            fd: self.master.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        if unsafe { libc::poll(&mut poll, 1, 20) } > 0 && poll.revents & libc::POLLIN != 0 {
            let mut buf = [0; 65536];
            if let Ok(n) = self.master.read(&mut buf) {
                self.output.push_str(&String::from_utf8_lossy(&buf[..n]));
                self.history.push_str(&String::from_utf8_lossy(&buf[..n]));
                self.parser.process(&buf[..n]);
                return n;
            }
        }
        0
    }
    fn wait_text(&mut self, text: &str) {
        self.wait_any_text(&[text]);
    }
    fn wait_any_text(&mut self, texts: &[&str]) {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            self.read();
            if texts
                .iter()
                .any(|text| self.parser.screen().contents().contains(text))
            {
                return;
            }
            if let Some(status) = self.child.try_wait().unwrap() {
                panic!("psst exited {status} before {texts:?}: {}", self.output);
            }
        }
        panic!("Missing {texts:?} in terminal output: {}", self.output);
    }
    fn wait_file(&mut self, path: &Path) {
        let start = Instant::now();
        while !path.exists() {
            // Keep draining output so a full PTY buffer cannot block the UI.
            self.read();
            if let Some(status) = self.child.try_wait().unwrap() {
                panic!("psst exited {status} before saving: {}", self.output);
            }
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "Config was not saved: {}",
                self.output
            );
        }
    }
    fn send(&mut self, bytes: &[u8]) {
        self.output.clear();
        self.master.write_all(bytes).unwrap();
        self.master.flush().unwrap();
    }
    fn finish(&mut self) {
        let start = Instant::now();
        loop {
            self.read();
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success(), "psst exited {status}: {}", self.output);
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(5), "UI did not exit");
        }
        // Process exit can precede the last read of buffered PTY output.
        loop {
            let mut poll = libc::pollfd {
                fd: self.master.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            if unsafe { libc::poll(&mut poll, 1, 0) } <= 0 || poll.revents & libc::POLLIN == 0 {
                break;
            }
            // A closed PTY can remain readable at EOF, notably on macOS.
            if self.read() == 0 {
                break;
            }
        }
        assert!(
            self.output.contains("\x1b[?1049l"),
            "alternate screen was not restored"
        );
        // macOS revokes the slave when its controlling session exits.
        let after = attrs(&self.master);
        assert_eq!(self.before.c_lflag, after.c_lflag);
        assert_eq!(self.before.c_iflag, after.c_iflag);
    }
}
impl Drop for Pty {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn attrs(file: &File) -> libc::termios {
    let mut attr = std::mem::MaybeUninit::uninit();
    assert_eq!(
        unsafe { libc::tcgetattr(file.as_raw_fd(), attr.as_mut_ptr()) },
        0,
        "tcgetattr failed: {}",
        std::io::Error::last_os_error()
    );
    unsafe { attr.assume_init() }
}
#[test]
fn actual_keys_add_save_reveal_hide_delete_and_restore_terminal() {
    let t = tempfile::tempdir().unwrap();
    let config = t.path().join(".psst.json");
    let mut pty = Pty::start(&config, None);
    pty.wait_text("No secrets yet");
    assert!(!config.exists());
    pty.send(b"a");
    pty.wait_text("Environment name");
    pty.send(b"\t\t\x1b[200~literal $() ' secret\x1b[201~"); // bracketed paste
    std::thread::sleep(Duration::from_millis(100));
    pty.send(b"\x13"); // Ctrl+S
    pty.wait_file(&config);
    pty.wait_text("Saved securely");
    let json: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    assert_eq!(json["SECRET_1"]["value"], "literal $() ' secret");
    assert!(
        !pty.parser
            .screen()
            .contents()
            .contains("literal $() ' secret")
    );
    pty.send(b" ");
    pty.wait_text("literal $() ' secret");
    pty.send(b" ");
    std::thread::sleep(Duration::from_millis(100));
    pty.read();
    assert!(
        !pty.parser
            .screen()
            .contents()
            .contains("literal $() ' secret")
    );
    pty.send(b"d");
    pty.wait_text("Confirm deletion");
    pty.send(b"\r");
    pty.wait_text("No secrets yet");
    assert_eq!(fs::read_to_string(&config).unwrap(), "{}");
    pty.send(b"q");
    pty.finish();
}
#[test]
fn editor_never_automatically_executes_shell_and_sigterm_restores_terminal() {
    let t = tempfile::tempdir().unwrap();
    let config = t.path().join("psst.json");
    let marker = t.path().join("EXECUTED");
    fs::write(&config, serde_json::json!({"SHELL":{"source":"shell","value":format!("printf x > '{}'", marker.display())}}).to_string()).unwrap();
    let mut pty = Pty::start(&config, None);
    pty.wait_text("SHELL");
    pty.send(b"\r");
    pty.wait_text("Environment name");
    assert!(!marker.exists());
    unsafe {
        libc::kill(pty.child.id() as i32, libc::SIGTERM);
    }
    pty.finish();
}
#[test]
fn vault_browser_mixed_rows_and_reference_selection() {
    let t = tempfile::tempdir().unwrap();
    let config = t.path().join("psst.json");
    fs::write(
        &config,
        "{\"KEY\":{\"source\":\"vault\",\"value\":\"/secret/old#field\"}}",
    )
    .unwrap();
    let bin = t.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let vault = bin.join("vault");
    fs::write(&vault, include_str!("terminal_vault_fixture.sh")).unwrap();
    fs::set_permissions(vault, fs::Permissions::from_mode(0o700)).unwrap();
    let mut pty = Pty::start(&config, Some(&bin));
    pty.wait_text("KEY");
    pty.send(b"\r");
    pty.wait_text("Environment name");
    pty.send(b"\x02");
    pty.wait_text("› secret/");
    assert!(!pty.parser.screen().contents().contains("pki/"));
    pty.send(b"\r");
    pty.wait_text("password");
    assert!(
        !pty.parser
            .screen()
            .contents()
            .contains("NEVER_RENDER_VAULT_VALUE")
    );
    pty.send(b"\x1b[B\r");
    pty.wait_text("Reference selected");
    pty.send(b"\x13");
    pty.wait_text("Saved securely");
    assert!(
        fs::read_to_string(&config)
            .unwrap()
            .contains("/secret#password")
    );
    pty.send(b"q");
    pty.finish();
}

#[test]
fn op_browser_search_selection_cancel_and_draft_save() {
    let t = tempfile::tempdir().unwrap();
    let config = t.path().join("psst.json");
    fs::write(
        &config,
        "{\"KEY\":{\"source\":\"op\",\"value\":\"op://old/item/field\"}}",
    )
    .unwrap();
    let bin = t.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let op = bin.join("op");
    fs::write(&op, "#!/bin/sh\ncase \"$1 $2\" in\n 'vault list') printf '%s' '[{\"id\":\"v\",\"name\":\"Work Vault\"}]';;\n 'item list') [ \"$3 $4\" = '--vault v' ] || exit 3; printf '%s' '[{\"id\":\"i\",\"title\":\"Example Item\"}]';;\n 'item get') [ \"$3 $4 $5\" = 'i --vault v' ] || exit 4; printf '%s' '{\"fields\":[{\"label\":\"Password\",\"type\":\"CONCEALED\",\"value\":\"DO_NOT_RENDER_FIELD_VALUE\",\"reference\":\"op://v/i/password\"}]}';;\n *) exit 9;; esac\n").unwrap();
    fs::set_permissions(op, fs::Permissions::from_mode(0o700)).unwrap();
    let mut pty = Pty::start(&config, Some(&bin));
    pty.wait_text("KEY");
    pty.send(b"\r");
    pty.wait_text("Environment name");
    pty.send(b"\x02");
    pty.wait_text("Work Vault");
    pty.send(b"\r");
    pty.wait_text("Example Item");
    pty.send(b"Example");
    std::thread::sleep(Duration::from_millis(100));
    pty.send(b"\r");
    pty.wait_text("› Password");
    assert!(
        !pty.parser
            .screen()
            .contents()
            .contains("DO_NOT_RENDER_FIELD_VALUE")
    );
    pty.send(b"\r");
    pty.wait_text("Reference selected");
    assert!(
        fs::read_to_string(&config)
            .unwrap()
            .contains("op://old/item/field")
    );
    pty.send(b"\x13");
    pty.wait_text("Saved securely");
    assert!(
        fs::read_to_string(&config)
            .unwrap()
            .contains("op://v/i/password")
    );
    pty.send(b"q");
    pty.finish();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_storage_opt_in_errors_refetch_and_cancellation_restore_terminal() {
    use psst::providers::local_storage::{KeyFile, StoredKey};
    use support::agent::{FakeAgent, Fault};

    let fake = FakeAgent::new().await;
    let root = std::env::temp_dir().canonicalize().unwrap();
    let dir = tempfile::tempdir_in(&root).unwrap();
    let mut stored = StoredKey::load(dir.path(), fake.key.clone()).unwrap();
    stored
        .set(&fake.client, "id", "DELIBERATE_REVEAL_ONLY")
        .await
        .unwrap();
    let config = dir.path().join("psst.json");
    fs::write(
        &config,
        r#"{"KEY":{"source":"local-storage","value":"id"}}"#,
    )
    .unwrap();
    let mut pty = Pty::start_with_agent(&config, None, Some(&fake.socket()));
    pty.wait_text("KEY");
    let before = fake.signs();
    assert!(
        !pty.parser
            .screen()
            .contents()
            .contains("DELIBERATE_REVEAL_ONLY")
    );
    fake.fault(Fault::InvalidSignature);
    pty.send(b" ");
    pty.wait_text("Invalid SSH agent signature");
    assert_eq!(fake.signs(), before + 1);
    fake.fault(Fault::None);
    for expected in [before + 2, before + 3] {
        pty.send(b" ");
        pty.wait_text("DELIBERATE_REVEAL_ONLY");
        assert_eq!(fake.signs(), expected);
        pty.send(b" ");
        std::thread::sleep(Duration::from_millis(100));
        pty.read();
        assert!(
            !pty.parser
                .screen()
                .contents()
                .contains("DELIBERATE_REVEAL_ONLY")
        );
    }
    // A cancelled fetch cannot reveal a late result, and exit restores the TTY.
    fake.delay();
    pty.send(b" ");
    let start = Instant::now();
    while fake.signs() < before + 4 {
        pty.read();
        assert!(start.elapsed() < Duration::from_secs(5));
    }
    pty.send(b"\r");
    pty.wait_text("Environment name");
    fake.release();
    std::thread::sleep(Duration::from_millis(100));
    pty.read();
    assert!(
        !pty.parser
            .screen()
            .contents()
            .contains("DELIBERATE_REVEAL_ONLY")
    );
    pty.send(b"\x1b");
    // KEY is also visible in the editor. Wait for cancellation before sending
    // q, or Crossterm can parse the adjacent Escape and q bytes as Alt+q.
    pty.wait_text("Changes discarded.");
    assert!(
        !pty.parser.screen().contents().contains("Environment name"),
        "Editor remained open after cancellation"
    );
    assert!(
        !pty.parser
            .screen()
            .contents()
            .contains("DELIBERATE_REVEAL_ONLY")
    );
    pty.send(b"q");
    pty.finish();
    let file = fs::read(
        dir.path()
            .join(".psst/local-storage")
            .join(fake.key.filename()),
    )
    .unwrap();
    assert_eq!(
        KeyFile::parse(&file, &fake.key, &fake.key.filename())
            .unwrap()
            .records
            .len(),
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn encrypted_create_save_resolve_exact_text_and_nul_rejection() {
    use support::agent::FakeAgent;
    let fake = FakeAgent::new().await;
    let root = std::env::temp_dir().canonicalize().unwrap();
    for value in ["", " \n🔑\tPASTED_SECRET\n "] {
        let dir = tempfile::tempdir_in(&root).unwrap();
        let config = dir.path().join("psst.json");
        let mut pty = Pty::start_with_agent(&config, None, Some(&fake.socket()));
        pty.wait_text("No secrets yet");
        pty.encrypted_draft("Exact ID");
        pty.send(b"\x1b[200~discard\0entire paste\x1b[201~");
        pty.wait_text("NUL rejected");
        pty.send(format!("\x1b[200~{value}\x1b[201~").as_bytes());
        std::thread::sleep(Duration::from_millis(100));
        pty.read();
        assert!(!pty.history.contains("PASTED_SECRET"));
        #[cfg(target_os = "linux")]
        assert!(
            !fs::read_to_string(format!("/proc/{}/cmdline", pty.child.id()))
                .unwrap()
                .contains("PASTED_SECRET")
        );
        pty.send(b"\x13");
        pty.wait_text("Saved encrypted value and configuration");
        let entries = psst::config::Config::load(config.clone()).unwrap().entries;
        assert_eq!(entries["SECRET_1"].source, "local-storage");
        assert_eq!(entries["SECRET_1"].value, "Exact ID");
        assert!(
            !fs::read_to_string(&config)
                .unwrap()
                .contains("PASTED_SECRET")
        );
        let ciphertext = fs::read_to_string(
            dir.path()
                .join(".psst/local-storage")
                .join(fake.key.filename()),
        )
        .unwrap();
        assert!(!ciphertext.contains("PASTED_SECRET"));
        let output = Command::new(assert_cmd::cargo::cargo_bin!("psst"))
            .env_remove("SSH_AUTH_SOCK")
            .env("SSH_AUTH_SOCK", fake.socket())
            .env("PSST_CONFIG", &config)
            .current_dir(&root)
            .args(["sh", "-c", "printf %s \"$SECRET_1\""])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, value.as_bytes());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("PASTED_SECRET"));
        pty.send(b"q");
        pty.finish();
        assert!(!pty.history.contains("PASTED_SECRET"));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn encrypted_create_save_resolve_with_nonexistent_config_parents() {
    use support::agent::FakeAgent;
    let fake = FakeAgent::new().await;
    let root = std::env::temp_dir().canonicalize().unwrap();
    let dir = tempfile::tempdir_in(&root).unwrap();
    let config_dir = dir.path().join("new-directory/nested");
    let config = config_dir.join("psst.json");
    assert!(!config_dir.exists());
    let mut pty = Pty::start_with_agent(&config, None, Some(&fake.socket()));
    pty.wait_text("No secrets yet");
    pty.encrypted_draft("new-parent");
    assert!(
        !config_dir.exists(),
        "Preparing a draft must not create storage"
    );
    pty.send(b"\x1b[200~MISSING_PARENT_SECRET\x1b[201~\x13");
    pty.wait_text("Saved encrypted value and configuration");
    let entries = psst::config::Config::load(config.clone()).unwrap().entries;
    assert_eq!(entries["SECRET_1"].source, "local-storage");
    assert_eq!(entries["SECRET_1"].value, "new-parent");
    let storage = config_dir.join(".psst/local-storage");
    let file = storage.join(fake.key.filename());
    for (path, mode) in [
        (config_dir.join(".psst"), 0o700),
        (storage, 0o700),
        (file.clone(), 0o600),
    ] {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            mode
        );
    }
    for path in [&config, &file] {
        assert!(
            !fs::read_to_string(path)
                .unwrap()
                .contains("MISSING_PARENT_SECRET")
        );
    }
    let output = Command::new(assert_cmd::cargo::cargo_bin!("psst"))
        .env_remove("SSH_AUTH_SOCK")
        .env("SSH_AUTH_SOCK", fake.socket())
        .env("PSST_CONFIG", &config)
        .current_dir(&root)
        .args(["sh", "-c", "printf %s \"$SECRET_1\""])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"MISSING_PARENT_SECRET");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("MISSING_PARENT_SECRET"));
    pty.send(b"q");
    pty.finish();
    assert!(!pty.history.contains("MISSING_PARENT_SECRET"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn encrypted_creation_chooser_selector_replacement_and_conflict() {
    use psst::providers::local_storage::{PublicKey, StoredKey};
    use support::agent::FakeAgent;
    let fake = FakeAgent::with_seeds(&[7, 8]).await;
    let second = PublicKey::from_blob(&fake.state.identities.lock().unwrap()[1]).unwrap();
    let root = std::env::temp_dir().canonicalize().unwrap();
    let dir = tempfile::tempdir_in(&root).unwrap();
    StoredKey::load(dir.path(), second.clone())
        .unwrap()
        .set(&fake.client, "shared", "OTHER_USER")
        .await
        .unwrap();
    StoredKey::load(dir.path(), fake.key.clone())
        .unwrap()
        .set(&fake.client, "shared", "OLD_VALUE")
        .await
        .unwrap();
    let config = dir.path().join("psst.json");
    let mut pty = Pty::start_with_agent(&config, None, Some(&fake.socket()));
    pty.wait_text("No secrets yet");
    pty.encrypted_draft("shared");
    pty.send(b"\x1b[200~NEW_VALUE\x1b[201~\x13");
    pty.wait_text("Choose an SSH fingerprint explicitly");
    pty.send(b"\t\r\x13"); // Choose highlighted first key explicitly.
    pty.wait_text("Confirm replacement");
    assert!(pty.parser.screen().contents().contains("ALL references"));
    let file = dir
        .path()
        .join(".psst/local-storage")
        .join(fake.key.filename());
    let before = fs::read(&file).unwrap();
    pty.send(b"n");
    pty.wait_text("Replacement cancelled");
    assert_eq!(fs::read(&file).unwrap(), before);
    assert!(!config.exists());
    pty.send(b"\x13");
    pty.wait_text("Confirm replacement");
    StoredKey::load(dir.path(), fake.key.clone())
        .unwrap()
        .set(&fake.client, "shared", "CONCURRENT_VALUE")
        .await
        .unwrap();
    let concurrent = fs::read(&file).unwrap();
    pty.send(b"y");
    pty.wait_text("Local-storage changed on disk");
    assert_eq!(fs::read(&file).unwrap(), concurrent);
    assert!(!config.exists());
    pty.send(b"\x13");
    pty.wait_text("Confirm replacement");
    pty.send(b"y");
    pty.wait_text("Saved encrypted value and configuration");
    let entries = psst::config::Config::load(config.clone()).unwrap().entries;
    assert_eq!(
        entries["SECRET_1"].value,
        format!("shared#{}", fake.key.fingerprint())
    );
    let actual =
        psst::providers::local_storage::StoredKey::load(dir.path(), fake.key.clone()).unwrap();
    assert_eq!(
        actual.records()[0]
            .decrypt(&fake.client, &fake.key, "shared")
            .await
            .unwrap()
            .as_str(),
        "NEW_VALUE"
    );
    assert_eq!(
        StoredKey::load(dir.path(), second.clone())
            .unwrap()
            .records()[0]
            .decrypt(&fake.client, &second, "shared")
            .await
            .unwrap()
            .as_str(),
        "OTHER_USER"
    );
    assert!(!pty.history.contains("NEW_VALUE") && !pty.history.contains("OLD_VALUE"));
    // Other IDs must not force a selector, even with several loaded keys.
    pty.encrypted_draft("unique");
    pty.send(b"\t\x1b[B\r\x13");
    pty.wait_text("Saved encrypted value and configuration");
    let entries = psst::config::Config::load(config.clone()).unwrap().entries;
    assert_eq!(entries["SECRET_2"].value, "unique");
    assert!(
        StoredKey::load(dir.path(), second)
            .unwrap()
            .records()
            .iter()
            .any(|r| r.id == "unique")
    );
    pty.send(b"q");
    pty.finish();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn encrypted_creation_agent_errors_cancellation_and_config_failure_leave_usable_tty() {
    use support::agent::{FakeAgent, Fault};
    let fake = FakeAgent::new().await;
    let root = std::env::temp_dir().canonicalize().unwrap();
    let dir = tempfile::tempdir_in(&root).unwrap();
    let config = dir.path().join("psst.json");
    let mut pty = Pty::start_with_agent(&config, None, Some(&fake.socket()));
    pty.wait_text("No secrets yet");
    pty.encrypted_draft("id");
    for (fault, errors) in [
        (Fault::Refuse, &["SSH agent refused"][..]),
        (
            Fault::Disconnect,
            &["early eof", "unexpected end of file"][..],
        ),
    ] {
        fake.fault(fault);
        pty.send(b"\x13");
        pty.wait_any_text(errors);
        assert!(!config.exists());
    }
    fake.fault(Fault::None);
    fake.delay();
    let before = fake.signs();
    pty.send(b"\x13");
    let start = Instant::now();
    while fake.signs() == before {
        pty.read();
        assert!(start.elapsed() < Duration::from_secs(5));
    }
    pty.send(b"\x1b");
    pty.wait_text("No secrets yet");
    fake.release();
    assert!(!config.exists());
    assert!(
        !dir.path()
            .join(".psst/local-storage")
            .join(fake.key.filename())
            .exists()
    );
    pty.encrypted_draft("id");
    // An external config edit happens while human editing is still open.
    fs::write(&config, "{}").unwrap();
    pty.send(b"\x13");
    pty.wait_text("Ciphertext saved; configuration unchanged");
    assert_eq!(fs::read_to_string(&config).unwrap(), "{}");
    assert!(
        dir.path()
            .join(".psst/local-storage")
            .join(fake.key.filename())
            .exists()
    );
    pty.send(b"\x1b");
    pty.wait_text("No secrets yet");
    pty.send(b"q");
    pty.finish();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn encrypted_human_editing_outlives_deadline_save_timeout_is_recoverable() {
    use support::agent::FakeAgent;
    let fake = FakeAgent::new().await;
    let root = std::env::temp_dir().canonicalize().unwrap();
    let dir = tempfile::tempdir_in(&root).unwrap();
    let config = dir.path().join("psst.json");
    let mut pty = Pty::start_with_timeout(&config, None, Some(&fake.socket()), 1);
    pty.wait_text("No secrets yet");
    pty.encrypted_draft("id");
    std::thread::sleep(Duration::from_millis(1200));
    fake.delay();
    pty.send(b"\x13");
    pty.wait_text("SSH agent phase timed out");
    assert!(!config.exists());
    assert!(
        !dir.path()
            .join(".psst/local-storage")
            .join(fake.key.filename())
            .exists()
    );
    assert_eq!(fake.signs(), 1); // No automatic retry after timeout.
    fake.release();
    pty.send(b"\x13");
    pty.wait_text("Saved encrypted value and configuration");
    assert!(config.exists());
    pty.send(b"q");
    pty.finish();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn encrypted_creation_no_eligible_keys_is_a_usable_error_not_arbitrary_selection() {
    use support::agent::FakeAgent;
    let fake = FakeAgent::new().await;
    fake.state.identities.lock().unwrap().clear();
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("psst.json");
    let mut pty = Pty::start_with_agent(&config, None, Some(&fake.socket()));
    pty.wait_text("No secrets yet");
    pty.send(b"a");
    pty.wait_text("Environment name");
    pty.send(b"\t\x1b[C\x1b[C\x0e");
    pty.wait_text("No eligible Ed25519 SSH agent keys");
    pty.send(b"\x13");
    pty.wait_text("Choose an SSH fingerprint explicitly");
    assert!(!config.exists());
    assert_eq!(fake.signs(), 0);
    pty.send(b"\x1b");
    pty.wait_text("No secrets yet");
    pty.send(b"q");
    pty.finish();
}
