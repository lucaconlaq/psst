use anyhow::{Result, bail};
use std::ffi::OsString;
/// Wrapper options and native argv for the command, which ends option parsing.
#[derive(Default)]
pub struct Args {
    /// Copy this config entry, taking priority over export/help/command modes.
    pub copy: Option<OsString>,
    /// Export nonempty resolved values to the caller's .env.
    pub write: bool,
    /// Pull provider contents into managed files, never inferred from wrapped argv.
    pub files_pull: bool,
    /// Push local managed-file contents to their configured providers.
    pub files_push: bool,
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
            if result.files_pull || result.files_push {
                match arg.to_str() {
                    Some("--yes") => result.yes = true,
                    Some("--help" | "-h") => result.help = true,
                    _ => bail!(
                        "{}",
                        if result.files_push {
                            PUSH_USAGE
                        } else {
                            PULL_USAGE
                        }
                    ),
                }
                continue;
            }
            match arg.to_str() {
                Some("--") => {
                    result.command.extend(args);
                    break;
                }
                Some("--files-pull") => {
                    if result.write || result.copy.is_some() {
                        bail!("{PULL_USAGE}");
                    }
                    result.files_pull = true;
                }
                Some("--files-push") => {
                    if result.write || result.copy.is_some() {
                        bail!("{PUSH_USAGE}");
                    }
                    result.files_push = true;
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
const PUSH_USAGE: &str = "Usage: psst --files-push [--yes] (local → provider). Push cannot be combined with pull, copy, environment export, or a wrapped command.";
const PULL_USAGE: &str = "Usage: psst --files-pull [--yes] (provider → local). Pull cannot be combined with other operations or a wrapped command.";

/// Noninteractive usage text, printed without loading configuration.
pub const HELP: &str = "psst — secrets, quietly

Usage: psst [--copy NAME | --write] [--] [COMMAND [ARGS...]]
       psst --files-pull [--yes]
       psst --files-push [--yes]

  psst                      Open the TUI
  psst command args...      Fetch environment entries in parallel and run command
  psst --copy NAME          Copy one secret (literal bytes, no newline)
  psst --write              Atomically overwrite .env in current directory
  psst --files-pull [--yes] Pull all managed files: provider → local, with batch replacement consent
  psst --files-push [--yes] Push all managed files: local → provider, with batch overwrite consent
  psst -h, --help           Show this help
  psst -V, --version        Show version
";
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn help_includes_usage_and_options_only() {
        assert!(HELP.contains("  psst -V, --version        Show version\n"));
        assert!(HELP.contains("psst --files-pull [--yes]"));
        assert!(HELP.contains("provider → local"));
        assert!(!HELP.contains("--write-files"));
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
    fn pull_options() {
        {
            let flag = "--files-pull";
            let args = Args::parse([flag, "--yes"].map(Into::into)).unwrap();
            assert!(args.files_pull && args.yes);
            assert!(args.command.is_empty());
            let args = Args::parse([flag, "--help"].map(Into::into)).unwrap();
            assert!(args.files_pull && args.help);
            for argv in [
                vec![flag, "unexpected"],
                vec![flag, "--", "command"],
                vec![flag, "--copy", "NAME"],
                vec!["--copy", "NAME", flag],
                vec![flag, "--copy=NAME"],
                vec!["--copy=NAME", flag],
                vec![flag, "--write"],
                vec!["--write", flag],
                vec![flag, "--files-pull"],
                vec![flag, "--write-files"],
            ] {
                assert!(
                    Args::parse(argv.iter().map(OsString::from)).is_err(),
                    "{argv:?}"
                );
            }
            for argv in [
                vec!["write-files", "--yes"],
                vec!["--", flag, "--yes"],
                vec!["command", flag, "--yes"],
            ] {
                let args = Args::parse(argv.iter().map(OsString::from)).unwrap();
                assert!(!args.files_pull && !args.yes);
                let expected: Vec<_> = argv
                    .into_iter()
                    .filter(|arg| *arg != "--")
                    .map(OsString::from)
                    .collect();
                assert_eq!(args.command, expected);
            }
        }
    }
    #[test]
    fn removed_alias_is_not_a_pull_operation() {
        let args = Args::parse(["--write-files"].map(Into::into)).unwrap();
        assert!(!args.files_pull);
        assert_eq!(args.command, vec![OsString::from("--write-files")]);
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
