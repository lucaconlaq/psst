use anyhow::{Context, Result, bail};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command};
use zeroize::Zeroizing;

/// Maximum captured provider output in bytes, before UTF-8 decoding.
pub const OUTPUT_LIMIT: u64 = 4 * 1024 * 1024;
// Each operation owns a process group until cleanup or explicit release.
pub(crate) struct Group(u32);
impl Group {
    pub(crate) fn new(pid: u32) -> Self {
        Self(pid)
    }
    pub(crate) fn release(&mut self) {
        self.0 = 0;
    }
}
impl Drop for Group {
    fn drop(&mut self) {
        #[cfg(unix)]
        if self.0 > 0 {
            // SAFETY: A negative PID targets the subprocess group we created.
            // No pointers are passed; failure (e.g. an exited group) is harmless.
            unsafe {
                libc::kill(-(self.0 as i32), libc::SIGKILL);
            }
        }
    }
}
/// Positive PSST_TIMEOUT seconds, falling back to 30 for missing/invalid values.
pub fn timeout() -> Duration {
    Duration::from_secs(
        std::env::var("PSST_TIMEOUT")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(30),
    )
}
/// Run literal argv without a controlling terminal, with null stdin/stderr,
/// and capture trimmed UTF-8 stdout.
/// Enforce size/NUL/exit checks; cancellation and timeout kill the process group.
pub async fn output(program: &str, args: &[&str], cwd: &Path) -> Result<Zeroizing<String>> {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            // A fresh session prevents CLI prompts from bypassing redirected
            // streams via /dev/tty. Its process group still uses the child PID.
            // SAFETY: setsid takes no pointers and is async-signal-safe.
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn().with_context(|| {
        format!("Cannot start {program}; check installation and authentication")
    })?;
    let _group = Group::new(child.id().unwrap_or(0));
    let stdout = child.stdout.take().unwrap();
    let run = async {
        let mut bytes = Zeroizing::new(Vec::new());
        stdout
            .take(OUTPUT_LIMIT + 1)
            .read_to_end(&mut bytes)
            .await?;
        if bytes.len() as u64 > OUTPUT_LIMIT {
            bail!("Provider output exceeded 4 MiB");
        }
        if !child.wait().await?.success() {
            bail!(
                "{program} failed; check authentication/reference (stderr suppressed to protect secrets)"
            );
        }
        let text = std::str::from_utf8(&bytes).context("Provider returned non-UTF8 output")?;
        if text.contains('\0') {
            bail!("Provider output contains a NUL byte");
        }
        Ok(Zeroizing::new(text.trim().to_owned()))
    };
    tokio::time::timeout(timeout(), run)
        .await
        .context("Provider timed out (PSST_TIMEOUT controls seconds)")?
}
