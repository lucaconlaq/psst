#![cfg(unix)]
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use psst::config::{Config, Entry, PersistenceError};
use psst::providers::local_storage::{Agent, Envelope, StoredKey};
use psst::providers::{self, Identity};
use zeroize::Zeroizing;

#[allow(dead_code)]
mod support;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn author_save_deadline_includes_queued_work_without_retry() {
    let fake = support::agent::FakeAgent::new().await;
    let root = std::env::temp_dir().canonicalize().unwrap();
    let dir = tempfile::tempdir_in(root).unwrap();
    fake.delay();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .env_remove("SSH_AUTH_SOCK")
        .env("SSH_AUTH_SOCK", fake.socket())
        .env("PSST_TIMEOUT", "1")
        .env("PSST_TEST_AUTHOR_ROOT", dir.path())
        .args(["--exact", "author_deadline_worker", "--nocapture"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let start = Instant::now();
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if start.elapsed() > Duration::from_secs(10) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("Author deadline worker exceeded its finite budget");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!((1..=2).contains(&fake.signs())); // No retry/fallback.
    assert!(!dir.path().join(".psst").join(fake.key.filename()).exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn author_rechecks_records_added_during_signing() {
    for (malformed, newly_loaded) in [(false, false), (true, false), (false, true)] {
        let fake = support::agent::FakeAgent::with_seeds(&[7, 8]).await;
        let root = std::env::temp_dir().canonicalize().unwrap();
        let dir = tempfile::tempdir_in(&root).unwrap();
        let staging = tempfile::tempdir_in(&root).unwrap();
        let other = fake.client.identities().await.unwrap().remove(1);
        StoredKey::load(staging.path(), other.clone())
            .unwrap()
            .set(&fake.client, "id", "OTHER_VALUE")
            .await
            .unwrap();
        // Consume the earlier signing notification before pausing creation.
        fake.requested().await;
        let identities = fake.state.identities.lock().unwrap().clone();
        if newly_loaded {
            // B's same-ID record exists before save, but only A is loaded.
            let target = dir.path().join(".psst");
            use std::os::unix::fs::PermissionsExt;
            std::fs::create_dir_all(&target).unwrap();
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o700)).unwrap();
            std::fs::copy(
                staging.path().join(".psst").join(other.filename()),
                target.join(other.filename()),
            )
            .unwrap();
            fake.state.identities.lock().unwrap().truncate(1);
        }
        fake.delay();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .env_remove("SSH_AUTH_SOCK")
            .env("SSH_AUTH_SOCK", fake.socket())
            .env("PSST_TIMEOUT", "10")
            .env("PSST_TEST_AUTHOR_RECHECK_ROOT", dir.path())
            .env("PSST_TEST_AUTHOR_MALFORMED", malformed.to_string())
            .args(["--exact", "author_recheck_worker", "--nocapture"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        fake.requested().await;
        // The selected-key writer established the private storage directories.
        // Persist another key's file while approval is paused, without signing.
        let target = dir.path().join(".psst").join(other.filename());
        std::fs::copy(staging.path().join(".psst").join(other.filename()), &target).unwrap();
        if malformed {
            std::fs::write(&target, b"invalid JSON").unwrap();
        }
        if newly_loaded {
            *fake.state.identities.lock().unwrap() = identities;
        }
        fake.release();
        let start = Instant::now();
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if start.elapsed() > Duration::from_secs(15) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("Author recheck worker exceeded its finite budget");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let result = child.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[tokio::test]
async fn author_recheck_worker() {
    let Some(root) = std::env::var_os("PSST_TEST_AUTHOR_RECHECK_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let agent = Agent::from_env().unwrap();
    let key = agent.identities().await.unwrap().remove(0);
    let identity = Identity {
        fingerprint: key.fingerprint(),
        token: key.blob(),
    };
    let author = providers::get("local-storage").unwrap().author().unwrap();
    let result = author
        .prepare(&root, "id", &identity)
        .unwrap()
        .save(Zeroizing::new("SELECTED_VALUE".into()))
        .await;
    let mut config = Config::load(root.join("psst.json")).unwrap();
    if std::env::var("PSST_TEST_AUTHOR_MALFORMED").unwrap() == "true" {
        let error = result.unwrap_err();
        assert!(error.downcast_ref::<PersistenceError>().unwrap().changed);
        assert!(error.to_string().contains("reference validation failed"));
        assert!(!config.path.exists());
        let stored = StoredKey::load(&root, key.clone()).unwrap();
        let record = stored
            .records()
            .iter()
            .find(|record| record.id == "id")
            .unwrap();
        assert_eq!(
            &*record.decrypt(&agent, &key, "id").await.unwrap(),
            "SELECTED_VALUE"
        );
    } else {
        let reference = result.unwrap();
        assert_eq!(reference, format!("id#{}", key.fingerprint()));
        let entry = Entry {
            source: "local-storage".into(),
            value: reference,
        };
        config
            .save([("SECRET".into(), entry)].into_iter().collect())
            .unwrap();
        let loaded = Config::load(config.path.clone()).unwrap();
        assert_eq!(
            &*providers::fetch(&loaded.entries["SECRET"], &root)
                .await
                .unwrap(),
            "SELECTED_VALUE"
        );
        let unqualified = Entry {
            source: "local-storage".into(),
            value: "id".into(),
        };
        assert!(
            providers::fetch(&unqualified, &root)
                .await
                .unwrap_err()
                .to_string()
                .contains("Ambiguous")
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn author_post_sign_listing_failure_deadline_and_cancellation() {
    use std::sync::atomic::Ordering;

    for mode in ["refuse", "timeout", "cancel", "removed"] {
        let fake = support::agent::FakeAgent::new().await;
        let root = std::env::temp_dir().canonicalize().unwrap();
        let dir = tempfile::tempdir_in(root).unwrap();
        fake.delay();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .env_remove("SSH_AUTH_SOCK")
            .env("SSH_AUTH_SOCK", fake.socket())
            .env("PSST_TIMEOUT", "2")
            .env("PSST_TEST_AUTHOR_LIST_ROOT", dir.path())
            .env("PSST_TEST_AUTHOR_LIST_MODE", mode)
            .args(["--exact", "author_listing_worker", "--nocapture"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        fake.requested().await;
        fake.state.delay_listing.store(1, Ordering::SeqCst);
        fake.release();
        tokio::time::timeout(
            Duration::from_secs(5),
            fake.state.listing_requested.notified(),
        )
        .await
        .unwrap();
        // Both signatures completed; re-listing must finish before replacement.
        assert_eq!(fake.signs(), 2);
        assert!(!dir.path().join(".psst").join(fake.key.filename()).exists());
        match mode {
            "refuse" => {
                fake.state
                    .listing_fault
                    .store(support::agent::Fault::Refuse as usize, Ordering::SeqCst);
                fake.state.listing_gate.add_permits(1);
            }
            "removed" => {
                fake.state.identities.lock().unwrap().clear();
                fake.state.listing_gate.add_permits(1);
            }
            "cancel" => std::fs::write(dir.path().join("cancel"), b"").unwrap(),
            _ => {}
        }
        let start = Instant::now();
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if start.elapsed() > Duration::from_secs(10) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("Author listing worker exceeded its finite budget");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let result = child.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(fake.signs(), 2); // No retries or fallback.
        assert!(!dir.path().join(".psst").join(fake.key.filename()).exists());
        assert!(!dir.path().join("psst.json").exists());
    }
}

#[tokio::test]
async fn author_listing_worker() {
    let Some(root) = std::env::var_os("PSST_TEST_AUTHOR_LIST_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let mode = std::env::var("PSST_TEST_AUTHOR_LIST_MODE").unwrap();
    let agent = Agent::from_env().unwrap();
    let key = agent.identities().await.unwrap().remove(0);
    let identity = Identity {
        fingerprint: key.fingerprint(),
        token: key.blob(),
    };
    let author = providers::get("local-storage").unwrap().author().unwrap();
    let pending = author.prepare(&root, "id", &identity).unwrap();
    let task = tokio::spawn(pending.save(Zeroizing::new("SELECTED_VALUE".into())));
    if mode == "cancel" {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !root.join("cancel").exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
    } else {
        let error = task.await.unwrap().unwrap_err();
        assert!(error.downcast_ref::<PersistenceError>().is_none());
        assert!(error.to_string().contains(match mode.as_str() {
            "refuse" => "refused identity listing",
            "removed" => "no longer loaded",
            _ => "timed out",
        }));
    }
    assert!(StoredKey::load(&root, key).unwrap().records().is_empty());
    assert!(!root.join("psst.json").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn author_updates_preserve_ciphertext_on_failure() {
    for mode in [
        "success", "refuse", "timeout", "queued", "cancel", "absent", "locked", "conflict",
    ] {
        let fake = support::agent::FakeAgent::new().await;
        let root = std::env::temp_dir().canonicalize().unwrap();
        let dir = tempfile::tempdir_in(root).unwrap();
        StoredKey::load(dir.path(), fake.key.clone())
            .unwrap()
            .set(&fake.client, "id", "EXACT_SECRET")
            .await
            .unwrap();
        let path = dir.path().join(".psst").join(fake.key.filename());
        let before = std::fs::read(&path).unwrap();
        let initial_signs = fake.signs();
        match mode {
            "refuse" => fake.fault(support::agent::Fault::Refuse),
            "timeout" | "queued" | "cancel" => fake.delay(),
            "absent" => fake.state.identities.lock().unwrap().clear(),
            _ => {}
        }
        let mut child = Command::new(std::env::current_exe().unwrap())
            .env_remove("SSH_AUTH_SOCK")
            .env("SSH_AUTH_SOCK", fake.socket())
            .env("PSST_TIMEOUT", "1")
            .env("PSST_TEST_UPDATE_ROOT", dir.path())
            .env("PSST_TEST_UPDATE_MODE", mode)
            .env(
                "PSST_TEST_UPDATE_KEY",
                data_encoding::BASE64.encode(&fake.key.blob()),
            )
            .args(["--exact", "author_update_worker", "--nocapture"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let start = Instant::now();
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if start.elapsed() > Duration::from_secs(10) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("Update worker exceeded finite budget: {mode}");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let result = child.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "{mode}: {}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        let after = std::fs::read(&path).unwrap();
        let signs = fake.signs() - initial_signs;
        if mode == "success" {
            assert_ne!(after, before);
            assert_eq!(signs, 2);
            let stored = StoredKey::load(dir.path(), fake.key.clone()).unwrap();
            assert_eq!(
                &*stored.records()[0]
                    .decrypt(&fake.client, &fake.key, "id")
                    .await
                    .unwrap(),
                "REPLACEMENT_SECRET"
            );
        } else {
            assert_eq!(after, before);
            // Queued mode includes an independently active signing phase.
            assert!(
                signs <= if mode == "queued" { 2 } else { 1 },
                "No retry/fallback: {mode}"
            );
        }
        if matches!(mode, "absent" | "locked" | "conflict") {
            assert_eq!(signs, 0);
        }
    }
}

#[tokio::test]
async fn author_update_worker() {
    use psst::providers::local_storage::PublicKey;
    let Some(root) = std::env::var_os("PSST_TEST_UPDATE_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let mode = std::env::var("PSST_TEST_UPDATE_MODE").unwrap();
    let key = PublicKey::from_blob(
        &data_encoding::BASE64
            .decode(std::env::var("PSST_TEST_UPDATE_KEY").unwrap().as_bytes())
            .unwrap(),
    )
    .unwrap();
    let identity = Identity {
        fingerprint: key.fingerprint(),
        token: key.blob(),
    };
    let author = providers::get("local-storage").unwrap().author().unwrap();
    let mismatched = format!("id#SHA256:{}", data_encoding::BASE64_NOPAD.encode(&[0; 32]));
    assert!(author.prepare(&root, &mismatched, &identity).is_err());
    let pending = author.prepare(&root, "id", &identity).unwrap();
    assert!(pending.replacing());
    assert!(pending.matches_reference(&format!("id#{}", key.fingerprint())));
    let path = root.join(".psst").join(key.filename());
    let before = std::fs::read(&path).unwrap();
    let lock = std::fs::OpenOptions::new()
        .write(true)
        .open(path.with_extension("lock"))
        .unwrap();
    if mode == "locked" {
        fs2::FileExt::try_lock_exclusive(&lock).unwrap();
    }
    if mode == "conflict" {
        std::fs::write(&path, b"concurrent edit").unwrap();
    }
    let active = if mode == "queued" {
        let agent = Agent::from_env().unwrap();
        let key = key.clone();
        let task =
            tokio::spawn(async move { Envelope::encrypt(&agent, &key, "active", "value").await });
        tokio::time::sleep(Duration::from_millis(100)).await;
        Some(task)
    } else {
        None
    };
    let start = Instant::now();
    let task = tokio::spawn(pending.save(Zeroizing::new("REPLACEMENT_SECRET".into())));
    if mode == "cancel" {
        tokio::time::sleep(Duration::from_millis(100)).await;
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
    } else {
        let result = task.await.unwrap();
        if mode == "success" {
            assert_eq!(result.unwrap(), "id");
        } else {
            let error = result.unwrap_err();
            assert!(error.downcast_ref::<PersistenceError>().is_none());
            if matches!(mode.as_str(), "timeout" | "queued") {
                assert!(error.to_string().contains("timed out"));
                assert!(start.elapsed() < Duration::from_millis(1600));
            }
        }
    }
    if let Some(active) = active {
        assert!(active.await.unwrap().is_err());
    }
    if mode == "conflict" {
        assert_eq!(std::fs::read(&path).unwrap(), b"concurrent edit");
        std::fs::write(&path, &before).unwrap(); // Restore only our injected edit.
    }
    if mode == "success" {
        assert_ne!(std::fs::read(&path).unwrap(), before);
    } else {
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    assert!(!root.join("psst.json").exists());
}

#[tokio::test]
async fn author_deadline_worker() {
    let Some(root) = std::env::var_os("PSST_TEST_AUTHOR_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let agent = Agent::from_env().unwrap();
    let key = agent.identities().await.unwrap().remove(0);
    let identity = Identity {
        fingerprint: key.fingerprint(),
        token: key.blob(),
    };
    let author = providers::get("local-storage").unwrap().author().unwrap();
    for id in ["", "bad#id", "bad\n", "bad\0"] {
        assert!(author.prepare(&root, id, &identity).is_err());
    }
    let pending = author.prepare(&root, "id", &identity).unwrap();
    assert!(!pending.replacing());
    let active_key = key.clone();
    let active =
        tokio::spawn(
            async move { Envelope::encrypt(&agent, &active_key, "active", "value").await },
        );
    // Give the delayed active phase the signing mutex before starting save.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let start = Instant::now();
    let error = pending
        .save(Zeroizing::new("EXACT_SECRET".into()))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("timed out"));
    assert!(start.elapsed() < Duration::from_millis(1600));
    assert!(active.await.unwrap().is_err());
    assert!(!root.join(key.filename()).exists());
}
