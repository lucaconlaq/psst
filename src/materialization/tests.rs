use super::*;
use crate::config::{Entry, WriteBoundary};

pub(super) fn tempdir() -> tempfile::TempDir {
    // macOS temporary roots can contain symlinks, which destinations reject.
    let root = std::env::temp_dir().canonicalize().unwrap();
    tempfile::tempdir_in(root).unwrap()
}

fn configuration(dir: &Path, names: &[&str]) -> Config {
    let mut config = Config::load(dir.join("custom.json")).unwrap();
    for name in names {
        config.entries.insert(
            (*name).into(),
            Entry {
                kind: MappingKind::File,
                source: "manual".into(),
                value: "NEW_PRIVATE_CONTENTS".into(),
            },
        );
    }
    config
}

fn commit(
    plan: &Plan,
    index: usize,
    created: &mut HashMap<NamespaceKey, Identity>,
) -> Result<Change> {
    let file = &plan.files[index];
    Ok(persist(
        file.result.as_ref().unwrap(),
        &file.destination,
        created,
        |_| Ok(()),
    )?)
}

#[tokio::test]
async fn missing_name_aliases_reject_before_providers_or_staging() {
    for names in [["Output", "output"], ["Missing/Output", "missing/output"]] {
        let root = tempdir();
        let mut config = configuration(root.path(), &names);
        for entry in config.entries.values_mut() {
            entry.source = "shell".into();
            entry.value = "touch fetched; printf PRIVATE".into();
        }
        // Deterministic filesystem-name seam: no destination probe or mutation
        // is needed to exercise rejection on a case-sensitive test host.
        let error = plan_with_names(&config, |path| {
            Ok(PathBuf::from(path.as_os_str().to_ascii_lowercase()))
        })
        .await
        .err()
        .unwrap();
        assert!(error.to_string().contains("alias one another"));
        assert!(!root.path().join("fetched").exists());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }
}

#[tokio::test]
async fn mounted_namespace_aliases_reject_before_fetch() {
    for case in [
        "absent",
        "storage-directory",
        "storage-subdirectory",
        "storage-file",
    ] {
        let root = tempdir();
        let real = root.path().join("real");
        let alias = root.path().join("alias");
        let storage = root.path().join(".psst");
        fs::create_dir(&real).unwrap();
        fs::create_dir(&alias).unwrap();
        fs::create_dir_all(storage.join("nested")).unwrap();
        let ciphertext = storage.join("ciphertext");
        fs::write(&ciphertext, b"encrypted bytes").unwrap();
        assert_eq!(fs::metadata(&ciphertext).unwrap().nlink(), 1);
        let original = fs::read(&ciphertext).unwrap();
        let names = match case {
            "absent" => vec!["real/Missing/Output", "alias/missing/output"],
            "storage-file" => {
                fs::write(alias.join("file"), b"encrypted bytes").unwrap();
                vec!["independent", "alias/file"]
            }
            _ => vec!["independent", "alias/new/output"],
        };
        let mut config = configuration(root.path(), &names);
        for entry in config.entries.values_mut() {
            entry.source = "shell".into();
            entry.value = "touch fetched; printf PRIVATE".into();
        }
        // Model bind mounts by substituting only their read-only dev/inode
        // identities. No mount privileges or symlink approximation is needed.
        let result = plan_with_identity(
            &config,
            |path| Ok(PathBuf::from(path.as_os_str().to_ascii_lowercase())),
            |path| {
                let mapped = match case {
                    "absent" => path.strip_prefix(&alias).ok().map(|p| real.join(p)),
                    "storage-directory" => path.strip_prefix(&alias).ok().map(|p| storage.join(p)),
                    "storage-subdirectory" => path
                        .strip_prefix(&alias)
                        .ok()
                        .map(|p| storage.join("nested").join(p)),
                    "storage-file" if path == alias.join("file") => Some(ciphertext.clone()),
                    _ => None,
                };
                inode(mapped.as_deref().unwrap_or(path))
            },
        )
        .await;
        let error = result.err().unwrap().to_string();
        assert!(error.contains(if case == "absent" {
            "alias one another"
        } else {
            "protected"
        }));
        assert!(!root.path().join("fetched").exists());
        assert!(!root.path().join("independent").exists());
        assert!(!real.join("Missing").exists());
        assert!(!alias.join("new").exists());
        assert_eq!(fs::read(&ciphertext).unwrap(), original);
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 3);
    }
}

#[tokio::test]
async fn equivalent_owned_parents_work_in_both_orders_after_failures() {
    for names in [["New/one", "new/two"], ["new/one", "New/two"]] {
        for fail_first in [false, true] {
            let root = tempdir();
            let planned = plan_with_names(&configuration(root.path(), &names), |path| {
                names::key_with(path, |_| Ok(Some(names::Semantics::AsciiInsensitive)))
            })
            .await
            .unwrap();
            let mut created = HashMap::new();
            let first = &planned.files[0];
            let result = persist(
                first.result.as_ref().unwrap(),
                &first.destination,
                &mut created,
                |at| {
                    if fail_first && at == WriteBoundary::Write {
                        bail!("injected failure");
                    }
                    Ok(())
                },
            );
            assert_eq!(result.is_err(), fail_first);
            // On sensitive hosts, move the owned inode to the second spelling
            // to model the lookup that an insensitive filesystem performs.
            // On native insensitive hosts no emulation is needed.
            let first_parent = first.destination.parent().unwrap();
            let second = &planned.files[1];
            let second_parent = second.destination.parent().unwrap();
            if !second_parent.exists() {
                fs::rename(first_parent, second_parent).unwrap();
            }
            persist(
                second.result.as_ref().unwrap(),
                &second.destination,
                &mut created,
                |_| Ok(()),
            )
            .unwrap();
            assert_eq!(
                fs::read(&second.destination).unwrap(),
                b"NEW_PRIVATE_CONTENTS"
            );
            assert_eq!(
                fs::read_dir(second_parent).unwrap().count(),
                if fail_first { 1 } else { 2 }
            );
        }
    }
}

#[tokio::test]
async fn bind_alias_owned_parents_survive_creation_and_failed_writes() {
    for names in [
        ["real/new/deep/one", "alias/new/deep/two"],
        ["alias/new/deep/one", "real/new/deep/two"],
    ] {
        for fail_first in [false, true] {
            for replace_parent in [false, true] {
                let root = tempdir();
                let real = root.path().join("real");
                let alias = root.path().join("alias");
                fs::create_dir(&real).unwrap();
                fs::create_dir(&alias).unwrap();
                // Model the planning-time bind namespace without privileges.
                // Snapshot safety still observes each actual original path.
                let planned =
                    plan_with_identity(&configuration(root.path(), &names), names::key, |path| {
                        let mapped = path.strip_prefix(&alias).ok().map(|p| real.join(p));
                        inode(mapped.as_deref().unwrap_or(path))
                    })
                    .await
                    .unwrap();
                let staging = planned.staging_dir().to_path_buf();
                let mut created = HashMap::new();
                let first = &planned.files[0];
                let result = persist(
                    first.result.as_ref().unwrap(),
                    &first.destination,
                    &mut created,
                    |at| {
                        if fail_first && at == WriteBoundary::Write {
                            bail!("injected write failure after mkdir");
                        }
                        Ok(())
                    },
                );
                assert_eq!(result.is_err(), fail_first);
                if let Err(error) = result {
                    assert!(!error.changed);
                }
                let first_new = first.destination.parent().unwrap().parent().unwrap();
                let second = &planned.files[1];
                let second_new = second.destination.parent().unwrap().parent().unwrap();
                // Move the same created inodes to emulate bind-alias lookup.
                // Neither alias root's original safety identity changes.
                fs::rename(first_new, second_new).unwrap();
                if replace_parent {
                    fs::rename(second_new, root.path().join("owned-aside")).unwrap();
                    fs::create_dir_all(second_new.join("deep")).unwrap();
                    fs::set_permissions(second_new, fs::Permissions::from_mode(0o700)).unwrap();
                    fs::set_permissions(second_new.join("deep"), fs::Permissions::from_mode(0o700))
                        .unwrap();
                }
                let result = commit(&planned, 1, &mut created);
                assert_eq!(result.is_err(), replace_parent);
                assert_eq!(second.destination.exists(), !replace_parent);
                if !replace_parent {
                    assert_eq!(
                        fs::read(&second.destination).unwrap(),
                        b"NEW_PRIVATE_CONTENTS"
                    );
                    assert_eq!(
                        fs::read_dir(second.destination.parent().unwrap())
                            .unwrap()
                            .count(),
                        if fail_first { 1 } else { 2 }
                    );
                    for path in [second_new.to_path_buf(), second_new.join("deep")] {
                        assert_eq!(fs::metadata(path).unwrap().mode() & 0o7777, 0o700);
                    }
                }
                drop(planned);
                assert!(!staging.exists());
            }
        }
    }
}

#[tokio::test]
async fn native_case_equivalent_owned_parents() {
    for names in [["New/one", "new/two"], ["new/one", "New/two"]] {
        for fail_first in [false, true] {
            let root = tempdir();
            fs::create_dir(root.path().join("Probe")).unwrap();
            let insensitive = root.path().join("probe").exists();
            fs::remove_dir(root.path().join("Probe")).unwrap();
            if !insensitive {
                continue;
            }
            let planned = plan(&configuration(root.path(), &names)).await.unwrap();
            let mut created = HashMap::new();
            let first = &planned.files[0];
            let result = persist(
                first.result.as_ref().unwrap(),
                &first.destination,
                &mut created,
                |at| {
                    if fail_first && at == WriteBoundary::Write {
                        bail!("injected failure");
                    }
                    Ok(())
                },
            );
            assert_eq!(result.is_err(), fail_first);
            commit(&planned, 1, &mut created).unwrap();
        }
    }
}

#[tokio::test]
async fn name_capabilities_gate_the_entire_batch_before_fetch_without_probes() {
    use names::Semantics;

    for unsupported in [
        "unknown",
        "query-error",
        "casefold",
        "mac-sensitive",
        "mac-insensitive",
    ] {
        for destination in ["Output", "é", "e\u{301}", "K", "missing/非ASCII"] {
            let root = tempdir();
            let restricted = root.path().join("restricted");
            fs::create_dir(&restricted).unwrap();
            let existing = root.path().join("existing");
            fs::write(&existing, b"original private bytes").unwrap();
            fs::set_permissions(&existing, fs::Permissions::from_mode(0o600)).unwrap();
            let before = snapshot(&existing).unwrap();
            let name = format!("restricted/{destination}");
            let mut config = configuration(root.path(), &["existing", "independent", &name]);
            for entry in config.entries.values_mut() {
                entry.source = "shell".into();
                entry.value = "touch fetched; printf PRIVATE".into();
            }
            let result = plan_with_names(&config, |path| {
                names::key_with(path, |parent| {
                    if parent.starts_with(&restricted) {
                        match unsupported {
                            "unknown" | "query-error" => bail!("unestablished semantics"),
                            "casefold" => bail!("casefold equivalence unsupported"),
                            "mac-sensitive" => Ok(Some(Semantics::AsciiSensitive)),
                            "mac-insensitive" => Ok(Some(Semantics::AsciiInsensitive)),
                            _ => unreachable!(),
                        }
                    } else {
                        Ok(Some(Semantics::Bytes))
                    }
                })
            })
            .await;
            let supported_ascii = destination == "Output" && unsupported.starts_with("mac-");
            assert_eq!(result.is_ok(), supported_ascii);
            assert_eq!(root.path().join("fetched").exists(), supported_ascii);
            assert_eq!(snapshot(&existing).unwrap(), before);
            assert!(!root.path().join("independent").exists());
            assert_eq!(fs::read_dir(&restricted).unwrap().count(), 0);
            drop(result);
            assert_eq!(
                fs::read_dir(root.path()).unwrap().count(),
                if supported_ascii { 3 } else { 2 }
            );
        }
    }

    // Exact byte semantics support distinct Unicode spellings; successful
    // planning fetches but still creates no destination or missing parents.
    let root = tempdir();
    let mut config = configuration(root.path(), &["é", "e\u{301}", "missing/非ASCII"]);
    for entry in config.entries.values_mut() {
        entry.source = "shell".into();
        entry.value = "touch fetched; printf PRIVATE".into();
    }
    let planned = plan_with_names(&config, |path| {
        names::key_with(path, |_| Ok(Some(Semantics::Bytes)))
    })
    .await
    .unwrap();
    assert!(planned.files.iter().all(|file| file.result.is_ok()));
    assert!(root.path().join("fetched").exists());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}

#[tokio::test]
async fn shared_missing_parents_accept_only_owned_creations_even_after_failure() {
    let root = tempdir();
    let config = configuration(root.path(), &["new/deep/one", "new/two", "new/deep/three"]);
    let planned = plan(&config).await.unwrap();
    let mut created = HashMap::new();
    let first = &planned.files[0];
    let error = persist(
        first.result.as_ref().unwrap(),
        &first.destination,
        &mut created,
        |at| {
            if at == WriteBoundary::Write {
                bail!("injected write failure");
            }
            Ok(())
        },
    )
    .unwrap_err();
    assert!(!error.changed);
    assert!(!first.destination.exists());
    commit(&planned, 1, &mut created).unwrap();
    commit(&planned, 2, &mut created).unwrap();
    assert_eq!(
        fs::read_dir(root.path().join("new/deep")).unwrap().count(),
        1
    );
    for path in ["new", "new/deep"] {
        assert_eq!(
            fs::metadata(root.path().join(path)).unwrap().mode() & 0o7777,
            0o700
        );
    }
    // An unrelated actor's creation is not treated as our parent snapshot.
    let config = configuration(root.path(), &["foreign/one", "independent"]);
    let planned = plan(&config).await.unwrap();
    fs::create_dir(root.path().join("foreign")).unwrap();
    assert!(commit(&planned, 0, &mut HashMap::new()).is_err());
    commit(&planned, 1, &mut HashMap::new()).unwrap();
}

#[tokio::test]
async fn shared_parent_spellings_work_in_both_orders_and_after_failed_write() {
    for names in [
        ["new/one", "sub/../new/two"],
        ["sub/../new/two", "new/one"],
        ["sub/../new/one", "sub/../new/two"],
    ] {
        for (existing_prefix, fail_first) in
            [(true, false), (true, true), (false, false), (false, true)]
        {
            let root = tempdir();
            if existing_prefix {
                fs::create_dir(root.path().join("sub")).unwrap();
            }
            let planned = plan(&configuration(root.path(), &names)).await.unwrap();
            let mut created = HashMap::new();
            if fail_first {
                let first = &planned.files[0];
                let error = persist(
                    first.result.as_ref().unwrap(),
                    &first.destination,
                    &mut created,
                    |at| {
                        if at == WriteBoundary::Write {
                            bail!("injected failure");
                        }
                        Ok(())
                    },
                )
                .unwrap_err();
                assert!(!error.changed);
                assert!(!first.destination.exists());
            } else {
                commit(&planned, 0, &mut created).unwrap();
            }
            commit(&planned, 1, &mut created).unwrap();
            assert_eq!(
                fs::read(&planned.files[1].destination).unwrap(),
                b"NEW_PRIVATE_CONTENTS"
            );
            assert_eq!(
                fs::read_dir(root.path().join("new")).unwrap().count(),
                if fail_first { 1 } else { 2 }
            );
        }
    }
}

#[tokio::test]
async fn missing_prefix_traversal_classifies_targets_and_rechecks_conflicts() {
    for existing in [false, true] {
        for mutate in [false, true] {
            let root = tempdir();
            let target = root.path().join("output");
            if existing {
                fs::write(&target, b"old").unwrap();
            }
            let planned = plan(&configuration(root.path(), &["new/../output", "good"]))
                .await
                .unwrap();
            let prepared = planned.files[0].result.as_ref().unwrap();
            assert_eq!(
                prepared.change,
                if existing {
                    Change::Replace
                } else {
                    Change::New
                }
            );
            assert_eq!(planned.replacements().count(), usize::from(existing));
            assert!(!root.path().join("new").exists());
            if mutate {
                fs::write(&target, b"external").unwrap();
            }
            let mut created = HashMap::new();
            if mutate {
                assert!(commit(&planned, 0, &mut created).is_err());
                assert_eq!(fs::read(&target).unwrap(), b"external");
            } else {
                commit(&planned, 0, &mut created).unwrap();
                assert_eq!(fs::read(&target).unwrap(), b"NEW_PRIVATE_CONTENTS");
                assert_eq!(
                    fs::metadata(root.path().join("new")).unwrap().mode() & 0o7777,
                    0o700
                );
            }
            commit(&planned, 1, &mut created).unwrap();
        }
    }
}

#[tokio::test]
async fn unchanged_traversal_establishes_and_syncs_parents_without_replacing_target() {
    for fail_sync in [false, true] {
        let root = tempdir();
        let target = root.path().join("output");
        fs::write(&target, b"NEW_PRIVATE_CONTENTS").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        let before = snapshot(&target).unwrap();
        let planned = plan(&configuration(root.path(), &["new/deep/../../output"]))
            .await
            .unwrap();
        let file = &planned.files[0];
        assert_eq!(file.result.as_ref().unwrap().change, Change::Unchanged);
        assert!(!file.destination.exists());
        let mut created = HashMap::new();
        let mut syncs = 0;
        let result = persist(
            file.result.as_ref().unwrap(),
            &file.destination,
            &mut created,
            |at| {
                assert!(at == WriteBoundary::DirectorySync);
                syncs += 1;
                if fail_sync {
                    bail!("injected traversal directory sync failure");
                }
                Ok(())
            },
        );
        if fail_sync {
            assert!(!result.unwrap_err().changed);
            assert_eq!(syncs, 1);
            // Retry must sync the already-created entry as well as deep.
            let mut retry_syncs = 0;
            let result = persist(
                file.result.as_ref().unwrap(),
                &file.destination,
                &mut created,
                |at| {
                    assert!(at == WriteBoundary::DirectorySync);
                    retry_syncs += 1;
                    Ok(())
                },
            );
            assert_eq!(result.unwrap(), Change::Unchanged);
            assert_eq!(retry_syncs, 3);
        } else {
            assert_eq!(result.unwrap(), Change::Unchanged);
            // The original traversal revisits new through deep/.., so its
            // directory entry is checked and synced again.
            assert_eq!(syncs, 3);
        }
        assert_eq!(
            fs::read(&file.destination).unwrap(),
            b"NEW_PRIVATE_CONTENTS"
        );
        assert_eq!(snapshot(&target).unwrap().target, before.target);
        for path in ["new", "new/deep"] {
            assert_eq!(
                fs::metadata(root.path().join(path)).unwrap().mode() & 0o7777,
                0o700
            );
        }
        assert_eq!(
            fs::read_dir(root.path().join("new/deep")).unwrap().count(),
            0
        );
    }
}

#[tokio::test]
async fn missing_prefix_traversal_does_not_hide_symlinks_or_nonregular_targets() {
    use std::os::unix::fs::symlink;

    for name in ["new/../link/../output", "new/../link", "new/../directory"] {
        let root = tempdir();
        fs::create_dir(root.path().join("directory")).unwrap();
        symlink(root.path().join("directory"), root.path().join("link")).unwrap();
        let planned = plan(&configuration(root.path(), &[name, "good"]))
            .await
            .unwrap();
        assert!(planned.files[0].result.is_err());
        assert!(!root.path().join("new").exists());
        commit(&planned, 1, &mut HashMap::new()).unwrap();
    }
}

#[tokio::test]
async fn moved_parent_cleanup_removes_populated_temporary_at_replace_boundary() {
    let root = tempdir();
    fs::create_dir(root.path().join("parent")).unwrap();
    let target = root.path().join("parent/file");
    fs::write(&target, b"old").unwrap();
    let planned = plan(&configuration(root.path(), &["parent/file", "good"]))
        .await
        .unwrap();
    let mut created = HashMap::new();
    let error = persist(
        planned.files[0].result.as_ref().unwrap(),
        &target,
        &mut created,
        |at| {
            if at == WriteBoundary::Replace {
                let temporary = fs::read_dir(root.path().join("parent"))?
                    .map(|entry| entry.unwrap().path())
                    .find(|path| {
                        path.file_name()
                            .unwrap()
                            .to_string_lossy()
                            .starts_with(".psst-write-")
                    })
                    .unwrap();
                assert_eq!(fs::read(temporary)?, b"NEW_PRIVATE_CONTENTS");
                fs::rename(root.path().join("parent"), root.path().join("moved"))?;
                fs::create_dir(root.path().join("parent"))?;
            }
            Ok(())
        },
    )
    .unwrap_err();
    assert!(!error.changed);
    assert!(!target.exists());
    assert_eq!(fs::read(root.path().join("moved/file")).unwrap(), b"old");
    assert_eq!(fs::read_dir(root.path().join("moved")).unwrap().count(), 1);
    assert_eq!(fs::read_dir(root.path().join("parent")).unwrap().count(), 0);
    commit(&planned, 1, &mut created).unwrap();
}

#[tokio::test]
async fn stale_targets_and_ancestors_fail_without_clobbering_independent_files() {
    for mutation in ["target", "new", "ancestor"] {
        let root = tempdir();
        fs::create_dir(root.path().join("parent")).unwrap();
        let target = root.path().join("parent/file");
        if mutation != "new" {
            fs::write(&target, b"old").unwrap();
        }
        let config = configuration(root.path(), &["parent/file", "good"]);
        let planned = plan(&config).await.unwrap();
        match mutation {
            "ancestor" => {
                fs::rename(root.path().join("parent"), root.path().join("moved")).unwrap();
                fs::create_dir(root.path().join("parent")).unwrap();
            }
            _ => fs::write(&target, b"external-change").unwrap(),
        }
        let mut created = HashMap::new();
        assert!(commit(&planned, 0, &mut created).is_err());
        commit(&planned, 1, &mut created).unwrap();
        if mutation == "target" || mutation == "new" {
            assert_eq!(fs::read(&target).unwrap(), b"external-change");
        } else {
            assert!(!target.exists());
        }
    }
}

#[tokio::test]
async fn owned_parent_replacement_and_changed_noop_are_still_conflicts() {
    let root = tempdir();
    let config = configuration(root.path(), &["new/one", "new/two", "same", "good"]);
    let same = root.path().join("same");
    fs::write(&same, b"NEW_PRIVATE_CONTENTS").unwrap();
    fs::set_permissions(&same, fs::Permissions::from_mode(0o600)).unwrap();
    let planned = plan(&config).await.unwrap();
    assert_eq!(
        planned.files[2].result.as_ref().unwrap().change,
        Change::Unchanged
    );
    let mut created = HashMap::new();
    commit(&planned, 0, &mut created).unwrap();
    fs::rename(root.path().join("new"), root.path().join("moved")).unwrap();
    fs::create_dir(root.path().join("new")).unwrap();
    fs::write(&same, b"external-change").unwrap();
    assert!(commit(&planned, 1, &mut created).is_err());
    assert!(commit(&planned, 2, &mut created).is_err());
    commit(&planned, 3, &mut created).unwrap();
    assert_eq!(
        fs::read(root.path().join("moved/one")).unwrap(),
        b"NEW_PRIVATE_CONTENTS"
    );
    assert!(!root.path().join("new/two").exists());
    assert_eq!(fs::read(&same).unwrap(), b"external-change");
}

#[tokio::test]
async fn write_faults_cleanup_and_distinguish_completed_replacement() {
    for failure in [
        WriteBoundary::Write,
        WriteBoundary::FileSync,
        WriteBoundary::Replace,
        WriteBoundary::DirectorySync,
    ] {
        let root = tempdir();
        let target = root.path().join("file");
        fs::write(&target, b"old").unwrap();
        let config = configuration(root.path(), &["file", "good"]);
        let planned = plan(&config).await.unwrap();
        let staging = planned.staging_dir().to_owned();
        let file = &planned.files[0];
        let mut created = HashMap::new();
        let error = persist(file.result.as_ref().unwrap(), &target, &mut created, |at| {
            if at == failure {
                bail!("injected persistence failure");
            }
            Ok(())
        })
        .unwrap_err();
        let replaced = failure == WriteBoundary::DirectorySync;
        assert_eq!(error.changed, replaced);
        assert_eq!(
            fs::read(&target).unwrap(),
            if replaced {
                b"NEW_PRIVATE_CONTENTS".as_slice()
            } else {
                b"old".as_slice()
            }
        );
        commit(&planned, 1, &mut created).unwrap();
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
        drop(planned);
        assert!(!staging.exists());
    }
}

#[tokio::test]
async fn ancestor_sync_faults_precede_replacement_and_retry_owned_entries() {
    // The first two syncs establish new in root, then deep in new. The
    // third sync persists the completed destination replacement in deep.
    for failure in 1..=3 {
        let root = tempdir();
        let planned = plan(&configuration(
            root.path(),
            &["new/deep/one", "new/deep/two", "good"],
        ))
        .await
        .unwrap();
        let staging = planned.staging_dir().to_owned();
        let mut created = HashMap::new();
        let first = &planned.files[0];
        let mut syncs = 0;
        let error = persist(
            first.result.as_ref().unwrap(),
            &first.destination,
            &mut created,
            |at| {
                if at == WriteBoundary::DirectorySync {
                    syncs += 1;
                    assert!(root.path().join("new").is_dir());
                    assert_eq!(root.path().join("new/deep").exists(), syncs >= 2);
                    assert_eq!(first.destination.exists(), syncs == 3);
                    if syncs == failure {
                        bail!("injected ancestor/destination directory sync failure");
                    }
                } else {
                    assert_eq!(syncs, 2, "ancestor syncs must precede writing");
                }
                Ok(())
            },
        )
        .unwrap_err();
        assert_eq!(syncs, failure);
        assert_eq!(error.changed, failure == 3);
        assert_eq!(first.destination.exists(), failure == 3);
        if error.changed {
            assert_eq!(
                fs::read(&first.destination).unwrap(),
                b"NEW_PRIVATE_CONTENTS"
            );
        }

        // A sibling must retry both ancestor entries even if they already
        // exist because of the failed attempt, not report false durability.
        let second = &planned.files[1];
        let mut retry_syncs = 0;
        persist(
            second.result.as_ref().unwrap(),
            &second.destination,
            &mut created,
            |at| {
                if at == WriteBoundary::DirectorySync {
                    retry_syncs += 1;
                    assert_eq!(second.destination.exists(), retry_syncs == 3);
                } else {
                    assert_eq!(retry_syncs, 2);
                }
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(retry_syncs, 3);
        assert_eq!(
            fs::read(&second.destination).unwrap(),
            b"NEW_PRIVATE_CONTENTS"
        );
        commit(&planned, 2, &mut created).unwrap();
        assert_eq!(
            fs::read_dir(root.path().join("new/deep")).unwrap().count(),
            if failure == 3 { 2 } else { 1 }
        );
        drop(planned);
        assert!(!staging.exists());
    }
}

#[tokio::test]
async fn final_recheck_catches_changes_during_temporary_write() {
    let root = tempdir();
    let target = root.path().join("file");
    fs::write(&target, b"old").unwrap();
    let config = configuration(root.path(), &["file"]);
    let planned = plan(&config).await.unwrap();
    let error = persist(
        planned.files[0].result.as_ref().unwrap(),
        &target,
        &mut HashMap::new(),
        |at| {
            if at == WriteBoundary::Replace {
                fs::write(&target, b"external-change")?;
            }
            Ok(())
        },
    )
    .unwrap_err();
    assert!(!error.changed);
    assert_eq!(fs::read(&target).unwrap(), b"external-change");
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}
