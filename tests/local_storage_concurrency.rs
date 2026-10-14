#![cfg(unix)]

use std::fs;
use std::time::Duration;

use psst::providers::local_storage::{KeyFile, StoredKey};

use support::agent::FakeAgent;

#[allow(dead_code)]
mod support;

#[tokio::test]
async fn change_while_signing_is_not_overwritten() {
    let fake = FakeAgent::new().await;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let mut stored = StoredKey::load(&root, fake.key.clone()).unwrap();
    stored.set(&fake.client, "id", "old").await.unwrap();
    let path = dir.path().join(".psst").join(fake.key.filename());
    fake.requested().await;
    fake.delay();
    let work = stored.set(&fake.client, "id", "new");
    tokio::pin!(work);
    tokio::select! {
        result = &mut work => panic!("unexpected completion: {result:?}"),
        _ = fake.requested() => {}
    }
    let externally_changed = KeyFile::new(&fake.key).encode(&fake.key).unwrap();
    fs::write(&path, &externally_changed).unwrap();
    fake.release();
    assert!(
        work.await
            .unwrap_err()
            .to_string()
            .contains("changed on disk")
    );
    assert_eq!(fs::read(path).unwrap(), externally_changed);
}

#[tokio::test]
async fn readers_see_whole_documents_during_repeated_replacement() {
    let fake = FakeAgent::new().await;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let mut stored = StoredKey::load(&root, fake.key.clone()).unwrap();
    stored.set(&fake.client, "id", "old").await.unwrap();
    let path = dir.path().join(".psst").join(fake.key.filename());
    let key = fake.key.clone();
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let finished = done.clone();
    let reader = std::thread::spawn(move || {
        let start = std::time::Instant::now();
        let mut reads = 0;
        while !finished.load(std::sync::atomic::Ordering::SeqCst)
            && start.elapsed() < Duration::from_secs(30)
        {
            let bytes = fs::read(&path).unwrap();
            let file = KeyFile::parse(&bytes, &key, &key.filename()).unwrap();
            assert_eq!(file.records.len(), 1);
            reads += 1;
        }
        reads
    });
    for _ in 0..12 {
        stored
            .set(&fake.client, "id", "replacement\n")
            .await
            .unwrap();
    }
    done.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(reader.join().unwrap() > 0);
}

#[test]
fn save_deadline_runs_with_isolated_timeout_environment() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "save_deadline_worker", "--nocapture"])
        .env_remove("SSH_AUTH_SOCK")
        .env("PSST_TIMEOUT", "1")
        .env("PSST_TEST_STORAGE_DEADLINE", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn save_deadline_worker() {
    if std::env::var_os("PSST_TEST_STORAGE_DEADLINE").is_none() {
        return;
    }
    let fake = FakeAgent::new().await;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let mut stored = StoredKey::load(&root, fake.key.clone()).unwrap();
    stored.set(&fake.client, "id", "old").await.unwrap();
    let path = dir.path().join(".psst").join(fake.key.filename());
    let before = fs::read(&path).unwrap();
    fake.delay();
    let signs = fake.signs();
    let error = stored
        .set(&fake.client, "id", "not saved")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("timed out"));
    assert_eq!(fake.signs(), signs + 1);
    assert_eq!(fs::read(&path).unwrap(), before);
    fake.release();
    stored.set(&fake.client, "id", "later").await.unwrap();
    assert_eq!(
        stored.records()[0]
            .decrypt(&fake.client, &fake.key, "id")
            .await
            .unwrap()
            .as_str(),
        "later"
    );
}
