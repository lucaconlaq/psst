use super::*;
use crate::providers::local_storage::{KeyFile, StoredKey};

async fn encrypted_configuration(root: &Path, fake: &agent::FakeAgent) -> Config {
    StoredKey::load(root, fake.key.clone())
        .unwrap()
        .set(&fake.client, "id", "OLD_PRIVATE")
        .await
        .unwrap();
    let mut config = configuration(root, &["one", "alias", "manual"]);
    for name in ["one", "alias"] {
        config.entries[name].source = "local-storage".into();
        config.entries[name].value = if name == "alias" {
            format!("id#{}", fake.key.fingerprint())
        } else {
            "id".into()
        };
    }
    config.save(config.entries.clone()).unwrap();
    config
}

#[tokio::test]
async fn manual_group_faults_preserve_completed_encrypted_aliases_and_clear_owned_inputs() {
    for failure in [
        WriteBoundary::Write,
        WriteBoundary::FileSync,
        WriteBoundary::Replace,
        WriteBoundary::DirectorySync,
    ] {
        let fake = agent::FakeAgent::new().await;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let mut config = encrypted_configuration(&root, &fake).await;
        config
            .entries
            .insert("manual-two".into(), config.entries["manual"].clone());
        fs::write(root.join("manual-two"), "NEW_PRIVATE").unwrap();
        config.save(config.entries.clone()).unwrap();
        let before = fs::read(&config.path).unwrap();
        let mut plan = Plan::build_with_agent(&config, Some(fake.client.clone())).await;
        plan.persist_encrypted(&config, |_| Ok(())).await;
        let encrypted = root.join(".psst").join(fake.key.filename());
        let completed = fs::read(&encrypted).unwrap();
        let mut replacements = 0;
        plan.persist(&config, |stage| {
            if stage == WriteBoundary::Replace {
                replacements += 1;
            }
            ensure!(stage != failure, "INJECTED_PRIVATE");
            Ok(())
        });
        let changed = failure == WriteBoundary::DirectorySync;
        for file in &plan.files[..2] {
            assert!(matches!(file.outcome, Outcome::Saved));
        }
        for file in &plan.files[2..] {
            let Outcome::Failed(reason) = file.outcome else {
                panic!("Expected atomic manual group failure");
            };
            assert_eq!(reason.contains("durability uncertain"), changed);
            assert!(!reason.contains("PRIVATE"));
        }
        assert!(!plan.report());
        assert!(plan.files.iter().all(|file| file.input.is_none()));
        assert_eq!(fs::read(&encrypted).unwrap(), completed);
        assert_eq!(
            replacements,
            usize::from(matches!(
                failure,
                WriteBoundary::Replace | WriteBoundary::DirectorySync
            ))
        );
        let latest = Config::load(config.path.clone()).unwrap();
        for name in ["manual", "manual-two"] {
            assert_eq!(
                latest.entries[name].value,
                if changed {
                    "NEW_PRIVATE"
                } else {
                    "OLD_PRIVATE"
                }
            );
            assert_eq!(fs::read_to_string(root.join(name)).unwrap(), "NEW_PRIVATE");
        }
        if !changed {
            assert_eq!(fs::read(&config.path).unwrap(), before);
        }
        let (_, lock) = config.reload_locked().unwrap();
        drop(lock);
        StoredKey::load(&root, fake.key.clone())
            .unwrap()
            .set(&fake.client, "id", "RETRY_PRIVATE")
            .await
            .unwrap();
        for directory in [&root, &root.join(".psst")] {
            assert!(fs::read_dir(directory).unwrap().all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".psst-write-")
            }));
        }
    }
}

#[tokio::test]
async fn encrypted_replacement_faults_report_every_alias_and_release_owned_snapshots() {
    for failure in [
        WriteBoundary::Write,
        WriteBoundary::FileSync,
        WriteBoundary::Replace,
        WriteBoundary::DirectorySync,
    ] {
        let fake = agent::FakeAgent::new().await;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let config = encrypted_configuration(&root, &fake).await;
        let path = root.join(".psst").join(fake.key.filename());
        let before = fs::read(&path).unwrap();
        let mut plan = Plan::build_with_agent(&config, Some(fake.client.clone())).await;
        let mut replacements = 0;
        plan.persist_encrypted(&config, |stage| {
            if stage == WriteBoundary::Replace {
                replacements += 1;
            }
            ensure!(stage != failure, "Injected encrypted replacement failure");
            Ok(())
        })
        .await;
        let changed = failure == WriteBoundary::DirectorySync;
        for file in &plan.files[..2] {
            let Outcome::Failed(reason) = file.outcome else {
                panic!("Expected grouped failure");
            };
            assert_eq!(reason.contains("durability uncertain"), changed);
            assert!(file.input.is_none());
        }
        assert_eq!(
            replacements,
            usize::from(matches!(
                failure,
                WriteBoundary::Replace | WriteBoundary::DirectorySync
            ))
        );
        if changed {
            let stored = StoredKey::load(&root, fake.key.clone()).unwrap();
            assert_eq!(
                &*stored.records()[0]
                    .decrypt(&fake.client, &fake.key, "id")
                    .await
                    .unwrap(),
                "NEW_PRIVATE"
            );
        } else {
            assert_eq!(fs::read(&path).unwrap(), before);
        }
        // A failed encrypted target does not prevent the independent manual group.
        plan.persist(&config, |_| Ok(()));
        assert!(matches!(plan.files[2].outcome, Outcome::Saved));
        assert!(plan.files.iter().all(|file| file.input.is_none()));
        // Neither config nor storage locks survive success/failure.
        let (_, lock) = config.reload_locked().unwrap();
        drop(lock);
        StoredKey::load(&root, fake.key.clone())
            .unwrap()
            .set(&fake.client, "id", "RETRY_PRIVATE")
            .await
            .unwrap();
        assert!(fs::read_dir(root.join(".psst")).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".psst-write-")
        }));
    }
}

#[tokio::test]
async fn encrypted_final_boundary_rechecks_all_inputs_mappings_safety_and_loaded_ciphertext() {
    for stage in [
        WriteBoundary::Write,
        WriteBoundary::FileSync,
        WriteBoundary::Replace,
    ] {
        for change in [
            "input",
            "identity",
            "ancestor",
            "protection",
            "mapping",
            "ciphertext",
        ] {
            let fake = agent::FakeAgent::new().await;
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().canonicalize().unwrap();
            let config = encrypted_configuration(&root, &fake).await;
            let path = root.join(".psst").join(fake.key.filename());
            let before = fs::read(&path).unwrap();
            let mut expected = before.clone();
            let mut plan = Plan::build_with_agent(&config, Some(fake.client.clone())).await;
            plan.persist_encrypted(&config, |boundary| {
                if boundary == stage {
                    match change {
                        "input" => fs::write(root.join("alias"), "CHANGED_PRIVATE")?,
                        "identity" => {
                            fs::rename(root.join("alias"), root.join("old"))?;
                            fs::write(root.join("alias"), "NEW_PRIVATE")?;
                        }
                        "ancestor" => {
                            // Replace the configuration directory by a symlink
                            // without changing the input or ciphertext bytes.
                            let moved = root.with_extension("moved");
                            fs::rename(&root, &moved)?;
                            symlink(&moved, &root)?;
                        }
                        "protection" => {
                            symlink(root.join("alias"), root.join(".psst/reachable"))?;
                        }
                        "mapping" => {
                            let mut latest = Config::load(config.path.clone())?;
                            latest.entries["alias"].value = "removed".into();
                            fs::write(&config.path, serde_json::to_vec(&latest.entries)?)?;
                        }
                        _ => {
                            expected.push(b' ');
                            fs::write(&path, &expected)?;
                        }
                    }
                }
                Ok(())
            })
            .await;
            assert!(
                plan.files[..2]
                    .iter()
                    .all(|file| matches!(file.outcome, Outcome::Failed(_))),
                "{change}"
            );
            assert_eq!(fs::read(&path).unwrap(), expected, "{change}");
            assert!(plan.files[..2].iter().all(|file| file.input.is_none()));
            if change == "ancestor" {
                fs::remove_file(&root).unwrap();
                fs::rename(root.with_extension("moved"), &root).unwrap();
            }
            plan.persist(&config, |_| Ok(()));
            assert!(matches!(plan.files[2].outcome, Outcome::Saved), "{change}");
        }
    }
}

#[tokio::test]
async fn encrypted_encoded_format_bound_fails_before_replacement_and_preserves_other_records() {
    let fake = agent::FakeAgent::new().await;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let config = encrypted_configuration(&root, &fake).await;
    let mut stored = StoredKey::load(&root, fake.key.clone()).unwrap();
    let large = Zeroizing::new("x".repeat(VALUE_LIMIT));
    for id in ["other1", "other2"] {
        stored.set(&fake.client, id, &large).await.unwrap();
    }
    let path = root.join(".psst").join(fake.key.filename());
    let before = fs::read(&path).unwrap();
    for name in ["one", "alias"] {
        fs::write(root.join(name), large.as_bytes()).unwrap();
    }
    let mut plan = Plan::build_with_agent(&config, Some(fake.client.clone())).await;
    let mut writes = 0;
    plan.persist_encrypted(&config, |_| {
        writes += 1;
        Ok(())
    })
    .await;
    assert_eq!(writes, 0);
    assert!(
        plan.files[..2]
            .iter()
            .all(|file| matches!(file.outcome, Outcome::Failed(_)))
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(
        KeyFile::parse(&before, &fake.key, &fake.key.filename())
            .unwrap()
            .records
            .len(),
        3
    );
}

#[tokio::test]
async fn cancelling_active_encrypted_save_releases_locks_sockets_and_pending_buffers() {
    use futures::FutureExt;
    use std::sync::atomic::Ordering;

    let fake = agent::FakeAgent::new().await;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let config = encrypted_configuration(&root, &fake).await;
    let path = root.join(".psst").join(fake.key.filename());
    let before = fs::read(&path).unwrap();
    let mut plan = Plan::build_with_agent(&config, Some(fake.client.clone())).await;
    let pause_after = fake.signs() + 1;
    fake.state.delay_after.store(pause_after, Ordering::SeqCst);
    fake.delay();
    // The owned future is dropped while an agent request is in flight. There
    // are no detached tasks, and the same operation can immediately retry.
    let mut work = Box::pin(plan.persist_encrypted(&config, |_| Ok(())));
    tokio::select! {
        _ = &mut work => panic!("Delayed save unexpectedly completed"),
        _ = async {
            while fake.state.signs.load(Ordering::SeqCst) <= pause_after { tokio::task::yield_now().await; }
        } => {}
    }
    assert!(work.as_mut().now_or_never().is_none());
    drop(work);
    // Dropping the parent batch also drops its zeroizing input owners.
    drop(plan);
    assert_eq!(fs::read(&path).unwrap(), before);
    let (_, lock) = config.reload_locked().unwrap();
    drop(lock);
    fake.release();
    StoredKey::load(&root, fake.key.clone())
        .unwrap()
        .set(&fake.client, "id", "RETRY_PRIVATE")
        .await
        .unwrap();
}
