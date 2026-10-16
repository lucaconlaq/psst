use anyhow::{Result, bail};
use std::ffi::OsString;
/// Wrapper options and native argv for the command, which ends option parsing.
#[derive(Default)]
pub struct Args {
    /// Copy this config entry, taking priority over write/help/command modes.
    pub copy: Option<OsString>,
    /// Export nonempty resolved values to the caller's .env.
    pub write: bool,
    /// Explicit managed-file materialization, never inferred from wrapped argv.
    pub write_files: bool,
    /// Approve replacements, without bypassing destination safety.
    pub yes: bool,
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
                Some("--write-files")
                    if result.command.is_empty() && !result.write && result.copy.is_none() =>
                {
                    result.write_files = true;
                    for option in args {
                        match option.to_str() {
                            Some("--yes") => result.yes = true,
                            Some("--help" | "-h") => result.help = true,
                            _ => bail!("Usage: psst --write-files [--yes]"),
                        }
                    }
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
pub const HELP: &str = "psst — secrets, quietly

Usage: psst [--copy NAME | --write] [--] [COMMAND [ARGS...]]
       psst --write-files [--yes]

  psst                      Open the TUI
  psst command args...      Fetch environment entries in parallel and run command
  psst --copy NAME          Copy one secret (literal bytes, no newline)
  psst --write              Atomically overwrite .env in current directory
  psst --write-files [--yes] Materialize all managed files with batch replacement consent
  psst -h, --help           Show this help
  psst -V, --version        Show version
";
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn help_includes_usage_and_options_only() {
        assert!(HELP.contains("  psst -V, --version        Show version\n"));
        assert!(HELP.contains("psst --write-files [--yes]"));
        assert!(HELP.contains("Open the TUI"));
        for control in ["Dashboard", "Ctrl+N", "Ctrl+S", "F1", "Legacy untagged"] {
            assert!(!HELP.contains(control));
        }
        assert!(HELP.ends_with("Show version\n"));
        for removed in ["Config:", "UI:", "Provider timeout:"] {
            assert!(!HELP.contains(removed));
        }
    }
    #[test]
    fn write_files_option() {
        let args = Args::parse(["--write-files", "--yes"].map(Into::into)).unwrap();
        assert!(args.write_files && args.yes);
        assert!(args.command.is_empty());
        let args = Args::parse(["--write-files", "--help"].map(Into::into)).unwrap();
        assert!(args.write_files && args.help);
        assert!(Args::parse(["--write-files", "unexpected"].map(Into::into)).is_err());
        for argv in [
            vec!["write-files", "--yes"],
            vec!["--", "--write-files", "--yes"],
            vec!["command", "--write-files", "--yes"],
        ] {
            let args = Args::parse(argv.iter().map(OsString::from)).unwrap();
            assert!(!args.write_files && !args.yes);
            let expected: Vec<_> = argv
                .into_iter()
                .filter(|arg| *arg != "--")
                .map(OsString::from)
                .collect();
            assert_eq!(args.command, expected);
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
