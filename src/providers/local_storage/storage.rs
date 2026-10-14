//! Config-relative, cooperative per-key persistence. No cross-file transaction.

use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use data_encoding::{BASE64_NOPAD, HEXLOWER};
use fs2::FileExt;

use super::{Agent, Envelope, FILE_LIMIT, KeyFile, PublicKey, decode, validate_id, validate_value};
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
        let path = config_dir.join(".psst").join(key.filename());
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

    /// Remove only the selected ID; retain the per-key file even when empty.
    /// Missing IDs/files are a no-op. Mapping deletion must save configuration
    /// first; independent browser deletion keeps mappings.
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
        let config_dir = dir.parent().context("Missing configuration directory")?;
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
        match fs::DirBuilder::new().mode(0o700).create(dir) {
            Ok(()) => {
                // Persist the new directory entry before storing ciphertext.
                File::open(config_dir)?.sync_all()?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        private(dir, true)?;
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

// Bound the entire scan, not just individual files. Exceeding either limit
// fails the listing rather than returning an incomplete set of records.
const LIST_ENTRY_LIMIT: usize = 4096;
const LIST_BYTE_LIMIT: usize = 64 * 1024 * 1024;

/// Load exactly the stored key selected by a canonical fingerprint, without
/// an agent. Only the decoded hash becomes a path; IDs never do.
pub(super) fn load_selected(config_dir: &Path, fingerprint: &str) -> Result<StoredKey> {
    let encoded = fingerprint
        .strip_prefix("SHA256:")
        .context("Expected OpenSSH SHA256 fingerprint")?;
    let hash = BASE64_NOPAD
        .decode(encoded.as_bytes())
        .context("Invalid OpenSSH fingerprint")?;
    ensure!(
        hash.len() == 32 && BASE64_NOPAD.encode(&hash) == encoded,
        "Invalid OpenSSH fingerprint"
    );
    let filename = format!("{}.json", HEXLOWER.encode(&hash));
    let path = config_dir.join(".psst").join(&filename);
    check_path(&path.with_extension("lock"))?;
    let bytes = read(&path)?.context("Selected local-storage key file is missing")?;
    let file: KeyFile =
        serde_json::from_slice(&bytes).context("Malformed local-storage key file")?;
    let key = PublicKey::from_blob(&decode(&file.public_key, 51, 51)?)?;
    ensure!(
        filename == key.filename(),
        "Mismatched local-storage filename"
    );
    file.validate(&key)?;
    Ok(StoredKey {
        path,
        key,
        file,
        baseline: Some(bytes),
    })
}

/// List validated ID/fingerprint metadata for every stored identity, without
/// contacting an agent or creating directories, files, or locks.
pub(super) fn list(config_dir: &Path) -> Result<Vec<(String, String)>> {
    let dir = config_dir.join(".psst");
    check_path(&dir)?;
    if dir.exists() {
        private(&dir, true)?;
    }
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut bytes_left = LIST_BYTE_LIMIT;
    let mut records = Vec::new();
    for (index, entry) in entries.enumerate() {
        ensure!(
            index < LIST_ENTRY_LIMIT,
            "Too many local-storage directory entries"
        );
        let path = entry?.path();
        check_path(&path)?;
        // Cooperative lock files and atomic-write temporary files are not records.
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        check_path(&path.with_extension("lock"))?;
        let bytes = read(&path)?.context("Local-storage file disappeared during listing")?;
        ensure!(
            bytes.len() <= bytes_left,
            "Local-storage listing exceeds byte limit"
        );
        bytes_left -= bytes.len();
        let file: KeyFile =
            serde_json::from_slice(&bytes).context("Malformed local-storage key file")?;
        let key = PublicKey::from_blob(&decode(&file.public_key, 51, 51)?)?;
        ensure!(
            path.file_name()
                .is_some_and(|name| name == key.filename().as_str()),
            "Mismatched local-storage filename"
        );
        file.validate(&key)?;
        let fingerprint = key.fingerprint();
        records.extend(
            file.records
                .into_iter()
                .map(|record| (record.id, fingerprint.clone())),
        );
    }
    records.sort();
    Ok(records)
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
    if dir.exists() {
        private(dir, true)?;
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

    fn fixture() -> (PublicKey, KeyFile) {
        let file: KeyFile = serde_json::from_slice(include_bytes!(
            "../../../tests/fixtures/local-storage-v1.json"
        ))
        .unwrap();
        let key =
            PublicKey::from_blob(&BASE64.decode(file.public_key.as_bytes()).unwrap()).unwrap();
        (key, file)
    }

    fn store(root: &Path, key: &PublicKey, file: KeyFile) -> PathBuf {
        let mut stored = StoredKey::load(root, key.clone()).unwrap();
        let _lock = stored.lock().unwrap();
        stored.commit(file, |_| Ok(())).unwrap();
        stored.path
    }

    #[tokio::test]
    async fn browser_lists_all_identities_as_qualified_metadata_without_agent() {
        use crate::browser::{Target, list as browse};
        use crate::providers::Provider;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let level = super::super::LocalStorage.browser().unwrap();
        assert!(browse(&level, &root).await.unwrap().is_empty());
        assert!(!root.join(".psst").exists());
        let (key, mut file) = fixture();
        file.records.push(file.records[0].clone());
        file.records[1].id = "another / ID".into();
        let original = store(&root, &key, file.clone());
        let before = fs::read(&original).unwrap();

        let other = PublicKey(
            ed25519_dalek::SigningKey::from_bytes(&[9; 32])
                .verifying_key()
                .to_bytes(),
        );
        let mut other_file = KeyFile::new(&other);
        let mut record = file.records[0].clone();
        record.public_key = other_file.public_key.clone();
        other_file.records.push(record);
        store(&root, &other, other_file);
        let empty = PublicKey(
            ed25519_dalek::SigningKey::from_bytes(&[8; 32])
                .verifying_key()
                .to_bytes(),
        );
        store(&root, &empty, KeyFile::new(&empty));
        let choices = browse(&level, &root).await.unwrap();
        assert_eq!(choices.len(), 3);
        let mut expected = vec![
            ("fixture".to_owned(), key.fingerprint()),
            ("another / ID".to_owned(), key.fingerprint()),
            ("fixture".to_owned(), other.fingerprint()),
        ];
        expected.sort();
        for (choice, (id, fingerprint)) in choices.iter().zip(expected) {
            assert_eq!(choice.label, id);
            assert_eq!(choice.detail, fingerprint);
            let Target::Reference(value) = &choice.target else {
                panic!("Expected a selectable reference");
            };
            assert_eq!(value, &format!("{id}#{fingerprint}"));
            assert!(super::super::reference(value).is_ok());
        }
        assert_eq!(fs::read(original).unwrap(), before);
        let elsewhere = tempfile::tempdir().unwrap();
        assert!(
            browse(&level, &elsewhere.path().canonicalize().unwrap())
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn browser_deletion_selects_stored_key_without_agent_and_keeps_other_records() {
        use crate::providers::Author;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let (key, mut file) = fixture();
        let id = "../exact / ID";
        file.records[0].id = id.into();
        let mut unrelated = file.records[0].clone();
        unrelated.id = "keep".into();
        file.records.push(unrelated);
        let path = store(&root, &key, file.clone());
        let other = PublicKey(
            ed25519_dalek::SigningKey::from_bytes(&[9; 32])
                .verifying_key()
                .to_bytes(),
        );
        let mut other_file = KeyFile::new(&other);
        let mut record = file.records[0].clone();
        record.public_key = other_file.public_key.clone();
        other_file.records.push(record);
        let other_path = store(&root, &other, other_file);
        let other_before = fs::read(&other_path).unwrap();
        let config = root.join("psst.json");
        fs::write(&config, b"configuration must remain unchanged").unwrap();
        let before = fs::read(&path).unwrap();
        let reference = format!("{id}#{}", key.fingerprint());
        // Synchronous selection/removal needs neither a runtime nor an agent.
        let author = super::super::LocalStorage;
        let mut deletion = author.prepare_browser_deletion(&root, &reference).unwrap();
        assert_eq!(fs::read(&path).unwrap(), before);
        deletion.remove(&mut |_| Ok(())).unwrap();
        let remaining = StoredKey::load(&root, key.clone()).unwrap();
        assert_eq!(remaining.records().len(), 1);
        assert_eq!(remaining.records()[0].id, "keep");
        assert_eq!(fs::read(&other_path).unwrap(), other_before);
        assert_eq!(
            fs::read(&config).unwrap(),
            b"configuration must remain unchanged"
        );
        assert!(author.prepare_browser_deletion(&root, &reference).is_err());
        let mut deletion = author
            .prepare_browser_deletion(&root, &format!("keep#{}", key.fingerprint()))
            .unwrap();
        deletion.remove(&mut |_| Ok(())).unwrap();
        assert!(path.exists());
        assert!(StoredKey::load(&root, key).unwrap().records().is_empty());
    }

    #[test]
    fn browser_deletion_requires_qualified_existing_reference() {
        use crate::providers::Author;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let (key, file) = fixture();
        let author = super::super::LocalStorage;
        for value in [
            "fixture".to_owned(),
            "fixture#../../outside".to_owned(),
            format!("fixture#{}=", key.fingerprint()),
            format!("fixture#{}", key.fingerprint()),
        ] {
            assert!(author.prepare_browser_deletion(&root, &value).is_err());
        }
        assert!(!root.join(".psst").exists());
        store(&root, &key, file);
        assert!(
            author
                .prepare_browser_deletion(&root, &format!("missing#{}", key.fingerprint()))
                .is_err()
        );
    }

    #[test]
    fn browser_deletion_rejects_conflicts_locks_and_changed_paths() {
        use crate::providers::Author;
        use std::os::unix::fs::symlink;

        let (key, file) = fixture();
        for failure in ["conflict", "lock", "symlink", "permissions"] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().canonicalize().unwrap();
            let path = store(&root, &key, file.clone());
            let before = fs::read(&path).unwrap();
            let mut deletion = super::super::LocalStorage
                .prepare_browser_deletion(&root, &format!("fixture#{}", key.fingerprint()))
                .unwrap();
            let mut held_lock = None;
            match failure {
                "conflict" => {
                    let mut changed = before.clone();
                    changed.push(b' ');
                    fs::write(&path, changed).unwrap();
                }
                "lock" => {
                    let lock = options()
                        .write(true)
                        .open(path.with_extension("lock"))
                        .unwrap();
                    lock.try_lock_exclusive().unwrap();
                    held_lock = Some(lock);
                }
                "symlink" => {
                    let moved = path.with_extension("moved");
                    fs::rename(&path, &moved).unwrap();
                    symlink(&moved, &path).unwrap();
                }
                _ => fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap(),
            }
            let changed = fs::read(&path).unwrap();
            assert!(deletion.remove(&mut |_| Ok(())).is_err());
            assert_eq!(fs::read(&path).unwrap(), changed);
            drop(held_lock);
        }
    }

    #[test]
    fn browser_deletion_reports_atomic_write_partial_outcomes() {
        use crate::providers::Author;

        let (key, file) = fixture();
        for failure in [
            WriteBoundary::Write,
            WriteBoundary::FileSync,
            WriteBoundary::Replace,
            WriteBoundary::DirectorySync,
        ] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().canonicalize().unwrap();
            let path = store(&root, &key, file.clone());
            let before = fs::read(&path).unwrap();
            let mut deletion = super::super::LocalStorage
                .prepare_browser_deletion(&root, &format!("fixture#{}", key.fingerprint()))
                .unwrap();
            let error = deletion
                .remove(&mut |stage| {
                    ensure!(stage != failure, "Injected deletion failure");
                    Ok(())
                })
                .unwrap_err();
            let changed = error.downcast_ref::<PersistenceError>().unwrap().changed;
            assert_eq!(changed, failure == WriteBoundary::DirectorySync);
            if changed {
                assert!(
                    StoredKey::load(&root, key.clone())
                        .unwrap()
                        .records()
                        .is_empty()
                );
            } else {
                assert_eq!(fs::read(&path).unwrap(), before);
            }
            deletion.remove(&mut |_| Ok(())).unwrap();
            assert!(
                StoredKey::load(&root, key.clone())
                    .unwrap()
                    .records()
                    .is_empty()
            );
        }
    }

    #[test]
    fn listing_rejects_invalid_formats_and_oversized_files() {
        let (key, fixture) = fixture();
        for invalid in [
            b"{}".to_vec(),
            b"{\"version\":1,\"version\":1}".to_vec(),
            {
                let mut file = fixture.clone();
                file.fingerprint = "wrong".into();
                serde_json::to_vec(&file).unwrap()
            },
            {
                let mut file = fixture.clone();
                file.records.push(file.records[0].clone());
                serde_json::to_vec(&file).unwrap()
            },
            {
                let mut file = fixture.clone();
                let mut invalid = file.records[0].clone();
                invalid.id = "unrelated".into();
                invalid.nonce = BASE64.encode(&[0; 11]);
                file.records.push(invalid);
                serde_json::to_vec(&file).unwrap()
            },
            {
                let other = PublicKey(
                    ed25519_dalek::SigningKey::from_bytes(&[9; 32])
                        .verifying_key()
                        .to_bytes(),
                );
                KeyFile::new(&other).encode(&other).unwrap()
            },
            vec![b' '; FILE_LIMIT + 1],
        ] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().canonicalize().unwrap();
            let path = store(&root, &key, fixture.clone());
            fs::write(path, invalid).unwrap();
            assert!(list(&root).is_err());
            assert!(load_selected(&root, &key.fingerprint()).is_err());
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let path = store(&root, &key, fixture);
        fs::rename(&path, path.with_file_name("wrong.json")).unwrap();
        assert!(list(&root).is_err());
    }

    #[test]
    fn listing_rejects_unsafe_paths_and_permissions() {
        use std::os::unix::fs::symlink;

        let (key, file) = fixture();
        for target in ["file", "lock", "storage", "ancestor"] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().canonicalize().unwrap();
            let path = store(&root, &key, file.clone());
            let target = match target {
                "file" => path,
                "lock" => path.with_extension("lock"),
                "storage" => root.join(".psst"),
                _ => {
                    let alias = root.join("alias");
                    symlink(&root, &alias).unwrap();
                    assert!(list(&alias).is_err());
                    assert!(load_selected(&alias, &key.fingerprint()).is_err());
                    continue;
                }
            };
            let moved = target.with_extension("moved");
            fs::rename(&target, &moved).unwrap();
            symlink(&moved, &target).unwrap();
            assert!(list(&root).is_err());
            assert!(load_selected(&root, &key.fingerprint()).is_err());
        }
        for component in ["file", "storage"] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().canonicalize().unwrap();
            let path = store(&root, &key, file.clone());
            let target = match component {
                "file" => path,
                _ => root.join(".psst"),
            };
            fs::set_permissions(target, fs::Permissions::from_mode(0o755)).unwrap();
            assert!(list(&root).is_err());
            assert!(load_selected(&root, &key.fingerprint()).is_err());
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let path = store(&root, &key, file);
        fs::remove_file(&path).unwrap();
        fs::create_dir(path).unwrap();
        assert!(list(&root).is_err());
        assert!(load_selected(&root, &key.fingerprint()).is_err());
    }

    #[test]
    fn storage_is_direct_and_only_storage_requires_private_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
        let (key, file) = fixture();
        let path = store(&root, &key, file);
        assert_eq!(path, root.join(".psst").join(key.filename()));
        assert!(path.with_extension("lock").exists());
        assert!(!root.join(".psst/local-storage").exists());
        assert_eq!(StoredKey::load(&root, key).unwrap().records().len(), 1);
        assert_eq!(list(&root).unwrap().len(), 1);
    }

    #[test]
    fn listing_bounds_aggregate_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        for seed in 0..=(LIST_BYTE_LIMIT / FILE_LIMIT) {
            let key = PublicKey(
                ed25519_dalek::SigningKey::from_bytes(&[seed as u8; 32])
                    .verifying_key()
                    .to_bytes(),
            );
            let file = KeyFile::new(&key);
            let path = store(&root, &key, file.clone());
            let mut bytes = file.encode(&key).unwrap();
            bytes.resize(FILE_LIMIT, b' ');
            fs::write(path, bytes).unwrap();
        }
        assert!(list(&root).unwrap_err().to_string().contains("byte limit"));
    }

    #[test]
    fn listing_bounds_directory_entries_including_ignored_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let (key, file) = fixture();
        let path = store(&root, &key, file);
        let storage = path.parent().unwrap();
        for index in 0..LIST_ENTRY_LIMIT {
            File::create(storage.join(format!("ignored-{index}.lock"))).unwrap();
        }
        assert!(list(&root).unwrap_err().to_string().contains("Too many"));
    }

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
