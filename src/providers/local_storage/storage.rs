//! Config-relative, cooperative per-key persistence. No cross-file transaction.

use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use fs2::FileExt;

use super::{Agent, Envelope, FILE_LIMIT, KeyFile, PublicKey, validate_id, validate_value};
use crate::config::{PersistenceError, WriteBoundary, atomic_write_checked};

/// Loaded bytes are retained to reject edits made since loading.
/// Callers supply the selected configuration directory, never the caller cwd.
pub struct StoredKey {
    path: PathBuf,
    key: PublicKey,
    file: KeyFile,
    baseline: Option<Vec<u8>>,
}

impl StoredKey {
    /// Load and strictly validate the complete file without contacting an agent.
    /// Missing files represent an empty document and are not created by reading.
    pub fn load(config_dir: &Path, key: PublicKey) -> Result<Self> {
        let path = config_dir.join(".psst/local-storage").join(key.filename());
        check_path(&path.with_extension("lock"))?;
        let baseline = read(&path)?;
        let file = match &baseline {
            Some(bytes) => KeyFile::parse(bytes, &key, &key.filename())?,
            None => KeyFile::new(&key),
        };
        Ok(Self {
            path,
            key,
            file,
            baseline,
        })
    }

    /// Validated records, including unrelated IDs preserved by mutations.
    pub fn records(&self) -> &[Envelope] {
        &self.file.records
    }

    /// Create or replace only this ID with fresh salt/nonce.
    /// The lock covers signing and replacement. Dropping the future before signing
    /// completes leaves ciphertext unchanged; replacement itself has no await.
    /// Errors can be downcast to PersistenceError to inspect completed changes.
    pub async fn set(&mut self, agent: &Agent, id: &str, value: &str) -> Result<()> {
        self.set_validated(agent, id, value, async { Ok(()) }).await
    }

    // Await authoring validation after signing but before replacement, so listing
    // failures or cancellation cannot hide a completed write.
    pub(super) async fn set_validated<T>(
        &mut self,
        agent: &Agent,
        id: &str,
        value: &str,
        validation: impl std::future::Future<Output = Result<T>>,
    ) -> Result<T> {
        validate_id(id)?;
        validate_value(value)?;
        self.file.validate(&self.key)?;
        let _lock = self.lock()?;
        let record = Envelope::encrypt(agent, &self.key, id, value).await?;
        let validated = validation.await?;
        let mut next = self.file.clone();
        next.records.retain(|record| record.id != id);
        next.records.push(record);
        self.commit(next, |_| Ok(()))?;
        Ok(validated)
    }

    // Keep the snapshot locked through verification; plaintext is discarded here.
    pub(super) async fn reuse_validated<T>(
        &self,
        agent: &Agent,
        id: &str,
        validation: impl std::future::Future<Output = Result<T>>,
    ) -> Result<T> {
        let record = self
            .file
            .records
            .iter()
            .find(|record| record.id == id)
            .context("No stored value for selected local-storage ID and key")?;
        let _lock = self.lock()?;
        let _value = record.decrypt(agent, &self.key, id).await?;
        let validated = validation.await?;
        ensure!(
            read(&self.path)? == self.baseline,
            "Local-storage changed on disk; reload before reusing"
        );
        Ok(validated)
    }

    /// Remove only the selected ID; retain the per-key file even when empty.
    /// Missing IDs/files are a no-op. Call only after configuration deletion.
    pub fn remove(&mut self, id: &str) -> Result<()> {
        self.remove_checked(id, |_| Ok(()))
    }

    pub(crate) fn remove_checked(
        &mut self,
        id: &str,
        boundary: impl FnMut(WriteBoundary) -> Result<()>,
    ) -> Result<()> {
        validate_id(id)?;
        let _lock = self.lock()?;
        let mut next = self.file.clone();
        next.records.retain(|record| record.id != id);
        if next.records.len() == self.file.records.len() {
            return Ok(());
        }
        self.commit(next, boundary)
    }

    fn lock(&self) -> Result<File> {
        let dir = self.path.parent().context("Missing storage directory")?;
        check_path(dir)?;
        let config_dir = dir
            .parent()
            .and_then(Path::parent)
            .context("Missing configuration directory")?;
        // Establish missing config ancestors without writing configuration.
        // Existing ancestors need not be private, but must not be symlinks.
        let missing: Vec<_> = config_dir
            .ancestors()
            .take_while(|path| !path.as_os_str().is_empty() && !path.exists())
            .collect();
        for path in missing.into_iter().rev() {
            check_path(path)?;
            match fs::DirBuilder::new().mode(0o700).create(path) {
                Ok(()) => {
                    File::open(path.parent().context("Missing directory parent")?)?.sync_all()?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
            check_path(path)?;
            ensure!(path.is_dir(), "Invalid configuration directory path type");
        }
        check_path(config_dir)?;
        for path in [dir.parent().context("Missing .psst directory")?, dir] {
            match fs::DirBuilder::new().mode(0o700).create(path) {
                Ok(()) => {
                    // Persist new directory entries before storing ciphertext.
                    File::open(path.parent().context("Missing directory parent")?)?.sync_all()?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
            private(path, true)?;
        }
        let path = self.path.with_extension("lock");
        check_path(&path)?;
        let lock = options().create(true).write(true).mode(0o600).open(&path)?;
        private_file(&lock)?;
        lock.try_lock_exclusive()
            .context("Local-storage key is locked; try again")?;
        ensure!(
            read(&self.path)? == self.baseline,
            "Local-storage changed on disk; reload before saving"
        );
        Ok(lock)
    }

    fn commit(
        &mut self,
        next: KeyFile,
        boundary: impl FnMut(WriteBoundary) -> Result<()>,
    ) -> Result<()> {
        let bytes = next.encode(&self.key)?;
        // Recheck after signing: uncooperative changes must not be overwritten.
        ensure!(
            read(&self.path)? == self.baseline,
            "Local-storage changed on disk; reload before saving"
        );
        let result = atomic_write_checked(&self.path, &bytes, boundary);
        if result.as_ref().err().is_none_or(|error| error.changed) {
            self.baseline = Some(bytes);
            self.file = next;
        }
        result.map_err(|error: PersistenceError| error.into())
    }
}

fn options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    options
}

fn private_file(file: &File) -> Result<()> {
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file(),
        "Local-storage path is not a regular file"
    );
    ensure!(
        metadata.permissions().mode() & 0o077 == 0,
        "Local-storage file permissions are not private"
    );
    Ok(())
}

fn private(path: &Path, directory: bool) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "Symlinked local-storage path refused"
    );
    ensure!(
        metadata.is_dir() == directory,
        "Invalid local-storage path type"
    );
    ensure!(
        metadata.permissions().mode() & 0o077 == 0,
        "Local-storage directory permissions are not private"
    );
    Ok(())
}

// Check all existing components, including ancestors of the selected config.
// Cooperative locks do not protect against a hostile process swapping
// directories.
fn check_path(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => ensure!(
                !metadata.file_type().is_symlink(),
                "Symlinked local-storage path refused"
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

fn read(path: &Path) -> Result<Option<Vec<u8>>> {
    check_path(path)?;
    let dir = path.parent().context("Missing storage directory")?;
    for directory in [dir.parent().context("Missing .psst directory")?, dir] {
        if directory.exists() {
            private(directory, true)?;
        }
    }
    match options().open(path) {
        Ok(file) => {
            private_file(&file)?;
            let mut bytes = Vec::new();
            file.take(FILE_LIMIT as u64 + 1).read_to_end(&mut bytes)?;
            ensure!(
                bytes.len() <= FILE_LIMIT,
                "Local-storage key file exceeds 4 MiB"
            );
            Ok(Some(bytes))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use data_encoding::BASE64;

    #[test]
    fn failed_commit_tracks_actual_replacement_and_loaded_state() {
        let fixture: KeyFile = serde_json::from_slice(include_bytes!(
            "../../../tests/fixtures/local-storage-v1.json"
        ))
        .unwrap();
        let key =
            PublicKey::from_blob(&BASE64.decode(fixture.public_key.as_bytes()).unwrap()).unwrap();
        for existing in [false, true] {
            for failure in [
                WriteBoundary::Write,
                WriteBoundary::FileSync,
                WriteBoundary::Replace,
                WriteBoundary::DirectorySync,
            ] {
                let dir = tempfile::tempdir().unwrap();
                let root = dir.path().canonicalize().unwrap();
                let mut stored = StoredKey::load(&root, key.clone()).unwrap();
                let _lock = stored.lock().unwrap();
                if existing {
                    stored.commit(fixture.clone(), |_| Ok(())).unwrap();
                }
                let before = stored.baseline.clone();
                let next = if existing {
                    KeyFile::new(&key)
                } else {
                    fixture.clone()
                };
                let expected = next.encode(&key).unwrap();
                let error = stored
                    .commit(next.clone(), |stage| {
                        ensure!(stage != failure, "Injected persistence failure");
                        Ok(())
                    })
                    .unwrap_err();
                let changed = error.downcast_ref::<PersistenceError>().unwrap().changed;
                assert_eq!(changed, failure == WriteBoundary::DirectorySync);
                assert_eq!(
                    read(&stored.path).unwrap(),
                    if changed { Some(expected) } else { before }
                );
                assert_eq!(stored.records().len(), usize::from(existing != changed));
                // The loaded baseline follows completed changes, even on error.
                stored.commit(next, |_| Ok(())).unwrap();
                KeyFile::parse(&fs::read(&stored.path).unwrap(), &key, &key.filename()).unwrap();
            }
        }
    }
}
