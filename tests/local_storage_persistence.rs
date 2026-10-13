#![cfg(unix)]

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

use psst::local_storage::{FILE_LIMIT, StoredKey, VALUE_LIMIT};

use support::agent::FakeAgent;

// This suite uses only a subset of the shared protocol fault fixture.
#[allow(dead_code)]
mod support;

#[tokio::test]
async fn create_replace_remove_preserve_ids_users_and_private_permissions() {
    let fake = FakeAgent::new().await;
    let other = FakeAgent::with_seed(8).await;
    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path().canonicalize().unwrap();
    let mut stored = StoredKey::load(&config_dir, fake.key.clone()).unwrap();
    assert!(!dir.path().join(".psst").exists());
    stored
        .set(&fake.client, "../exact ID", " first\n")
        .await
        .unwrap();
    stored.set(&fake.client, "survivor", "🔑").await.unwrap();
    let old = stored.records()[0].clone();
    let mut user = StoredKey::load(&config_dir, other.key.clone()).unwrap();
    user.set(&other.client, "../exact ID", "other user")
        .await
        .unwrap();
    let root = dir.path().join(".psst/local-storage");
    let other_bytes = fs::read(root.join(other.key.filename())).unwrap();
    stored
        .set(&fake.client, "../exact ID", " replacement\n")
        .await
        .unwrap();
    let replacement = stored
        .records()
        .iter()
        .find(|r| r.id == "../exact ID")
        .unwrap();
    assert_ne!(old.salt, replacement.salt);
    assert_ne!(old.nonce, replacement.nonce);
    assert_eq!(
        replacement
            .decrypt(&fake.client, &fake.key, &replacement.id)
            .await
            .unwrap()
            .as_str(),
        " replacement\n"
    );
    stored.remove("../exact ID").unwrap();
    let loaded = StoredKey::load(&config_dir, fake.key.clone()).unwrap();
    assert_eq!(loaded.records().len(), 1);
    assert_eq!(
        loaded.records()[0]
            .decrypt(&fake.client, &fake.key, "survivor")
            .await
            .unwrap()
            .as_str(),
        "🔑"
    );
    assert_eq!(
        fs::read(root.join(other.key.filename())).unwrap(),
        other_bytes
    );
    assert_eq!(
        user.records()[0]
            .decrypt(&other.client, &other.key, "../exact ID")
            .await
            .unwrap()
            .as_str(),
        "other user"
    );
    for (path, mode) in [
        (dir.path().join(".psst"), 0o700),
        (root.clone(), 0o700),
        (root.join(fake.key.filename()), 0o600),
        (root.join(fake.key.filename()).with_extension("lock"), 0o600),
    ] {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            mode
        );
    }
    let before = fs::read(root.join(fake.key.filename())).unwrap();
    stored.remove("missing").unwrap();
    assert_eq!(fs::read(root.join(fake.key.filename())).unwrap(), before);
    user.remove("../exact ID").unwrap();
    assert!(
        StoredKey::load(&config_dir, other.key.clone())
            .unwrap()
            .records()
            .is_empty()
    );
}

#[tokio::test]
async fn loaded_conflicts_and_lock_contention_do_not_sign_or_clobber() {
    use fs2::FileExt;
    let fake = FakeAgent::new().await;
    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path().canonicalize().unwrap();
    let mut a = StoredKey::load(&config_dir, fake.key.clone()).unwrap();
    let mut stale_missing = StoredKey::load(&config_dir, fake.key.clone()).unwrap();
    a.set(&fake.client, "id", "one").await.unwrap();
    let path = dir
        .path()
        .join(".psst/local-storage")
        .join(fake.key.filename());
    let before = fs::read(&path).unwrap();
    let signs = fake.signs();
    assert!(
        stale_missing
            .set(&fake.client, "id", "stale")
            .await
            .is_err()
    );
    assert!(stale_missing.remove("id").is_err());
    let mut stale_existing = StoredKey::load(&config_dir, fake.key.clone()).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path.with_extension("lock"))
        .unwrap();
    lock.try_lock_exclusive().unwrap();
    assert!(a.set(&fake.client, "id", "locked").await.is_err());
    assert!(a.remove("id").is_err());
    assert_eq!(fake.signs(), signs);
    assert_eq!(fs::read(&path).unwrap(), before);
    drop(lock);
    a.set(&fake.client, "id", "two").await.unwrap();
    let signs = fake.signs();
    assert!(
        stale_existing
            .set(&fake.client, "unrelated", "stale")
            .await
            .is_err()
    );
    assert!(stale_existing.remove("id").is_err());
    assert_eq!(fake.signs(), signs);
    assert_eq!(
        StoredKey::load(&config_dir, fake.key.clone())
            .unwrap()
            .records()
            .len(),
        1
    );
}

#[tokio::test]
async fn malformed_oversized_metadata_and_permissions_fail_without_signing() {
    let fake = FakeAgent::new().await;
    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path().canonicalize().unwrap();
    let mut stored = StoredKey::load(&config_dir, fake.key.clone()).unwrap();
    stored.set(&fake.client, "id", "value").await.unwrap();
    let root = dir.path().join(".psst/local-storage");
    let path = root.join(fake.key.filename());
    let good = fs::read(&path).unwrap();
    let mut unknown: serde_json::Value = serde_json::from_slice(&good).unwrap();
    unknown["version"] = 2.into();
    let mut mismatch = unknown.clone();
    mismatch["version"] = 1.into();
    mismatch["fingerprint"] = "wrong".into();
    let mut duplicate: serde_json::Value = serde_json::from_slice(&good).unwrap();
    let record = duplicate["records"][0].clone();
    duplicate["records"].as_array_mut().unwrap().push(record);
    let signs = fake.signs();
    for bytes in [
        b"malformed".to_vec(),
        b"{\"version\":1,\"version\":1}".to_vec(),
        serde_json::to_vec(&unknown).unwrap(),
        serde_json::to_vec(&mismatch).unwrap(),
        serde_json::to_vec(&duplicate).unwrap(),
        vec![b' '; FILE_LIMIT + 1],
    ] {
        fs::write(&path, &bytes).unwrap();
        assert!(StoredKey::load(&config_dir, fake.key.clone()).is_err());
        assert!(stored.set(&fake.client, "id", "new").await.is_err());
        assert!(stored.remove("id").is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    fs::write(&path, &good).unwrap();
    for target in [path.clone(), root.clone(), dir.path().join(".psst")] {
        let mode = fs::metadata(&target).unwrap().permissions().mode();
        fs::set_permissions(&target, fs::Permissions::from_mode(mode | 0o044)).unwrap();
        assert!(StoredKey::load(&config_dir, fake.key.clone()).is_err());
        assert!(stored.set(&fake.client, "id", "new").await.is_err());
        assert!(stored.remove("id").is_err());
        fs::set_permissions(&target, fs::Permissions::from_mode(mode)).unwrap();
    }
    let lock = path.with_extension("lock");
    fs::set_permissions(&lock, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(stored.set(&fake.client, "id", "new").await.is_err());
    assert!(stored.remove("id").is_err());
    assert_eq!(fake.signs(), signs);
    assert_eq!(fs::read(path).unwrap(), good);
}

#[tokio::test]
async fn symlinked_ancestors_directories_records_and_locks_are_refused() {
    let fake = FakeAgent::new().await;
    for relative in [".psst", ".psst/local-storage", "record", "lock"] {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().canonicalize().unwrap();
        let mut stored = StoredKey::load(&config_dir, fake.key.clone()).unwrap();
        stored.set(&fake.client, "id", "value").await.unwrap();
        let record = dir
            .path()
            .join(".psst/local-storage")
            .join(fake.key.filename());
        let target = match relative {
            "record" => record.clone(),
            "lock" => record.with_extension("lock"),
            _ => dir.path().join(relative),
        };
        let moved = dir.path().join("moved");
        fs::rename(&target, &moved).unwrap();
        symlink(&moved, &target).unwrap();
        let signs = fake.signs();
        assert!(StoredKey::load(&config_dir, fake.key.clone()).is_err());
        assert!(stored.set(&fake.client, "id", "new").await.is_err());
        assert!(stored.remove("id").is_err());
        assert_eq!(fake.signs(), signs);
        assert!(
            fs::symlink_metadata(target)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path().canonicalize().unwrap();
    let alias = config_dir.join("alias");
    let real = config_dir.join("real");
    fs::create_dir(&real).unwrap();
    symlink(&real, &alias).unwrap();
    assert!(StoredKey::load(&alias, fake.key.clone()).is_err());
}

#[tokio::test]
async fn aggregate_size_refusal_preserves_whole_file() {
    let fake = FakeAgent::new().await;
    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path().canonicalize().unwrap();
    let mut stored = StoredKey::load(&config_dir, fake.key.clone()).unwrap();
    let value = "x".repeat(VALUE_LIMIT);
    for id in ["a", "b"] {
        stored.set(&fake.client, id, &value).await.unwrap();
    }
    let path = dir
        .path()
        .join(".psst/local-storage")
        .join(fake.key.filename());
    let before = fs::read(&path).unwrap();
    assert!(stored.set(&fake.client, "c", &value).await.is_err());
    assert_eq!(fs::read(path).unwrap(), before);
    assert_eq!(stored.records().len(), 2);
}

#[tokio::test]
async fn cancelling_signing_releases_lock_and_preserves_completed_changes() {
    let fake = FakeAgent::new().await;
    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path().canonicalize().unwrap();
    let mut stored = StoredKey::load(&config_dir, fake.key.clone()).unwrap();
    stored.set(&fake.client, "completed", "kept").await.unwrap();
    let path = dir
        .path()
        .join(".psst/local-storage")
        .join(fake.key.filename());
    let before = fs::read(&path).unwrap();
    fake.requested().await; // Consume the preceding successful phase's notice.
    fake.delay();
    {
        let work = stored.set(&fake.client, "cancelled", "not saved");
        tokio::pin!(work);
        tokio::select! {
            result = &mut work => panic!("unexpected completion: {result:?}"),
            _ = fake.requested() => {}
        }
    }
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(stored.records().len(), 1);
    fake.release();
    stored.set(&fake.client, "later", "works").await.unwrap();
    stored.remove("later").unwrap();
    assert_eq!(
        stored.records()[0]
            .decrypt(&fake.client, &fake.key, "completed")
            .await
            .unwrap()
            .as_str(),
        "kept"
    );
}
