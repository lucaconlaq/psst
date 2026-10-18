use super::*;
use crate::config::Entry;
use fs2::FileExt;
use std::{fs::OpenOptions, os::unix::fs::symlink};

mod encrypted;

// Reuse the same deterministic Unix agent as binary acceptance tests. The
// crate alias keeps that shared fixture independent of its inclusion site.
#[allow(dead_code)]
mod agent {
    use crate as psst;
    include!("../../tests/support/agent.rs");
}

fn configuration(root: &Path, names: &[&str]) -> Config {
    let mut config = Config::load(root.join("custom.json")).unwrap();
    for name in names {
        config.entries.insert(
            (*name).into(),
            Entry {
                kind: MappingKind::File,
                source: "manual".into(),
                value: "OLD_PRIVATE".into(),
            },
        );
        fs::write(root.join(name), "NEW_PRIVATE").unwrap();
    }
    config.save(config.entries.clone()).unwrap();
    config
}

#[tokio::test]
async fn latest_values_are_overwritten_and_unrelated_mappings_preserved_in_one_save() {
    let root = tempfile::tempdir().unwrap();
    let config = configuration(root.path(), &["one", "two"]);
    let mut plan = Plan::build(&config).await;
    let mut latest = Config::load(config.path.clone()).unwrap();
    latest.entries["one"].value = "CONCURRENT_PRIVATE".into();
    latest.entries.insert(
        "ENV".into(),
        Entry {
            kind: MappingKind::Env,
            source: "manual".into(),
            value: "ENV_PRIVATE".into(),
        },
    );
    latest.entries.insert(
        "added".into(),
        Entry {
            kind: MappingKind::File,
            source: "unknown".into(),
            value: "REFERENCE_PRIVATE".into(),
        },
    );
    latest.save(latest.entries.clone()).unwrap();
    let mut replacements = 0;
    plan.persist(&config, |stage| {
        if stage == WriteBoundary::Replace {
            replacements += 1;
        }
        Ok(())
    });
    assert_eq!(replacements, 1);
    assert!(
        plan.files
            .iter()
            .all(|file| matches!(file.outcome, Outcome::Saved) && file.input.is_none())
    );
    let loaded = Config::load(config.path.clone()).unwrap();
    assert_eq!(loaded.entries["one"].value, "NEW_PRIVATE");
    assert_eq!(loaded.entries["two"].value, "NEW_PRIVATE");
    assert_eq!(loaded.entries["ENV"].value, "ENV_PRIVATE");
    assert_eq!(loaded.entries["added"].value, "REFERENCE_PRIVATE");
}

#[tokio::test]
async fn input_identity_contents_ancestors_and_protection_are_rechecked_independently() {
    for mutation in 0..6 {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("parent")).unwrap();
        let config = configuration(root.path(), &["parent/one", "two"]);
        let mut plan = Plan::build(&config).await;
        let path = root.path().join("parent/one");
        match mutation {
            0 => fs::write(&path, "REREAD_PRIVATE").unwrap(),
            1 => {
                fs::rename(&path, root.path().join("old")).unwrap();
                fs::write(&path, "NEW_PRIVATE").unwrap();
            }
            2 => {
                fs::rename(root.path().join("parent"), root.path().join("old")).unwrap();
                fs::create_dir(root.path().join("parent")).unwrap();
                fs::write(&path, "NEW_PRIVATE").unwrap();
            }
            3 => {
                fs::rename(root.path().join("parent"), root.path().join("old")).unwrap();
                symlink(root.path().join("old"), root.path().join("parent")).unwrap();
            }
            4 => {
                fs::create_dir(root.path().join(".psst")).unwrap();
                fs::hard_link(&path, root.path().join(".psst/record")).unwrap();
            }
            _ => {
                fs::remove_file(&path).unwrap();
                symlink(root.path().join("two"), &path).unwrap();
            }
        }
        plan.persist(&config, |_| Ok(()));
        assert!(
            matches!(plan.files[0].outcome, Outcome::Failed(_)),
            "mutation {mutation}"
        );
        assert!(matches!(plan.files[1].outcome, Outcome::Saved));
        let latest = Config::load(config.path.clone()).unwrap();
        assert_eq!(latest.entries["parent/one"].value, "OLD_PRIVATE");
        assert_eq!(latest.entries["two"].value, "NEW_PRIVATE");
    }
}

#[tokio::test]
async fn write_and_sync_mutations_rebuild_the_group_at_final_replacement() {
    for boundary in [WriteBoundary::Write, WriteBoundary::FileSync] {
        for mutation in 0..5 {
            let root = tempfile::tempdir().unwrap();
            fs::create_dir(root.path().join("parent")).unwrap();
            let config = configuration(root.path(), &["parent/one", "two"]);
            let mut plan = Plan::build(&config).await;
            let path = root.path().join("parent/one");
            let mut injected = false;
            let mut replacements = 0;
            let mut completed = 0;
            plan.persist(&config, |stage| {
                if stage == boundary && !injected {
                    injected = true;
                    match mutation {
                        0 => fs::write(&path, "REREAD_PRIVATE")?,
                        1 => {
                            fs::rename(&path, root.path().join("old"))?;
                            fs::write(&path, "NEW_PRIVATE")?;
                        }
                        2 => {
                            fs::rename(root.path().join("parent"), root.path().join("old"))?;
                            fs::create_dir(root.path().join("parent"))?;
                            fs::write(&path, "NEW_PRIVATE")?;
                        }
                        3 => {
                            fs::rename(root.path().join("parent"), root.path().join("old"))?;
                            symlink(root.path().join("old"), root.path().join("parent"))?;
                        }
                        _ => {
                            // A reachable storage alias changes protection
                            // without changing the input's own metadata.
                            fs::create_dir(root.path().join(".psst"))?;
                            symlink(&path, root.path().join(".psst/record"))?;
                        }
                    }
                }
                if stage == WriteBoundary::Replace {
                    replacements += 1;
                }
                if stage == WriteBoundary::DirectorySync {
                    completed += 1;
                }
                Ok(())
            });
            assert!(injected);
            assert_eq!(replacements, 2);
            assert_eq!(completed, 1);
            assert!(matches!(plan.files[0].outcome, Outcome::Failed(_)));
            assert!(matches!(plan.files[1].outcome, Outcome::Saved));
            assert!(plan.files.iter().all(|file| file.input.is_none()));
            let latest = Config::load(config.path.clone()).unwrap();
            assert_eq!(latest.entries["parent/one"].value, "OLD_PRIVATE");
            assert_eq!(latest.entries["two"].value, "NEW_PRIVATE");
            assert!(!fs::read_dir(root.path()).unwrap().any(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".tmp")
            }));
        }
    }
}

#[tokio::test]
async fn final_mutations_can_exclude_the_entire_group_without_replacement() {
    let root = tempfile::tempdir().unwrap();
    let config = configuration(root.path(), &["one", "two"]);
    let original = fs::read(&config.path).unwrap();
    let mut plan = Plan::build(&config).await;
    plan.persist(&config, |stage| {
        if stage == WriteBoundary::FileSync {
            fs::write(root.path().join("one"), "CHANGED_PRIVATE")?;
            fs::write(root.path().join("two"), "CHANGED_PRIVATE")?;
        }
        ensure!(stage != WriteBoundary::DirectorySync, "Must not replace");
        Ok(())
    });
    assert!(
        plan.files
            .iter()
            .all(|file| matches!(file.outcome, Outcome::Failed(_)))
    );
    assert_eq!(fs::read(&config.path).unwrap(), original);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 4);
}

#[tokio::test]
async fn dangling_storage_entries_do_not_block_unrelated_saves() {
    for timing in 0..3 {
        let root = tempfile::tempdir().unwrap();
        let config = configuration(root.path(), &["one", "two"]);
        fs::create_dir(root.path().join(".psst")).unwrap();
        let dangling = root.path().join(".psst/dangling");
        let absent = root.path().join("absent");
        if timing == 0 {
            symlink(&absent, &dangling).unwrap();
        }
        let mut plan = Plan::build(&config).await;
        assert!(
            plan.files
                .iter()
                .all(|file| matches!(file.outcome, Outcome::Pending))
        );
        if timing == 1 {
            // Approval-time mutation, after the plan is already complete.
            symlink(&absent, &dangling).unwrap();
        }
        plan.persist(&config, |stage| {
            if timing == 2 && stage == WriteBoundary::FileSync {
                symlink(&absent, &dangling)?;
            }
            Ok(())
        });
        assert!(
            plan.files
                .iter()
                .all(|file| matches!(file.outcome, Outcome::Saved))
        );
        let latest = Config::load(config.path.clone()).unwrap();
        assert_eq!(latest.entries["one"].value, "NEW_PRIVATE");
        assert_eq!(latest.entries["two"].value, "NEW_PRIVATE");
        let protected = Protected::load(&latest).unwrap();
        assert!(protected.check(&dangling).is_err());
    }
}

#[tokio::test]
async fn unresolvable_storage_aliases_still_fail_closed() {
    let root = tempfile::tempdir().unwrap();
    let config = configuration(root.path(), &["one"]);
    fs::create_dir(root.path().join(".psst")).unwrap();
    let alias = root.path().join(".psst/loop");
    symlink(&alias, &alias).unwrap();
    assert!(Protected::load(&config).is_err());
    let plan = Plan::build(&config).await;
    assert!(matches!(plan.files[0].outcome, Outcome::Failed(_)));
}

#[tokio::test]
async fn removed_and_retargeted_mappings_fail_without_rejecting_other_files() {
    for mutation in 0..3 {
        let root = tempfile::tempdir().unwrap();
        let config = configuration(root.path(), &["one", "two"]);
        let mut plan = Plan::build(&config).await;
        let mut latest = Config::load(config.path.clone()).unwrap();
        match mutation {
            0 => {
                latest.entries.shift_remove("one");
            }
            1 => latest.entries["one"].source = "shell".into(),
            _ => latest.entries["one"].kind = MappingKind::Env,
        }
        latest.save(latest.entries.clone()).unwrap();
        plan.persist(&config, |_| Ok(()));
        assert!(matches!(plan.files[0].outcome, Outcome::Failed(_)));
        assert!(matches!(plan.files[1].outcome, Outcome::Saved));
    }
}

#[tokio::test]
async fn grouped_persistence_failures_distinguish_replacement_and_clear_owned_plaintext() {
    for failure in [
        WriteBoundary::Write,
        WriteBoundary::FileSync,
        WriteBoundary::Replace,
        WriteBoundary::DirectorySync,
    ] {
        let root = tempfile::tempdir().unwrap();
        let config = configuration(root.path(), &["one", "two"]);
        let mut plan = Plan::build(&config).await;
        plan.persist(&config, |stage| {
            ensure!(stage != failure, "INJECTED_PRIVATE");
            Ok(())
        });
        let changed = failure == WriteBoundary::DirectorySync;
        for file in &plan.files {
            let Outcome::Failed(reason) = file.outcome else {
                panic!("expected grouped failure")
            };
            assert_eq!(reason.contains("durability uncertain"), changed);
            assert!(!reason.contains("PRIVATE"));
            assert!(file.input.is_none());
        }
        let latest = Config::load(config.path.clone()).unwrap();
        assert_eq!(
            latest.entries["one"].value,
            if changed {
                "NEW_PRIVATE"
            } else {
                "OLD_PRIVATE"
            }
        );
        assert_eq!(latest.entries["two"].value, latest.entries["one"].value);
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 4);
    }
}

#[tokio::test]
async fn final_conflicts_and_lock_contention_never_clobber_competing_writes() {
    for contention in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let config = configuration(root.path(), &["one", "two"]);
        let mut plan = Plan::build(&config).await;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(config.lock_path())
            .unwrap();
        if contention {
            lock.lock_exclusive().unwrap();
        }
        plan.persist(&config, |stage| {
            if stage == WriteBoundary::Replace {
                fs::write(&config.path, "{}").unwrap();
            }
            Ok(())
        });
        assert!(
            plan.files
                .iter()
                .all(|file| matches!(file.outcome, Outcome::Failed(_)))
        );
        if !contention {
            assert_eq!(fs::read_to_string(&config.path).unwrap(), "{}");
        }
    }
}

#[tokio::test]
async fn unsafe_configuration_or_lock_paths_fail_closed() {
    for mutation in 0..3 {
        let root = tempfile::tempdir().unwrap();
        let config = configuration(root.path(), &["one"]);
        let original = fs::read(&config.path).unwrap();
        let mut plan = Plan::build(&config).await;
        if mutation == 0 {
            fs::rename(&config.path, root.path().join("other.json")).unwrap();
            symlink(root.path().join("other.json"), &config.path).unwrap();
        } else if mutation == 1 {
            fs::remove_file(config.lock_path()).unwrap();
            symlink(root.path().join("one"), config.lock_path()).unwrap();
        }
        plan.persist(&config, |stage| {
            if mutation == 2 && stage == WriteBoundary::Replace {
                fs::rename(&config.path, root.path().join("other.json"))?;
                symlink(root.path().join("other.json"), &config.path)?;
            }
            Ok(())
        });
        assert!(matches!(plan.files[0].outcome, Outcome::Failed(_)));
        assert_eq!(fs::read(&config.path).unwrap(), original);
        assert_eq!(fs::read(root.path().join("one")).unwrap(), b"NEW_PRIVATE");
    }
}

#[tokio::test]
async fn persisted_format_limit_is_enforced_before_writing() {
    let root = tempfile::tempdir().unwrap();
    let config = configuration(root.path(), &["one"]);
    // JSON escaping makes a valid input exceed the persisted configuration cap.
    fs::write(root.path().join("one"), vec![b'\n'; INPUT_LIMIT]).unwrap();
    let original = fs::read(&config.path).unwrap();
    let mut plan = Plan::build(&config).await;
    let mut writes = 0;
    plan.persist(&config, |_| {
        writes += 1;
        Ok(())
    });
    assert_eq!(writes, 0);
    assert!(matches!(plan.files[0].outcome, Outcome::Failed(_)));
    assert_eq!(fs::read(&config.path).unwrap(), original);
}

#[tokio::test]
async fn cancellation_drops_owned_zeroizing_snapshots_before_any_persistence() {
    use futures::FutureExt;
    let root = tempfile::tempdir().unwrap();
    let config = configuration(root.path(), &["one", "two"]);
    let original = fs::read(&config.path).unwrap();
    let plan = Plan::build(&config).await;
    assert!(plan.files.iter().all(|file| file.input.is_some()));
    // The pending future owns the plan, exactly like waiting for CLI approval.
    let mut work = Box::pin(async move {
        let _owned = plan;
        std::future::pending::<()>().await;
    });
    assert!(work.as_mut().now_or_never().is_none());
    drop(work);
    // Input has no custom cleanup: its sole plaintext field is Zeroizing<Vec>.
    // Dropping the future drops that owner, without a detached task or staging.
    assert_eq!(fs::read(&config.path).unwrap(), original);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 4);
}
