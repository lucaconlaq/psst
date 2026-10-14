use anyhow::{Result, bail};
use std::ffi::OsString;
/// Wrapper options and native argv for the command, which ends option parsing.
#[derive(Default)]
pub struct Args {
    /// Copy this config entry, taking priority over write/help/command modes.
    pub copy: Option<OsString>,
    /// Export nonempty resolved values to the caller's .env.
    pub write: bool,
    /// Print help unless copy or write takes priority.
    pub help: bool,
    /// Print version without loading configuration or dispatching other actions.
    pub version: bool,
    /// Wrapped executable followed by its untouched native arguments.
    pub command: Vec<OsString>,
}
impl Args {
    /// Parse arguments excluding `argv[0]`, preserving downstream arguments verbatim.
    ///
    /// Version returns immediately; copy requires a nonempty secret name.
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self> {
        let mut result = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--") => {
                    result.command.extend(args);
                    break;
                }
                Some("--copy") => {
                    result.copy = Some(
                        args.next()
                            .filter(|v| !v.is_empty() && !v.to_string_lossy().starts_with('-'))
                            .ok_or_else(|| anyhow::anyhow!("--copy requires a secret name"))?,
                    );
                }
                Some("--write") => result.write = true,
                Some("--help" | "-h") => result.help = true,
                Some("--version" | "-V") => {
                    result.version = true;
                    return Ok(result);
                }
                Some(s) if s.starts_with("--copy=") => {
                    let name = &s[7..];
                    if name.is_empty() {
                        bail!("--copy requires a secret name");
                    }
                    result.copy = Some(name.into());
                }
                _ => {
                    result.command.push(arg);
                    result.command.extend(args);
                    break;
                }
            }
        }
        Ok(result)
    }
}
/// Noninteractive usage text, printed without loading configuration.
pub const HELP: &str = "psst — secrets, quietly\n\nUsage: psst [--copy NAME | --write] [--] [COMMAND [ARGS...]]\n\n  psst                      Open the secret manager\n  psst command args...      Fetch secrets in parallel and run command\n  psst --copy NAME          Copy one secret (literal bytes, no newline)\n  psst --write              Atomically overwrite .env in current directory\n  psst -h, --help           Show this help\n  psst -V, --version        Show version\n";
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn help_ends_after_options() {
        assert!(HELP.ends_with("  psst -V, --version        Show version\n"));
        for removed in ["Config:", "UI:", "Provider timeout:"] {
            assert!(!HELP.contains(removed));
        }
    }
    #[test]
    fn version_stops_parsing() {
        for flag in ["--version", "-V"] {
            let args = Args::parse([flag, "--copy"].map(Into::into)).unwrap();
            assert!(args.version);
            assert!(args.copy.is_none());
            assert!(args.command.is_empty());
        }
    }
    #[test]
    fn passthrough() {
        let p = Args::parse(["sh", "-c", "a b; $X", "--write", "--help"].map(Into::into)).unwrap();
        assert!(!p.write && !p.help);
        assert_eq!(p.command.len(), 5);
        assert_eq!(
            Args::parse(["--", "--help"].map(Into::into))
                .unwrap()
                .command,
            vec![OsString::from("--help")]
        );
        assert!(Args::parse(["--copy"].map(Into::into)).is_err());
        assert!(
            Args::parse(["--write", "--copy", "X", "-h"].map(Into::into))
                .unwrap()
                .copy
                .is_some()
        );
    }
}
