use anyhow::{Context, Result, bail};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
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
    let mut state = TransferState::NotStarted;
    capture(program, args, cwd, None, true, &mut state).await
}

/// Evidence retained even when the transfer future is dropped. Once started,
/// a writing command may have changed remote state, including on any error.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TransferState {
    NotStarted,
    Started,
    Succeeded,
}

/// Exact UTF-8 output and optional secret stdin, each bounded to 4 MiB.
/// The caller owns payload lifetime; no payload is copied into argv or errors.
/// Cancellation drops all owned buffers and kills the owned process group.
/// `state` distinguishes failure before spawn from potentially uncertain writes;
/// even Succeeded is command success, not a remote durability guarantee.
pub(crate) async fn exact(
    program: &str,
    args: &[&str],
    cwd: &Path,
    payload: Option<Zeroizing<Vec<u8>>>,
    state: &mut TransferState,
) -> Result<Zeroizing<String>> {
    capture(program, args, cwd, payload, false, state).await
}

async fn capture(
    program: &str,
    args: &[&str],
    cwd: &Path,
    payload: Option<Zeroizing<Vec<u8>>>,
    trim: bool,
    state: &mut TransferState,
) -> Result<Zeroizing<String>> {
    *state = TransferState::NotStarted;
    if payload
        .as_ref()
        .is_some_and(|p| p.len() as u64 > OUTPUT_LIMIT)
    {
        bail!("Provider input exceeded 4 MiB");
    }
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(if payload.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
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
    let mut child = command.spawn().map_err(|_| {
        if trim {
            anyhow::anyhow!("Cannot start {program}; check installation and authentication")
        } else {
            anyhow::anyhow!("Cannot start provider; check installation and authentication")
        }
    })?;
    *state = TransferState::Started;
    let _group = Group::new(child.id().unwrap_or(0));
    let mut stdout = child.stdout.take().context("Provider stdout unavailable")?;
    let stdin = child.stdin.take();
    let run = async {
        let read = async {
            // Allocate once before reading: growing a secret-bearing Vec can
            // free an unwiped previous allocation. Include one overflow byte.
            let mut bytes = Zeroizing::new(vec![0; OUTPUT_LIMIT as usize + 1]);
            let mut used = 0;
            loop {
                let count = stdout
                    .read(&mut bytes[used..])
                    .await
                    .map_err(|_| anyhow::anyhow!("Cannot read provider output"))?;
                if count == 0 {
                    break;
                }
                used += count;
                if used as u64 > OUTPUT_LIMIT {
                    bail!("Provider output exceeded 4 MiB");
                }
            }
            Ok::<_, anyhow::Error>((bytes, used))
        };
        let write = async move {
            if let Some(payload) = payload {
                let mut stdin = stdin.context("Provider stdin unavailable")?;
                stdin
                    .write_all(&payload)
                    .await
                    .map_err(|_| anyhow::anyhow!("Cannot write provider stdin"))?;
                stdin
                    .shutdown()
                    .await
                    .map_err(|_| anyhow::anyhow!("Cannot close provider stdin"))?;
                // Drop the pipe here so the child sees EOF before we wait.
            }
            Ok::<_, anyhow::Error>(())
        };
        // Neither branch is detached. Cancellation/error drops both and their
        // zeroizing buffers; stdout is drained while stdin may be blocked.
        let ((bytes, used), ()) = tokio::try_join!(read, write)?;
        if !child
            .wait()
            .await
            .map_err(|_| anyhow::anyhow!("Cannot wait for provider"))?
            .success()
        {
            if trim {
                bail!(
                    "{program} failed; check authentication/reference (stderr suppressed to protect secrets)"
                );
            }
            bail!("Provider failed (stderr suppressed to protect secrets)");
        }
        let text =
            std::str::from_utf8(&bytes[..used]).context("Provider returned non-UTF8 output")?;
        if text.contains('\0') {
            bail!("Provider output contains a NUL byte");
        }
        let text = if trim { text.trim() } else { text };
        let mut result = Zeroizing::new(String::with_capacity(text.len()));
        result.push_str(text);
        Ok(result)
    };
    let result = tokio::time::timeout(timeout(), run)
        .await
        .context("Provider timed out (PSST_TIMEOUT controls seconds)")?;
    if result.is_ok() {
        *state = TransferState::Succeeded;
    }
    result
}

#[cfg(test)]
mod tests;
