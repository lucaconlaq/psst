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

struct Pty {
    master: File,
    _slave: File,
    child: Child,
    output: String,
    before: libc::termios,
    parser: vt100::Parser,
}
impl Pty {
    fn start(config: &Path, bin: Option<&Path>) -> Self {
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
            .env("PSST_CONFIG", config)
            .env("TERM", "xterm-256color")
            .stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap()))
            .stderr(Stdio::from(slave.try_clone().unwrap()));
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
            before,
            parser: vt100::Parser::new(30, 120, 0),
        }
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
                self.parser.process(&buf[..n]);
                return n;
            }
        }
        0
    }
    fn wait_text(&mut self, text: &str) {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            self.read();
            if self.parser.screen().contents().contains(text) {
                return;
            }
            if let Some(status) = self.child.try_wait().unwrap() {
                panic!("psst exited {status} before {text:?}: {}", self.output);
            }
        }
        panic!("Missing {text:?} in terminal output: {}", self.output);
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
