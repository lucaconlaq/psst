use anyhow::{Context, Result, bail};
use fs2::FileExt;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

/// A provider identifier and stored reference, or plaintext manual literal.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    /// Case-sensitive provider identifier; unknown providers remain editable.
    pub source: String,
    /// Provider reference, trusted shell command, or plaintext manual literal.
    pub value: String,
}
/// Environment names mapped to entries in JSON insertion order.
pub type Entries = IndexMap<String, Entry>;

struct StrictEntries(Entries);
impl<'de> Deserialize<'de> for StrictEntries {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = StrictEntries;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an object of unique environment names")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> std::result::Result<Self::Value, M::Error> {
                let mut entries = Entries::new();
                while let Some((name, entry)) = map.next_entry::<String, Entry>()? {
                    if entries.insert(name, entry).is_some() {
                        return Err(serde::de::Error::custom("duplicate environment name"));
                    }
                }
                Ok(StrictEntries(entries))
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

/// Whether a name is an ASCII environment identifier: `[A-Za-z_][A-Za-z0-9_]*`.
pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}
/// Find the nearest config, checking visible before hidden names at each ancestor.
/// A nonempty override is relative to cwd; traversal stops at home or root.
/// With no match, return cwd/psst.json without creating it.
pub fn discover(
    cwd: &Path,
    home: Option<&Path>,
    override_path: Option<&std::ffi::OsStr>,
) -> PathBuf {
    if let Some(p) = override_path.filter(|p| !p.is_empty()) {
        return cwd.join(p);
    }
    for dir in cwd.ancestors() {
        for name in ["psst.json", ".psst.json"] {
            let p = dir.join(name);
            if p.is_file() {
                return p;
            }
        }
        if Some(dir) == home {
            break;
        }
    }
    cwd.join("psst.json")
}

/// Ordered configuration with the original bytes retained for conflict detection.
pub struct Config {
    /// Chosen config path, also determining provider cwd.
    pub path: PathBuf,
    /// Current entries; use save to validate and persist a replacement.
    pub entries: Entries,
    baseline: Option<Vec<u8>>,
}
impl Config {
    /// Load a config up to 4 MiB, rejecting invalid schema, names, duplicates and NUL.
    /// Missing and zero-byte files are treated as empty without creating a file.
    pub fn load(path: PathBuf) -> Result<Self> {
        let baseline = read_optional(&path)?;
        let entries: Entries = match baseline.as_deref() {
            None | Some([]) => Entries::new(),
            Some(bytes) => serde_json::from_slice::<StrictEntries>(bytes).map_err(|e| {
                // Serde type errors can include literal values. Never echo them.
                anyhow::anyhow!("Invalid config {} at line {}, column {} (expected unique environment names with string source/value entries)", path.display(), e.line(), e.column())
            })?.0,
        };
        for (name, entry) in &entries {
            if !valid_name(name) {
                bail!("Invalid environment name in config: {name:?}");
            }
            if entry.source.contains('\0') || entry.value.contains('\0') {
                bail!("Config entry {name:?} contains a NUL byte");
            }
        }
        Ok(Self {
            path,
            entries,
            baseline,
        })
    }
    /// Directory used as cwd when resolving references.
    pub fn dir(&self) -> &Path {
        self.path.parent().unwrap_or(Path::new("."))
    }
    /// Validate and atomically save entries under a cooperative lock.
    /// Reject changes since loading; uncooperative writers can still race the write.
    pub fn save(&mut self, entries: Entries) -> Result<()> {
        self.save_checked(entries, |_| Ok(()))
    }
    pub(crate) fn save_checked(
        &mut self,
        entries: Entries,
        boundary: impl FnMut(WriteBoundary) -> Result<()>,
    ) -> Result<()> {
        for (name, entry) in &entries {
            if !valid_name(name) || entry.source.contains('\0') || entry.value.contains('\0') {
                bail!("Invalid name or NUL in entry");
            }
        }
        let dir = self.dir();
        fs::create_dir_all(dir)?;
        let lock_path = self.path.with_extension("psst-lock");
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)?;
        lock.try_lock_exclusive()
            .context("Config is being saved by another psst process; try again")?;
        if read_optional(&self.path)? != self.baseline {
            bail!("Config changed on disk. Reopen psst before saving.");
        }
        let bytes = serde_json::to_vec_pretty(&entries)?;
        if bytes.len() > 4 * 1024 * 1024 {
            bail!("Config exceeds 4 MiB");
        }
        let result = atomic_write_checked(&self.path, &bytes, boundary);
        if result.as_ref().err().is_none_or(|error| error.changed) {
            self.baseline = Some(bytes);
            self.entries = entries;
        }
        result.map_err(anyhow::Error::new)
    }
}
fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::File::open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(4 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
            if bytes.len() > 4 * 1024 * 1024 {
                bail!("Config exceeds 4 MiB");
            }
            Ok(Some(bytes))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("Cannot read {}", path.display())),
    }
}
/// Replace a file using a synced temporary file in its directory (0600 on Unix).
/// The parent must exist. A directory-sync error can occur after replacement.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    atomic_write_observed(path, bytes).map_err(anyhow::Error::new)
}

/// Failed persistence, including whether replacement already changed the file.
/// A changed file is not necessarily durable when directory syncing fails.
#[derive(Debug)]
pub struct PersistenceError {
    /// Replacement completed; do not claim rollback, even if syncing failed.
    pub changed: bool,
    /// Failure at the recorded persistence boundary.
    pub source: anyhow::Error,
}

impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (content changed: {})", self.source, self.changed)
    }
}

impl std::error::Error for PersistenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum WriteBoundary {
    Write,
    FileSync,
    Replace,
    DirectorySync,
}

/// Atomic replacement with explicit pre/post-replacement failure evidence.
pub fn atomic_write_observed(
    path: &Path,
    bytes: &[u8],
) -> std::result::Result<(), PersistenceError> {
    atomic_write_checked(path, bytes, |_| Ok(()))
}

pub(crate) fn atomic_write_checked(
    path: &Path,
    bytes: &[u8],
    mut boundary: impl FnMut(WriteBoundary) -> Result<()>,
) -> std::result::Result<(), PersistenceError> {
    let mut changed = false;
    let result = (|| -> Result<()> {
        let dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut temp = tempfile::NamedTempFile::new_in(dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            temp.as_file()
                .set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        boundary(WriteBoundary::Write)?;
        temp.write_all(bytes)?;
        boundary(WriteBoundary::FileSync)?;
        temp.as_file().sync_all()?;
        boundary(WriteBoundary::Replace)?;
        temp.persist(path).map_err(|e| e.error)?;
        changed = true;
        boundary(WriteBoundary::DirectorySync)?;
        #[cfg(unix)]
        fs::File::open(dir)?.sync_all()?;
        Ok(())
    })();
    result.map_err(|source| PersistenceError { changed, source })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atomic_failures_preserve_old_or_complete_new_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("document");
        for failure in [
            WriteBoundary::Write,
            WriteBoundary::FileSync,
            WriteBoundary::Replace,
            WriteBoundary::DirectorySync,
        ] {
            fs::write(&path, b"old").unwrap();
            let error = atomic_write_checked(&path, b"complete new", |stage| {
                anyhow::ensure!(stage != failure, "Injected failure");
                Ok(())
            })
            .unwrap_err();
            assert_eq!(error.changed, failure == WriteBoundary::DirectorySync);
            assert_eq!(
                fs::read(&path).unwrap(),
                if error.changed {
                    b"complete new".as_slice()
                } else {
                    b"old".as_slice()
                }
            );
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        }
        atomic_write(&path, b"legacy wrapper").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"legacy wrapper");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn config_failed_replacement_keeps_loaded_entries_and_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("psst.json");
        let mut config = Config::load(path.clone()).unwrap();
        // A directory substituted for a missing file cannot be overwritten.
        fs::create_dir(&path).unwrap();
        assert!(config.save(Entries::new()).is_err());
        assert!(config.entries.is_empty());
        assert!(config.baseline.is_none());
        fs::remove_dir(&path).unwrap();
        config.save(Entries::new()).unwrap();
        assert_eq!(fs::read(path).unwrap(), b"{}");
    }

    #[test]
    fn source_nul_is_rejected_on_load_and_save() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("psst.json");
        fs::write(&path, r#"{"KEY":{"source":"bad\u0000source","value":"x"}}"#).unwrap();
        assert!(Config::load(path.clone()).is_err());
        fs::write(&path, "{}").unwrap();
        let mut config = Config::load(path.clone()).unwrap();
        let entries = Entries::from([(
            "KEY".into(),
            Entry {
                source: "bad\0source".into(),
                value: "x".into(),
            },
        )]);
        assert!(config.save(entries).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "{}");
        assert!(config.entries.is_empty());
    }
    #[test]
    fn discovery_and_root_termination() {
        let t = tempfile::tempdir().unwrap();
        let child = t.path().join("a/b");
        fs::create_dir_all(&child).unwrap();
        fs::write(t.path().join("psst.json"), "{}").unwrap();
        fs::write(t.path().join(".psst.json"), "{}").unwrap();
        assert_eq!(discover(&child, None, None), t.path().join("psst.json"));
        assert_eq!(
            discover(&child, None, Some("missing".as_ref())),
            child.join("missing")
        );
        assert_eq!(
            discover(Path::new("/"), None, None),
            Path::new("/psst.json")
        );
    }
    #[test]
    fn default_config_is_created_only_when_saved() {
        let t = tempfile::tempdir().unwrap();
        let path = discover(t.path(), Some(t.path()), None);
        assert_eq!(path, t.path().join("psst.json"));
        let mut config = Config::load(path.clone()).unwrap();
        assert!(!path.exists());
        config
            .save(Entries::from([(
                "KEY".into(),
                Entry {
                    source: "manual".into(),
                    value: "test-secret".into(),
                },
            )]))
            .unwrap();
        assert_eq!(
            Config::load(path).unwrap().entries["KEY"].value,
            "test-secret"
        );
        assert!(!t.path().join(".psst.json").exists());
    }
    #[test]
    fn persistence_conflict_and_validation() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("nested/.psst.json");
        let mut c = Config::load(p.clone()).unwrap();
        assert!(!p.exists());
        let entries = IndexMap::from([(
            "ABC".into(),
            Entry {
                source: "manual".into(),
                value: "literal".into(),
            },
        )]);
        c.save(entries.clone()).unwrap();
        assert_eq!(
            Config::load(p.clone())
                .unwrap()
                .entries
                .keys()
                .collect::<Vec<_>>(),
            vec!["ABC"]
        );
        fs::write(&p, "{}").unwrap();
        assert!(c.save(entries).is_err());
        fs::write(&p, "null").unwrap();
        assert!(Config::load(p).is_err());
        assert!(!valid_name("1BAD"));
        assert!(valid_name("_GOOD_2"));
    }
}
