use anyhow::{Context, Result, bail};
use std::{path::Path, process::Stdio};
use tokio::{io::AsyncWriteExt, process::Command};

/// Copy nonempty literal bytes to the desktop clipboard, without a newline.
/// Timeout/cancellation kills helper descendants; successful owners are retained.
pub async fn copy(value: &str) -> Result<()> {
    if value.is_empty() {
        bail!("Secret is empty");
    }
    let candidates: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("pbcopy", &[])]
    } else if cfg!(target_os = "windows") {
        &[("clip.exe", &[])]
    } else if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        &[
            ("wl-copy", &[]),
            ("xclip", &["-selection", "clipboard"]),
            ("xsel", &["--clipboard", "--input"]),
        ]
    } else {
        &[
            ("xclip", &["-selection", "clipboard"]),
            ("xsel", &["--clipboard", "--input"]),
        ]
    };
    for (program, args) in candidates {
        let mut command = Command::new(program);
        command
            .args(*args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);
        let child = command.spawn();
        let mut child = match child {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            other => other.context("Cannot start clipboard helper")?,
        };
        let mut group = crate::process::Group::new(child.id().unwrap_or(0));
        let work = async {
            let mut stdin = child.stdin.take().unwrap();
            stdin
                .write_all(value.as_bytes())
                .await
                .context("Clipboard helper failed while receiving secret")?;
            drop(stdin);
            if !child.wait().await?.success() {
                bail!("Clipboard helper failed; check your desktop session");
            }
            Ok(())
        };
        tokio::time::timeout(crate::process::timeout(), work)
            .await
            .context("Clipboard helper timed out")??;
        // xclip/wl-copy may fork a clipboard owner that must outlive this call.
        group.release();
        return Ok(());
    }
    bail!("No clipboard helper found (install wl-copy, xclip or xsel)")
}
/// Escape a value for dotenv readers, including literal dollars and newlines.
/// The result is not shell-script syntax and should not be sourced.
pub fn dotenv(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_./:@%+,-".contains(c))
    {
        return value.into();
    }
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
            .replace('\n', "\\n")
    )
}
/// Atomically export successful nonempty values in order, omitting fetch failures.
/// Names must be validated environment identifiers; values remain plaintext.
pub fn write_env(
    path: &Path,
    results: &[(String, Result<zeroize::Zeroizing<String>>)],
) -> Result<()> {
    let mut text = zeroize::Zeroizing::new(String::new());
    for (name, result) in results {
        if let Ok(value) = result
            && !value.is_empty()
        {
            text.push_str(&format!("{name}={}\n", dotenv(value)));
        }
    }
    crate::config::atomic_write(path, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_replacement_keeps_legacy_escaping_order_and_omissions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        std::fs::write(&path, "old content").unwrap();
        write_env(
            &path,
            &[
                ("B".into(), Ok(zeroize::Zeroizing::new("line\n$\"".into()))),
                ("EMPTY".into(), Ok(zeroize::Zeroizing::new(String::new()))),
                ("FAIL".into(), Err(anyhow::anyhow!("failure"))),
                ("A".into(), Ok(zeroize::Zeroizing::new("plain".into()))),
            ],
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "B=\"line\\n\\$\\\"\"\nA=plain\n"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let destination = dir.path().join("directory");
        std::fs::create_dir(&destination).unwrap();
        let error = write_env(&destination, &[]).unwrap_err();
        assert!(
            !error
                .downcast_ref::<crate::config::PersistenceError>()
                .unwrap()
                .changed
        );
        assert!(destination.is_dir());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}
