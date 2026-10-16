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
        Self::start_args(config, bin, socket, timeout, &[])
    }
    fn start_args(
        config: &Path,
        bin: Option<&Path>,
        socket: Option<&Path>,
        timeout: u64,
        args: &[&str],
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
            .args(args)
            .env_remove("SSH_AUTH_SOCK")
            .env("PSST_CONFIG", config)
            .env("PSST_TIMEOUT", timeout.to_string())
            .env("TERM", "xterm-256color")
            .stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap()))
            .stderr(Stdio::from(slave.try_clone().unwrap()));
        if !args.is_empty() {
            command.env("TMPDIR", config.parent().unwrap());
        }
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
    fn local_storage_browser(&mut self) {
        self.local_storage_browser_for("Environment name");
    }
    fn local_storage_browser_for(&mut self, label: &str) {
        self.send(b"\x0e");
        self.wait_text(label);
        // The normal draft starts with manual; advance to local-storage.
        self.send(b"\t\x1b[C\x1b[C\x02");
        self.wait_text("Local storage IDs");
        self.wait_without_text(label);
        self.wait_text("Type filters"); // Wait for the browser load to finish.
    }
    fn encrypted_draft(&mut self, id: &str) {
        self.encrypted_draft_for(id, "Environment name");
    }
    fn encrypted_draft_for(&mut self, id: &str, label: &str) {
        self.local_storage_browser_for(label);
        self.send(b"\x0e");
        self.wait_text("Create encrypted value");
        self.wait_text("Masked exact text");
        assert!(!self.parser.screen().contents().contains("Environment name"));
        self.send(format!("\x1b[200~{id}\x1b[201~\t").as_bytes());
        self.wait_text(id);
    }
    fn link_stored_value(&mut self) {
        self.wait_text("Value stored; configuration unchanged");
        self.send(b"\r");
        self.wait_text("Reference selected");
        self.send(b"\x13");
        self.wait_text("Saved securely");
    }
    fn cancel_creator(&mut self) {
        self.send(b"\x1b");
        self.wait_text("Draft cancelled");
        self.cancel_browser();
    }
    fn cancel_browser(&mut self) {
        self.send(b"\x1b");
        self.wait_text("Environment name");
        self.send(b"\x1b");
        self.wait_text("Changes discarded");
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
    fn wait_without_text(&mut self, text: &str) {
        let start = Instant::now();
        while self.parser.screen().contents().contains(text) {
            self.read();
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "psst exited before hiding {text:?}"
            );
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "Still showing {text:?}: {}",
                self.output
            );
        }
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
fn assert_materialization_cleanup(root: &Path) {
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        assert!(!name.starts_with("psst-files-") && !name.starts_with(".psst-write-"));
        if entry.file_type().unwrap().is_dir() {
            assert_materialization_cleanup(&entry.path());
        }
    }
}

fn cancel_materialization(pty: &mut Pty, root: &Path) {
    pty.send(b"\x03");
    let start = Instant::now();
    loop {
        pty.read();
        if let Some(status) = pty.child.try_wait().unwrap() {
            assert_eq!(status.code(), Some(130), "{}", pty.history);
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "Ctrl+C did not exit"
        );
    }
    while pty.read() > 0 {}
    for contents in ["STAGED_PRIVATE", "RESOLVED_PRIVATE", "ORIGINAL_PRIVATE"] {
        assert!(!pty.history.contains(contents));
    }
    assert_materialization_cleanup(root);
}

#[test]
fn managed_file_ctrl_c_during_subprocess_resolution_and_consent() {
    for consent in [false, true] {
        let root = support::tempdir();
        let config = root.path().join("custom.json");
        if consent {
            fs::write(root.path().join("wait"), b"ORIGINAL_PRIVATE").unwrap();
        }
        fs::write(
            &config,
            serde_json::json!({
                "first": {"type":"file", "source":"manual", "value":"STAGED_PRIVATE"},
                "wait": {"type":"file", "source":"shell", "value": if consent {
                    "printf RESOLVED_PRIVATE"
                } else {
                    "printf '%s' $$ > provider-pid; exec sleep 60"
                }}
            })
            .to_string(),
        )
        .unwrap();
        let mut pty = Pty::start_args(&config, None, None, 30, &["--write-files"]);
        if consent {
            pty.wait_text("Replace all? [y/N]");
        } else {
            pty.wait_file(&root.path().join("provider-pid"));
        }
        // Readiness is signalled by the provider/prompt, never a fixed sleep.
        let stages: Vec<_> = fs::read_dir(root.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("psst-files-")
            })
            .collect();
        assert_eq!(stages.len(), 1);
        assert_eq!(
            fs::read_dir(&stages[0]).unwrap().count(),
            if consent { 2 } else { 1 },
            "{}",
            pty.history
        );
        cancel_materialization(&mut pty, root.path());
        assert!(!root.path().join("first").exists());
        if consent {
            assert_eq!(
                fs::read(root.path().join("wait")).unwrap(),
                b"ORIGINAL_PRIVATE"
            );
        } else {
            assert!(!root.path().join("wait").exists());
            let pid: i32 = fs::read_to_string(root.path().join("provider-pid"))
                .unwrap()
                .parse()
                .unwrap();
            let start = Instant::now();
            loop {
                let alive = unsafe { libc::kill(pid, 0) } == 0;
                #[cfg(target_os = "linux")]
                let alive = alive
                    && fs::read_to_string(format!("/proc/{pid}/stat"))
                        .is_ok_and(|stat| !stat.split_once(") ").unwrap().1.starts_with('Z'));
                if !alive {
                    break;
                }
                assert!(
                    start.elapsed() < Duration::from_secs(5),
                    "provider survived cancellation"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn managed_file_ctrl_c_during_agent_signing_closes_socket_and_staging() {
    use std::sync::atomic::Ordering;
    let agent = support::agent::FakeAgent::new().await;
    agent.delay();
    let root = support::tempdir();
    fs::create_dir(root.path().join(".psst")).unwrap();
    fs::set_permissions(root.path().join(".psst"), fs::Permissions::from_mode(0o700)).unwrap();
    let record = root.path().join(".psst").join(agent.key.filename());
    fs::write(&record, include_bytes!("fixtures/local-storage-v1.json")).unwrap();
    fs::set_permissions(&record, fs::Permissions::from_mode(0o600)).unwrap();
    let config = root.path().join("custom.json");
    fs::write(
        &config,
        serde_json::json!({
            "first": {"type":"file", "source":"manual", "value":"STAGED_PRIVATE"},
            "wait": {"type":"file", "source":"local-storage", "value":"fixture"}
        })
        .to_string(),
    )
    .unwrap();
    let mut pty = Pty::start_args(&config, None, Some(&agent.socket()), 30, &["--write-files"]);
    agent.requested().await;
    assert_eq!(agent.signs(), 1);
    cancel_materialization(&mut pty, root.path());
    assert!(!root.path().join("first").exists());
    assert!(!root.path().join("wait").exists());
    tokio::time::timeout(Duration::from_secs(5), async {
        while agent.state.closed.load(Ordering::SeqCst)
            < agent.state.connections.load(Ordering::SeqCst)
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(agent.signs(), 1);
    assert_eq!(
        fs::read(&record).unwrap(),
        include_bytes!("fixtures/local-storage-v1.json")
    );
    assert!(!pty.history.contains("🔑"));
}

#[test]
fn managed_file_batch_consent_approval_refusal_and_stale_targets() {
    for (answer, approve, mutate) in [
        (b"y\n".as_slice(), true, false),
        (b"n\n", false, false),
        (b"\n", false, false),
        (b"yes\n", true, true),
    ] {
        let root = support::tempdir();
        let config = root.path().join("custom.json");
        let first = root.path().join("replace-one");
        let second = root.path().join("replace-two");
        let same = root.path().join("same");
        for path in [&first, &second, &same] {
            fs::write(path, b"OLD_PRIVATE").unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        fs::write(
            &config,
            serde_json::json!({
                "new/deep/one": {"type":"file", "source":"manual", "value":"NEW_PRIVATE"},
                "new/deep/two": {"type":"file", "source":"manual", "value":"NEW_PRIVATE"},
                "replace-one": {"type":"file", "source":"manual", "value":"NEW_PRIVATE"},
                "replace-two": {"type":"file", "source":"manual", "value":"NEW_PRIVATE"},
                "same": {"type":"file", "source":"manual", "value":"OLD_PRIVATE"},
                "bad": {"type":"file", "source":"unknown", "value":"HIDDEN_PRIVATE"}
            })
            .to_string(),
        )
        .unwrap();
        let mut pty = Pty::start_args(&config, None, None, 30, &["--write-files"]);
        pty.wait_text("Replace all? [y/N]");
        assert!(!root.path().join("new").exists());
        assert_eq!(fs::read(&first).unwrap(), b"OLD_PRIVATE");
        assert_eq!(fs::read(&second).unwrap(), b"OLD_PRIVATE");
        let prompt = pty.history.find("Replace all? [y/N]").unwrap();
        assert!(pty.history.find("bad: failed").unwrap() < prompt);
        assert!(pty.history[..prompt].contains(first.to_str().unwrap()));
        assert!(pty.history[..prompt].contains(second.to_str().unwrap()));
        if mutate {
            fs::write(&first, b"EXTERNAL_PRIVATE").unwrap();
        }
        pty.send(answer);
        let start = Instant::now();
        loop {
            pty.read();
            if let Some(status) = pty.child.try_wait().unwrap() {
                assert!(!status.success()); // The provider failure is best effort.
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "consent did not finish"
            );
        }
        while pty.read() > 0 {}
        assert_eq!(pty.history.matches("Replace all? [y/N]").count(), 1);
        for hidden in [
            "OLD_PRIVATE",
            "NEW_PRIVATE",
            "HIDDEN_PRIVATE",
            "EXTERNAL_PRIVATE",
        ] {
            assert!(!pty.history.contains(hidden));
        }
        if approve {
            assert_eq!(
                fs::read(root.path().join("new/deep/one"))
                    .unwrap_or_else(|error| panic!("{error}: {}", pty.history)),
                b"NEW_PRIVATE"
            );
            assert_eq!(
                fs::read(root.path().join("new/deep/two")).unwrap(),
                b"NEW_PRIVATE"
            );
            assert_eq!(fs::read(&second).unwrap(), b"NEW_PRIVATE");
            assert_eq!(
                fs::read(&first).unwrap(),
                if mutate {
                    b"EXTERNAL_PRIVATE".as_slice()
                } else {
                    b"NEW_PRIVATE".as_slice()
                }
            );
            assert!(pty.history.contains("same: unchanged"));
            if mutate {
                assert!(
                    pty.history
                        .contains("replace-one: failed before replacement")
                );
            }
        } else {
            assert!(!root.path().join("new").exists());
            assert_eq!(fs::read(&first).unwrap(), b"OLD_PRIVATE");
            assert_eq!(fs::read(&second).unwrap(), b"OLD_PRIVATE");
        }
        assert!(fs::read_dir(root.path()).unwrap().all(|entry| {
            let name = entry.unwrap().file_name();
            !name.to_string_lossy().starts_with("psst-files-")
                && !name.to_string_lossy().starts_with(".psst-write-")
        }));
        assert_eq!(attrs(&pty.master).c_lflag, pty.before.c_lflag);
    }
}

#[test]
fn files_dashboard_edits_only_mappings_and_restores_terminal() {
    let t = tempfile::tempdir().unwrap();
    let config = t.path().join("psst.json");
    let destination = t.path().join("TOKEN");
    fs::write(&destination, "untouched destination").unwrap();
    fs::write(&config, r#"{"TOKEN":{"source":"manual","value":"environment"},"./TOKEN":{"type":"file","source":"manual","value":"file contents"}}"#).unwrap();
    let mut pty = Pty::start(&config, None);
    pty.wait_text("Entries · 1");
    pty.send(b"\t");
    pty.wait_text("Files · 1");
    pty.wait_text("DESTINATION");
    assert!(!pty.parser.screen().contents().contains("file contents"));
    pty.send(b"\r");
    pty.wait_text("Destination");
    pty.send(b"\t\t\x1b[200~ changed\x1b[201~\x13");
    pty.wait_text("psst --write-files");
    let json: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    assert_eq!(json["./TOKEN"]["value"], "file contents changed");
    assert_eq!(json["./TOKEN"]["type"], "file");
    assert_eq!(json["TOKEN"]["value"], "environment");
    pty.send(b"\x04");
    pty.wait_text("Confirm deletion");
    pty.wait_text("Destination and provider contents kept");
    assert!(!pty.parser.screen().contents().contains("Remove ciphertext"));
    pty.send(b"\t\x1b");
    pty.wait_text("Deletion cancelled");
    pty.send(b"\x04\r");
    pty.wait_text("Files · 0");
    assert_eq!(
        fs::read_to_string(&destination).unwrap(),
        "untouched destination"
    );
    pty.send(b"\x0e");
    pty.wait_text("Destination");
    pty.send(b"\x1b[200~unsaved-file\x1b[201~\x1b");
    pty.wait_text("Changes discarded");
    pty.send(b"\x1bOP");
    pty.wait_text("MANAGE ENTRIES AND FILES");
    pty.send(b"\x1b");
    pty.wait_text("Returned from help");
    pty.send(b"\t");
    pty.wait_text("Entries · 1");
    pty.send(b"\x03");
    pty.finish();
    let json: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    assert_eq!(json.as_object().unwrap().len(), 1);
    assert_eq!(json["TOKEN"]["value"], "environment");
}

#[test]
fn actual_keys_add_save_reveal_hide_delete_and_restore_terminal() {
    let t = tempfile::tempdir().unwrap();
    let config = t.path().join(".psst.json");
    let mut pty = Pty::start(&config, None);
    pty.wait_text("No secrets yet");
    assert!(!config.exists());
    pty.send(b"\x0e");
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
    pty.send(b"\x12");
    pty.wait_text("literal $() ' secret");
    pty.send(b"\x12");
    std::thread::sleep(Duration::from_millis(100));
    pty.read();
    assert!(
        !pty.parser
            .screen()
            .contents()
            .contains("literal $() ' secret")
    );
    pty.send(b"\x04");
    pty.wait_text("Confirm deletion");
    pty.send(b"\r");
    pty.wait_text("No secrets yet");
    assert_eq!(fs::read_to_string(&config).unwrap(), "{}");
    pty.send(b"\x03");
    pty.finish();
}
#[test]
fn dashboard_types_filters_backspace_erases_f1_returns_and_escape_quits() {
    let t = tempfile::tempdir().unwrap();
    let config = t.path().join("psst.json");
    fs::write(
        &config,
        r#"{"AQ":{"source":"manual","value":"hidden"},"BETA":{"source":"manual","value":"other"}}"#,
    )
    .unwrap();
    for filtered in [false, true] {
        let mut pty = Pty::start(&config, None);
        pty.wait_text("BETA");
        // Former action keys are ordinary filter text, including q (not quit).
        pty.send(b"aq");
        pty.wait_text("Entries · 1 · Filter: aq");
        assert!(!pty.parser.screen().contents().contains("Environment name"));
        assert!(!pty.parser.screen().contents().contains("BETA"));
        pty.send(b"\x7f");
        pty.wait_text("Entries · 2 · Filter: a");
        pty.send(b"\x1bOP"); // F1 in an xterm terminal.
        pty.wait_text("Help");
        pty.send(b"\x1b");
        pty.wait_without_text("Help");
        pty.wait_text("Entries · 2 · Filter: a");
        if !filtered {
            pty.send(b"\x7f");
            // The entry count is unchanged, so wait for the filter to disappear.
            pty.wait_without_text("Filter: a");
            pty.wait_text("Entries · 2");
        }
        pty.wait_text("Esc  quit");
        pty.send(b"\x1b");
        pty.finish();
    }
}

#[test]
fn dashboard_escape_exits_immediately_after_an_error_even_when_filtered() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("psst.json");
    let bytes = r#"{"EMPTY":{"source":"manual","value":""}}"#;
    fs::write(&config, bytes).unwrap();
    for filtered in [false, true] {
        let mut pty = Pty::start(&config, None);
        pty.wait_text("EMPTY");
        if filtered {
            pty.send(b"empty");
            pty.wait_text("Filter: empty");
        }
        pty.send(b"\x12");
        pty.wait_text("Manual secret is empty");
        pty.wait_text("Esc  quit");
        pty.send(b"\x1b");
        pty.finish();
        assert_eq!(fs::read_to_string(&config).unwrap(), bytes);
    }
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
    pty.send(b"\x03");
    pty.finish();
}

#[test]
fn local_storage_browser_deletes_unlinked_value_without_agent_and_keeps_mappings() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().canonicalize().unwrap();
    let config = root.join("psst.json");
    let contents = "{\"KEY\":{\"source\":\"local-storage\",\"value\":\"previous\"},\"SHARED\":{\"source\":\"local-storage\",\"value\":\"fixture\"}}";
    fs::write(&config, contents).unwrap();
    let storage = root.join(".psst");
    fs::create_dir(&storage).unwrap();
    fs::set_permissions(&storage, fs::Permissions::from_mode(0o700)).unwrap();
    let file =
        storage.join("cff7d2bf46744594be2dc71b73a668396c2dfdf6e3d5514919204a41ba3889c1.json");
    let mut document: psst::providers::local_storage::KeyFile =
        serde_json::from_slice(include_bytes!("fixtures/local-storage-v1.json")).unwrap();
    let mut unlinked = document.records[0].clone();
    unlinked.id = "unlinked".into();
    document.records.push(unlinked);
    fs::write(&file, serde_json::to_vec(&document).unwrap()).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    let before = fs::read(&file).unwrap();
    let mut pty = Pty::start(&config, None);
    pty.wait_text("KEY");
    pty.send(b"\r");
    pty.wait_text("Environment name");
    pty.send(b"\x02");
    pty.wait_text("Local storage IDs");
    pty.wait_text("unlinked");
    pty.send(b"\x04");
    pty.wait_text("Delete encrypted value?");
    pty.wait_text("Possible references: 1");
    pty.send(b"\x1b");
    pty.wait_text("Deletion cancelled");
    assert_eq!(fs::read(&file).unwrap(), before);
    // Delete the unlinked row first, then the shared record.
    pty.send(b"\x1b[B\x04");
    pty.wait_text("Possible references: 0");
    pty.send(b"\r");
    pty.wait_text("Encrypted value removed; configuration unchanged");
    pty.send(b"\x1b[A\x04");
    pty.wait_text("Possible references: 1");
    pty.send(b"y");
    pty.wait_text("Encrypted value removed; configuration unchanged");
    pty.wait_text("+ New value");
    let document: psst::providers::local_storage::KeyFile =
        serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    assert!(document.records.is_empty());
    assert_eq!(fs::read_to_string(config).unwrap(), contents);
    pty.send(b"\x1b");
    pty.wait_text("Environment name");
    pty.wait_text("previous");
    pty.send(b"\x03");
    pty.finish();
}

#[test]
fn op_browser_and_fetch_failures_cannot_write_to_controlling_terminal() {
    let t = tempfile::tempdir().unwrap();
    let config = t.path().join("psst.json");
    let contents = "{\"KEY\":{\"source\":\"op\",\"value\":\"op://old/item/field\"}}";
    fs::write(&config, contents).unwrap();
    let bin = t.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let op = bin.join("op");
    fs::write(
        &op,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> calls\nif (printf 'OUTSIDE_UI_AUTH_PROMPT' > /dev/tty) 2>/dev/null; then\n sleep 10\nfi\nprintf 'RAW_PROVIDER_DIAGNOSTIC' >&2\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(op, fs::Permissions::from_mode(0o700)).unwrap();
    for action in [b"\x02", b"\x12"] {
        let mut pty = Pty::start(&config, Some(&bin));
        pty.wait_text("KEY");
        pty.send(b"\r");
        pty.wait_text("Environment name");
        pty.send(b"\x10"); // Ctrl+P no longer fetches reference plaintext.
        std::thread::sleep(Duration::from_millis(100));
        pty.read();
        assert!(
            !pty.history
                .contains("op failed; check authentication/reference")
        );
        pty.send(action);
        pty.wait_text("op failed; check authentication/reference");
        pty.send(b"\x03");
        pty.finish();
        assert!(!pty.history.contains("OUTSIDE_UI_AUTH_PROMPT"));
        assert!(!pty.history.contains("RAW_PROVIDER_DIAGNOSTIC"));
    }
    assert_eq!(
        fs::read_to_string(t.path().join("calls")).unwrap(),
        "vault list --format json\nread op://old/item/field\n"
    );
    assert_eq!(fs::read_to_string(config).unwrap(), contents);
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
    pty.send(b"Exampleq");
    pty.wait_text("Type to filter: Exampleq");
    assert!(!pty.parser.screen().contents().contains("› Example Item"));
    pty.send(b"\x7f");
    pty.wait_text("› Example Item");
    pty.send(b"\x1b"); // First clear the filter, without leaving this level.
    pty.wait_without_text("Type to filter: Example");
    assert!(pty.parser.screen().contents().contains("› Example Item"));
    pty.send(b"\x7f"); // Empty-filter Backspace must not go to the parent.
    pty.send(b"\x1bOP");
    pty.wait_text("Help");
    pty.send(b"\x1b");
    pty.wait_text("› Example Item");
    pty.send(b"\x1b"); // Unfiltered Escape goes to the parent level.
    pty.wait_text("Work Vault");
    pty.send(b"\r");
    pty.wait_text("Example Item");
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
    pty.send(b"\x03");
    pty.finish();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ctrl_y_copies_exact_bytes_and_reports_failure_in_all_five_views() {
    use psst::providers::local_storage::StoredKey;
    use std::sync::atomic::Ordering;
    use support::agent::FakeAgent;

    let fake = FakeAgent::new().await;
    let root = std::env::temp_dir().canonicalize().unwrap();
    let value = "  DUMMY_CLIPBOARD_雪\nsecond line\t ";
    for kind in ["env", "file"] {
        let label = if kind == "file" {
            "Destination"
        } else {
            "Environment name"
        };
        for view in ["dashboard", "reference", "browser", "manual", "encrypted"] {
            let dir = tempfile::tempdir_in(&root).unwrap();
            let config = dir.path().join("psst.json");
            let source = if view == "manual" {
                "manual"
            } else {
                "local-storage"
            };
            let reference = if view == "manual" { "" } else { "id" };
            fs::write(
                &config,
                serde_json::json!({"KEY": {"type": kind, "source": source, "value": reference}})
                    .to_string(),
            )
            .unwrap();
            let config_before = fs::read(&config).unwrap();
            let mut stored = StoredKey::load(dir.path(), fake.key.clone()).unwrap();
            stored.set(&fake.client, "id", value).await.unwrap();
            let storage = dir.path().join(".psst").join(fake.key.filename());
            let storage_before = fs::read(&storage).unwrap();
            let bin = dir.path().join("bin");
            fs::create_dir(&bin).unwrap();
            let clipboard = dir.path().join("clipboard");
            let fail = dir.path().join("fail-helper");
            for helper in ["pbcopy", "wl-copy", "xclip", "xsel"] {
                let path = bin.join(helper);
                fs::write(
                    &path,
                    format!(
                        "#!/bin/sh\nif [ -f '{}' ]; then exit 1; fi\ncat > '{}'\n",
                        fail.display(),
                        clipboard.display()
                    ),
                )
                .unwrap();
                fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
            }
            let mut pty = Pty::start_with_agent(&config, Some(&bin), Some(&fake.socket()));
            if kind == "file" {
                pty.wait_text("+ New entry");
                pty.send(b"\t");
            }
            pty.wait_text("KEY");
            match view {
                "reference" => {
                    pty.send(b"\r");
                    pty.wait_text(label);
                }
                "browser" => pty.local_storage_browser_for(label),
                "manual" => {
                    pty.send(b"\r");
                    pty.wait_text(label);
                    pty.send(b"\t\t");
                    pty.send(format!("\x1b[200~{value}\x1b[201~").as_bytes());
                }
                "encrypted" => {
                    pty.encrypted_draft_for("unsaved", label);
                    pty.send(format!("\x1b[200~{value}\x1b[201~").as_bytes());
                }
                _ => {}
            }
            let signs = fake.signs();
            let connections = fake.state.connections.load(Ordering::SeqCst);
            let draft = matches!(view, "manual" | "encrypted");
            pty.send(b"\x19");
            pty.wait_text("Copied.");
            assert_eq!(fs::read(&clipboard).unwrap(), value.as_bytes(), "{view}");
            assert_eq!(fake.signs(), signs + usize::from(!draft), "{view}");
            fs::write(&fail, "fail").unwrap();
            pty.send(b"\x19");
            pty.wait_text("Clipboard helper failed");
            assert_eq!(fs::read(&clipboard).unwrap(), value.as_bytes());
            assert_eq!(fake.signs(), signs + 2 * usize::from(!draft), "{view}");
            if draft {
                assert_eq!(fake.state.connections.load(Ordering::SeqCst), connections);
            }
            assert!(!pty.history.contains("DUMMY_CLIPBOARD"));
            assert_eq!(fs::read(&config).unwrap(), config_before);
            assert_eq!(fs::read(&storage).unwrap(), storage_before);
            assert!(!dir.path().join("KEY").exists());
            pty.send(b"\x03");
            pty.finish();
        }
    }
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
    pty.send(b"\x12");
    pty.wait_text("Invalid SSH agent signature");
    assert_eq!(fake.signs(), before + 1);
    fake.fault(Fault::None);
    for expected in [before + 2, before + 3] {
        pty.send(b"\x12");
        pty.wait_text("DELIBERATE_REVEAL_ONLY");
        assert_eq!(fake.signs(), expected);
        pty.send(b"\x12");
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
    pty.send(b"\x12");
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
    // KEY is also visible in the editor. Wait for cancellation before quitting.
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
    pty.send(b"\x03");
    pty.finish();
    let file = fs::read(dir.path().join(".psst").join(fake.key.filename())).unwrap();
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
        pty.wait_text("Value stored; configuration unchanged");
        assert!(!config.exists());
        pty.link_stored_value();
        let entries = psst::config::Config::load(config.clone()).unwrap().entries;
        assert_eq!(entries["SECRET_1"].source, "local-storage");
        assert_eq!(
            entries["SECRET_1"].value,
            format!("Exact ID#{}", fake.key.fingerprint())
        );
        assert!(
            !fs::read_to_string(&config)
                .unwrap()
                .contains("PASTED_SECRET")
        );
        let ciphertext =
            fs::read_to_string(dir.path().join(".psst").join(fake.key.filename())).unwrap();
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
        pty.send(b"\x03");
        pty.finish();
        assert!(!pty.history.contains("PASTED_SECRET"));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn encrypted_creator_reveal_toggle_expiry_and_hidden_save() {
    use support::agent::FakeAgent;
    let fake = FakeAgent::new().await;
    let root = std::env::temp_dir().canonicalize().unwrap();
    let dir = tempfile::tempdir_in(&root).unwrap();
    let config = dir.path().join("psst.json");
    let value = "DELIBERATELY_REVEALED_DRAFT";
    let mut pty = Pty::start_with_agent(&config, None, Some(&fake.socket()));
    pty.wait_text("No secrets yet");
    pty.encrypted_draft("reveal-id");
    pty.send(format!("\x1b[200~{value}\x1b[201~").as_bytes());
    std::thread::sleep(Duration::from_millis(100));
    pty.read();
    assert!(!pty.history.contains(value));
    let signs = fake.signs();
    pty.send(b"\x12"); // Ctrl+R deliberately reveals the draft, without fetching.
    pty.wait_text(value);
    assert_eq!(fake.signs(), signs);
    pty.send(b"\x12");
    std::thread::sleep(Duration::from_millis(100));
    pty.read();
    assert!(!pty.parser.screen().contents().contains(value));
    pty.send(b"\x12");
    pty.wait_text(value);
    let start = Instant::now();
    while pty.parser.screen().contents().contains(value) {
        pty.read();
        assert!(
            start.elapsed() < Duration::from_secs(35),
            "Draft did not auto-hide"
        );
    }
    assert!(start.elapsed() >= Duration::from_secs(29));
    assert_eq!(fake.signs(), signs);
    // Reveal once more, then explicitly hide before continuing to save.
    pty.send(b"\x12");
    pty.wait_text(value);
    pty.send(b"\x12");
    std::thread::sleep(Duration::from_millis(100));
    pty.read();
    assert!(!pty.parser.screen().contents().contains(value));
    // Deliberate reveals remain in terminal history; only later output is masked.
    pty.history.clear();
    pty.send(b"\x13");
    pty.link_stored_value();
    for path in [
        config.clone(),
        dir.path().join(".psst").join(fake.key.filename()),
    ] {
        assert!(!fs::read_to_string(path).unwrap().contains(value));
    }
    pty.send(b"\x03");
    pty.finish();
    assert!(!pty.history.contains(value));
    let stored =
        psst::providers::local_storage::StoredKey::load(dir.path(), fake.key.clone()).unwrap();
    assert_eq!(
        stored.records()[0]
            .decrypt(&fake.client, &fake.key, "reveal-id")
            .await
            .unwrap()
            .as_str(),
        value
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn encrypted_creator_ctrl_u_is_inert_and_save_lock_error_keeps_ciphertext() {
    use psst::providers::local_storage::StoredKey;
    use support::agent::FakeAgent;
    let fake = FakeAgent::with_seeds(&[7, 8]).await;
    let root = std::env::temp_dir().canonicalize().unwrap();
    let dir = tempfile::tempdir_in(&root).unwrap();
    StoredKey::load(dir.path(), fake.key.clone())
        .unwrap()
        .set(&fake.client, "existing", "NEVER_REUSED_PLAINTEXT")
        .await
        .unwrap();
    let file = dir.path().join(".psst").join(fake.key.filename());
    let before = fs::read(&file).unwrap();
    let config = dir.path().join("psst.json");
    let mut pty = Pty::start_with_agent(&config, None, Some(&fake.socket()));
    pty.wait_text("No secrets yet");
    for id in ["missing", "existing"] {
        pty.encrypted_draft(id);
        let signs = fake.signs();
        for keys in [&b"\x15"[..], &b"\t\r\x15"[..]] {
            pty.send(keys); // Ctrl+U is inert both before and after choosing a key.
            std::thread::sleep(Duration::from_millis(100));
            pty.read();
            assert!(
                pty.parser
                    .screen()
                    .contents()
                    .contains("Create encrypted value")
            );
            assert!(
                !pty.parser
                    .screen()
                    .contents()
                    .contains("Choose an SSH fingerprint explicitly")
            );
            assert!(
                !pty.parser
                    .screen()
                    .contents()
                    .contains("Confirm replacement")
            );
            assert!(!config.exists());
            assert_eq!(fake.signs(), signs);
            assert_eq!(fs::read(&file).unwrap(), before);
        }
        let lock = fs::OpenOptions::new()
            .write(true)
            .open(file.with_extension("lock"))
            .unwrap();
        fs2::FileExt::try_lock_exclusive(&lock).unwrap();
        pty.send(b"\x13");
        if id == "existing" {
            pty.wait_text("Confirm replacement");
            pty.send(b"y");
        }
        pty.wait_text("Local-storage key is locked");
        assert_eq!(fs::read(&file).unwrap(), before);
        assert!(!config.exists());
        drop(lock);
        pty.cancel_creator();
    }
    pty.send(b"\x03");
    pty.finish();
    assert!(!pty.history.contains("NEVER_REUSED_PLAINTEXT"));
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
    pty.wait_text("Value stored; configuration unchanged");
    assert!(!config.exists());
    pty.link_stored_value();
    let entries = psst::config::Config::load(config.clone()).unwrap().entries;
    assert_eq!(entries["SECRET_1"].source, "local-storage");
    assert_eq!(
        entries["SECRET_1"].value,
        format!("new-parent#{}", fake.key.fingerprint())
    );
    let storage = config_dir.join(".psst");
    let file = storage.join(fake.key.filename());
    for (path, mode) in [(storage, 0o700), (file.clone(), 0o600)] {
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
    pty.send(b"\x03");
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
    let file = dir.path().join(".psst").join(fake.key.filename());
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
    pty.wait_text("Value stored; configuration unchanged");
    assert!(!config.exists());
    pty.link_stored_value();
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
    // Browser selection qualifies even unique IDs with the chosen key.
    pty.encrypted_draft("unique");
    pty.send(b"\t\x1b[B\r\x13");
    pty.link_stored_value();
    let entries = psst::config::Config::load(config.clone()).unwrap().entries;
    assert_eq!(
        entries["SECRET_2"].value,
        format!("unique#{}", second.fingerprint())
    );
    assert!(
        StoredKey::load(dir.path(), second)
            .unwrap()
            .records()
            .iter()
            .any(|r| r.id == "unique")
    );
    pty.send(b"\x03");
    pty.finish();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn encrypted_deletion_default_cancel_explicit_and_unresolved_restore_terminal() {
    use psst::config::{Config, Entry};
    use psst::providers::local_storage::{PublicKey, StoredKey};
    let fake = support::agent::FakeAgent::with_seeds(&[7, 8]).await;
    let other = PublicKey::from_blob(&fake.state.identities.lock().unwrap()[1]).unwrap();
    for mode in [
        "keep",
        "cancel",
        "remove",
        "ambiguous",
        "missing",
        "malformed",
        "timeout",
        "busy_cancel",
    ] {
        let root = std::env::temp_dir().canonicalize().unwrap();
        let dir = tempfile::tempdir_in(root).unwrap();
        for (key, id, value) in [
            (&fake.key, "id", "FIRST_PRIVATE"),
            (&fake.key, "unrelated", "KEEP_PRIVATE"),
            (&other, "id", "OTHER_PRIVATE"),
        ] {
            StoredKey::load(dir.path(), key.clone())
                .unwrap()
                .set(&fake.client, id, value)
                .await
                .unwrap();
        }
        let path = dir.path().join("psst.json");
        let value = if mode == "ambiguous" {
            "id".into()
        } else if mode == "missing" {
            format!("missing#{}", fake.key.fingerprint())
        } else {
            format!("id#{}", fake.key.fingerprint())
        };
        let entry = |value| Entry {
            kind: psst::config::MappingKind::Env,
            source: "local-storage".into(),
            value,
        };
        Config::load(path.clone())
            .unwrap()
            .save(
                [
                    ("FIRST".into(), entry(value.clone())),
                    ("SHARED".into(), entry(value)),
                    ("OTHER".into(), entry(format!("id#{}", other.fingerprint()))),
                ]
                .into_iter()
                .collect(),
            )
            .unwrap();
        let file = dir.path().join(".psst").join(fake.key.filename());
        let other_file = file.with_file_name(other.filename());
        if mode == "malformed" {
            fs::write(&file, b"{}").unwrap();
        }
        let before = fs::read(&file).unwrap();
        let other_before = fs::read(&other_file).unwrap();
        let mut pty = Pty::start_with_timeout(&path, None, Some(&fake.socket()), 1);
        pty.wait_text("FIRST");
        pty.send(b"\x04");
        pty.wait_text("[ ] Remove ciphertext");
        pty.wait_text("Known shared references");
        if matches!(mode, "timeout" | "busy_cancel") {
            // A timed-out socket may close before consuming its release permit.
            while let Ok(permit) = fake.state.listing_gate.try_acquire() {
                permit.forget();
            }
            fake.state
                .delay_listing
                .store(1, std::sync::atomic::Ordering::SeqCst);
        }
        if mode == "cancel" {
            pty.send(b"\t\x1b");
            pty.wait_text("Deletion cancelled");
            assert!(
                Config::load(path.clone())
                    .unwrap()
                    .entries
                    .contains_key("FIRST")
            );
        } else {
            if mode != "keep" {
                pty.send(b"\t");
                pty.wait_text("[x] Remove ciphertext");
            }
            pty.send(b"\r");
            if mode == "busy_cancel" {
                pty.wait_text("Selecting identity");
                pty.send(b"\x1b");
                pty.wait_text("Deletion cancelled");
                assert!(
                    Config::load(path.clone())
                        .unwrap()
                        .entries
                        .contains_key("FIRST")
                );
            } else {
                pty.wait_text(if mode == "remove" {
                    "Mapping and selected ciphertext removed"
                } else if mode == "keep" {
                    "ciphertext kept"
                } else {
                    "Mapping removed; ciphertext kept"
                });
                assert!(
                    !Config::load(path.clone())
                        .unwrap()
                        .entries
                        .contains_key("FIRST")
                );
            }
        }
        if matches!(mode, "timeout" | "busy_cancel") {
            fake.state
                .delay_listing
                .store(0, std::sync::atomic::Ordering::SeqCst);
            fake.state.listing_gate.add_permits(1);
        }
        if mode == "remove" {
            let stored = StoredKey::load(dir.path(), fake.key.clone()).unwrap();
            assert_eq!(stored.records().len(), 1);
            assert_eq!(stored.records()[0].id, "unrelated");
            pty.send(b"\x12"); // Remaining shared mapping fails safely.
            pty.wait_text("No loaded eligible key");
        } else {
            assert_eq!(fs::read(&file).unwrap(), before);
        }
        assert_eq!(fs::read(&other_file).unwrap(), other_before);
        let record = StoredKey::load(dir.path(), other.clone())
            .unwrap()
            .records()[0]
            .clone();
        assert_eq!(
            &**record.decrypt(&fake.client, &other, "id").await.unwrap(),
            "OTHER_PRIVATE"
        );
        pty.send(b"\x0e");
        pty.wait_text("Environment name");
        pty.send(b"\x03");
        pty.finish();
        assert!(
            !pty.history.contains("FIRST_PRIVATE")
                && !pty.history.contains("KEEP_PRIVATE")
                && !pty.history.contains("OTHER_PRIVATE")
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn encrypted_browser_link_and_shared_update_preserve_ciphertext_and_other_users() {
    use psst::config::{Config, Entry};
    use psst::providers::local_storage::{PublicKey, StoredKey};
    let fake = support::agent::FakeAgent::with_seeds(&[7, 8]).await;
    let other = PublicKey::from_blob(&fake.state.identities.lock().unwrap()[1]).unwrap();
    let root = std::env::temp_dir().canonicalize().unwrap();
    let dir = tempfile::tempdir_in(root).unwrap();
    for (key, id, value) in [
        (&fake.key, "shared", "OLD_SECRET"),
        (&fake.key, "unrelated", "KEEP"),
        (&other, "shared", "OTHER_USER"),
    ] {
        StoredKey::load(dir.path(), key.clone())
            .unwrap()
            .set(&fake.client, id, value)
            .await
            .unwrap();
    }
    let path = dir.path().join("psst.json");
    let selected = format!("shared#{}", fake.key.fingerprint());
    let entry = |value: String| Entry {
        kind: psst::config::MappingKind::Env,
        source: "local-storage".into(),
        value,
    };
    Config::load(path.clone())
        .unwrap()
        .save(
            [
                ("FIRST".into(), entry(selected.clone())),
                (
                    "./shared-file".into(),
                    Entry {
                        kind: psst::config::MappingKind::File,
                        ..entry(selected.clone())
                    },
                ),
                (
                    "OTHER".into(),
                    entry(format!("shared#{}", other.fingerprint())),
                ),
            ]
            .into_iter()
            .collect(),
        )
        .unwrap();
    let file = dir.path().join(".psst").join(fake.key.filename());
    let other_file = dir.path().join(".psst").join(other.filename());
    let before = fs::read(&file).unwrap();
    let other_before = fs::read(&other_file).unwrap();
    let old = StoredKey::load(dir.path(), fake.key.clone())
        .unwrap()
        .records()
        .iter()
        .find(|r| r.id == "shared")
        .unwrap()
        .clone();
    let mut pty = Pty::start_with_agent(&path, None, Some(&fake.socket()));
    pty.wait_text("FIRST");
    let signs = fake.signs();
    pty.local_storage_browser();
    pty.send(b"shared");
    std::thread::sleep(Duration::from_millis(100));
    if other.fingerprint() < fake.key.fingerprint() {
        pty.send(b"\x1b[B");
    }
    pty.send(b"\r");
    pty.wait_text("Reference selected");
    assert!(
        !Config::load(path.clone())
            .unwrap()
            .entries
            .contains_key("SECRET_1")
    );
    assert_eq!(fake.signs(), signs);
    assert_eq!(fs::read(&file).unwrap(), before);
    pty.send(b"\x13");
    pty.wait_text("Saved securely");
    assert_eq!(fake.signs(), signs);
    assert_eq!(fs::read(&file).unwrap(), before);
    assert_eq!(fs::read(&other_file).unwrap(), other_before);
    assert_eq!(
        Config::load(path.clone()).unwrap().entries["SECRET_1"].value,
        selected
    );
    // Open FIRST and update through its qualified reference.
    pty.send(b"\x1b[H\r");
    pty.wait_text("Environment name");
    pty.send(b"\x02");
    pty.wait_text("Local storage IDs");
    pty.wait_text("Type filters");
    pty.send(b"\x0e");
    pty.wait_text("Create encrypted value");
    pty.wait_text("Masked exact text");
    pty.send(
        b"\x1b[200~shared\x1b[201~\t\x1b[200~ \nUPDATED_SECRET\t\xe2\x98\x83\n\x1b[201~\t\r\x13",
    );
    pty.wait_text("Confirm replacement");
    assert!(
        pty.parser
            .screen()
            .contents()
            .contains("Loaded config only: 3")
    );
    assert!(
        pty.parser
            .screen()
            .contents()
            .contains("possible references.")
    );
    pty.send(b"n");
    pty.wait_text("Replacement cancelled");
    assert_eq!(fs::read(&file).unwrap(), before);
    pty.send(b"\x13y");
    pty.link_stored_value();
    let after = StoredKey::load(dir.path(), fake.key.clone()).unwrap();
    let updated = after.records().iter().find(|r| r.id == "shared").unwrap();
    assert_ne!(updated.salt, old.salt);
    assert_ne!(updated.nonce, old.nonce);
    assert_eq!(after.records().len(), 2);
    assert_eq!(fs::read(&other_file).unwrap(), other_before);
    let result = Command::new(assert_cmd::cargo::cargo_bin!("psst"))
        .env_remove("SSH_AUTH_SOCK")
        .env("SSH_AUTH_SOCK", fake.socket())
        .env("PSST_CONFIG", &path)
        .env("PSST_TIMEOUT", "5")
        .args([
            "sh",
            "-c",
            "printf '%s|%s|%s' \"$FIRST\" \"$SECRET_1\" \"$OTHER\"",
        ])
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(
        String::from_utf8(result.stdout).unwrap(),
        " \nUPDATED_SECRET\t☃\n| \nUPDATED_SECRET\t☃\n|OTHER_USER"
    );
    assert!(!pty.history.contains("UPDATED_SECRET") && !pty.history.contains("OLD_SECRET"));
    pty.send(b"\x03");
    pty.finish();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn file_authoring_linking_and_browser_deletion_keep_destinations_untouched() {
    use psst::config::Config;
    use psst::providers::local_storage::StoredKey;
    let fake = support::agent::FakeAgent::new().await;
    let dir = support::tempdir();
    let config = dir.path().join("psst.json");
    fs::write(
        &config,
        r#"{"ENV":{"source":"local-storage","value":"shared"}}"#,
    )
    .unwrap();
    let config_before = fs::read(&config).unwrap();
    let destination = dir.path().join("config.local");
    fs::write(&destination, "DESTINATION_ONLY").unwrap();
    let mut pty = Pty::start_with_agent(&config, None, Some(&fake.socket()));
    pty.wait_text("ENV");
    pty.send(b"\t");
    pty.wait_text("+ New file");
    pty.encrypted_draft_for("shared", "Destination");
    pty.send(b"\x1b[200~AUTHORED_PRIVATE\x1b[201~\x13");
    pty.wait_text("Value stored; configuration unchanged");
    assert_eq!(fs::read(&config).unwrap(), config_before);
    let storage = dir.path().join(".psst").join(fake.key.filename());
    let stored = fs::read(&storage).unwrap();
    pty.send(b"\r");
    pty.wait_text("Reference selected");
    // A literal destination is not an environment name. Linking is a separate save.
    pty.send(b"\x1b[Z\x1b[Z\x1b[200~./config.local\x1b[201~\x13");
    pty.wait_text("psst --write-files");
    let loaded = Config::load(config.clone()).unwrap();
    assert_eq!(
        loaded.entries["./config.local"].value,
        format!("shared#{}", fake.key.fingerprint())
    );
    assert!(loaded.entries["./config.local"].kind == psst::config::MappingKind::File);
    assert_eq!(fs::read(&storage).unwrap(), stored);
    let linked = fs::read(&config).unwrap();
    let signs = fake.signs();
    pty.send(b"\r\x02");
    pty.wait_text("Local storage IDs");
    pty.wait_text("Type filters");
    pty.send(b"\x04");
    pty.wait_text("Delete encrypted value?");
    pty.wait_text("Possible references: 2");
    pty.wait_text("Other configurations may also use it.");
    pty.send(b"n");
    pty.wait_without_text("Delete encrypted value?");
    assert_eq!(fs::read(&storage).unwrap(), stored);
    pty.send(b"\x04y");
    pty.wait_text("configuration unchanged");
    assert!(
        StoredKey::load(dir.path(), fake.key.clone())
            .unwrap()
            .records()
            .is_empty()
    );
    assert_eq!(fake.signs(), signs);
    assert_eq!(fs::read(&config).unwrap(), linked);
    assert_eq!(
        fs::read_to_string(&destination).unwrap(),
        "DESTINATION_ONLY"
    );
    assert!(!pty.history.contains("AUTHORED_PRIVATE") && !pty.history.contains("DESTINATION_ONLY"));
    pty.send(b"\x03");
    pty.finish();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn files_author_save_materialize_and_environment_compatibility() {
    use psst::config::{Config, MappingKind};
    let fake = support::agent::FakeAgent::new().await;
    let root = std::env::temp_dir().canonicalize().unwrap();
    let dir = tempfile::tempdir_in(root).unwrap();
    let config = dir.path().join("psst.json");
    fs::write(
        &config,
        r#"{"ENV":{"source":"manual","value":"LEGACY_ENV"}}"#,
    )
    .unwrap();
    let before = fs::read(&config).unwrap();
    let destination = dir.path().join("nested/config.local");
    let text = " \nAUTHORED_FIXTURE\t☃\n";
    let mut pty = Pty::start_with_agent(&config, None, Some(&fake.socket()));
    pty.wait_text("ENV");
    pty.send(b"\t");
    pty.wait_text("+ New file");
    pty.encrypted_draft_for("integrated", "Destination");
    pty.send(format!("\x1b[200~{text}\x1b[201~\x13").as_bytes());
    pty.wait_text("Value stored; configuration unchanged");
    assert_eq!(fs::read(&config).unwrap(), before);
    assert!(!destination.exists());
    pty.send(b"\r");
    pty.wait_text("Reference selected");
    pty.send(b"\x1b[Z\x1b[Z\x1b[200~./nested/config.local\x1b[201~\x13");
    pty.wait_text("psst --write-files");
    let loaded = Config::load(config.clone()).unwrap();
    assert!(loaded.entries["ENV"].kind == MappingKind::Env);
    assert!(loaded.entries["./nested/config.local"].kind == MappingKind::File);
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    assert_eq!(saved["ENV"]["type"], "env");
    assert_eq!(saved["./nested/config.local"]["type"], "file");
    assert!(!destination.exists());
    assert!(!pty.history.contains("AUTHORED_FIXTURE"));
    pty.send(b"\x03");
    pty.finish();

    let storage = dir.path().join(".psst").join(fake.key.filename());
    let ciphertext = fs::read(&storage).unwrap();
    let signs = fake.signs();
    let caller = dir.path().join("caller");
    fs::create_dir(&caller).unwrap();
    let run = |args: &[&str]| {
        let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("psst"));
        command
            .current_dir(&caller)
            .env("PSST_CONFIG", &config)
            .env("SSH_AUTH_SOCK", fake.socket())
            .env("PSST_TIMEOUT", "5")
            .env("TMPDIR", dir.path())
            .args(args)
            .timeout(Duration::from_secs(10));
        command.assert().success().get_output().clone()
    };
    let wrapped = run(&["/bin/sh", "-c", "printf '%s' \"$ENV\""]);
    assert_eq!(wrapped.stdout, b"LEGACY_ENV");
    run(&["--write"]);
    assert_eq!(fs::read(caller.join(".env")).unwrap(), b"ENV=LEGACY_ENV\n");
    assert_eq!(fake.signs(), signs);
    assert!(!destination.exists());
    assert!(!caller.join("nested").exists());

    for outcome in ["written", "unchanged"] {
        let output = run(&["--write-files"]);
        let report = String::from_utf8_lossy(&output.stderr);
        assert!(
            report.contains(&format!("./nested/config.local: {outcome}")),
            "{report}"
        );
        assert!(!report.contains("AUTHORED_FIXTURE") && !report.contains("Replace all?"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("AUTHORED_FIXTURE"));
        assert_eq!(fs::read(&destination).unwrap(), text.as_bytes());
        assert_eq!(
            fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(destination.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(fs::read(&storage).unwrap(), ciphertext);
        assert_materialization_cleanup(dir.path());
        assert_materialization_cleanup(destination.parent().unwrap());
    }
    assert_eq!(fake.signs(), signs + 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn encrypted_browser_link_config_failure_keeps_ciphertext_and_usable_terminal() {
    use psst::providers::local_storage::StoredKey;
    use support::agent::{FakeAgent, Fault};
    let fake = FakeAgent::new().await;
    let root = std::env::temp_dir().canonicalize().unwrap();
    let dir = tempfile::tempdir_in(root).unwrap();
    StoredKey::load(dir.path(), fake.key.clone())
        .unwrap()
        .set(&fake.client, "id", "NEVER_RENDER")
        .await
        .unwrap();
    let file = dir.path().join(".psst").join(fake.key.filename());
    let before = fs::read(&file).unwrap();
    let config = dir.path().join("psst.json");
    let mut pty = Pty::start_with_agent(&config, None, Some(&fake.socket()));
    pty.wait_text("No secrets yet");
    let signs = fake.signs();
    pty.local_storage_browser();
    fake.fault(Fault::Refuse); // Linking must not ask the agent to sign or decrypt.
    let lock = fs::OpenOptions::new()
        .write(true)
        .open(file.with_extension("lock"))
        .unwrap();
    fs2::FileExt::try_lock_exclusive(&lock).unwrap();
    // Browsing and choosing a reference do not need the storage write lock.
    pty.send(b"id\r");
    pty.wait_text("Reference selected");
    assert!(!config.exists());
    assert_eq!(fake.signs(), signs);
    fs::write(&config, b"{}").unwrap(); // External edit conflicts with loaded missing config.
    pty.send(b"\x13");
    pty.wait_text("changed on disk");
    assert_eq!(fs::read(&config).unwrap(), b"{}");
    assert_eq!(fs::read(&file).unwrap(), before);
    assert_eq!(fake.signs(), signs);
    drop(lock);
    fake.fault(Fault::None);
    assert!(!pty.history.contains("NEVER_RENDER"));
    pty.send(b"\x1b");
    pty.wait_text("Changes discarded");
    pty.send(b"\x0e");
    pty.wait_text("Environment name");
    pty.send(b"\x1b");
    pty.wait_text("Changes discarded");
    pty.send(b"\x03");
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
    pty.cancel_creator();
    fake.release();
    assert!(!config.exists());
    assert!(!dir.path().join(".psst").join(fake.key.filename()).exists());
    pty.encrypted_draft("id");
    // An external config edit happens while human editing is still open.
    fs::write(&config, "{}").unwrap();
    pty.send(b"\x13");
    pty.wait_text("Value stored; configuration unchanged");
    assert_eq!(fs::read_to_string(&config).unwrap(), "{}");
    pty.send(b"\r");
    pty.wait_text("Reference selected");
    pty.send(b"\x13");
    pty.wait_text("changed on disk");
    assert_eq!(fs::read_to_string(&config).unwrap(), "{}");
    assert!(dir.path().join(".psst").join(fake.key.filename()).exists());
    pty.send(b"\x1b");
    pty.wait_text("No secrets yet");
    pty.send(b"\x03");
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
    assert!(!dir.path().join(".psst").join(fake.key.filename()).exists());
    assert_eq!(fake.signs(), 1); // No automatic retry after timeout.
    fake.release();
    pty.send(b"\x13");
    pty.wait_text("Value stored; configuration unchanged");
    assert!(!config.exists());
    pty.link_stored_value();
    assert!(config.exists());
    pty.send(b"\x03");
    pty.finish();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn encrypted_creation_no_eligible_keys_is_a_usable_error_not_arbitrary_selection() {
    use support::agent::FakeAgent;
    let fake = FakeAgent::new().await;
    fake.state.identities.lock().unwrap().clear();
    let root = std::env::temp_dir().canonicalize().unwrap();
    let dir = tempfile::tempdir_in(&root).unwrap();
    let config = dir.path().join("psst.json");
    let mut pty = Pty::start_with_agent(&config, None, Some(&fake.socket()));
    pty.wait_text("No secrets yet");
    pty.send(b"\x0e");
    pty.wait_text("Environment name");
    pty.send(b"\t\x1b[C\x1b[C\x02");
    pty.wait_text("Local storage IDs");
    pty.wait_text("Type filters");
    pty.send(b"\x0e");
    pty.wait_text("No eligible Ed25519 SSH agent keys");
    pty.send(b"\x13");
    pty.wait_text("Choose an SSH fingerprint explicitly");
    assert!(!config.exists());
    assert_eq!(fake.signs(), 0);
    pty.cancel_creator();
    pty.send(b"\x03");
    pty.finish();
}
