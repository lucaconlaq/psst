use super::*;
use crate::config::{Entry, WriteBoundary};
use std::time::{Duration, Instant};

fn configuration(root: &Path, destinations: &[&str]) -> Config {
    let mut config = Config::load(root.join("custom.json")).unwrap();
    for name in destinations {
        config.entries.insert(
            (*name).into(),
            Entry {
                kind: MappingKind::File,
                source: "manual".into(),
                value: "PRIVATE_PAYLOAD".repeat(4096),
            },
        );
    }
    config
}

#[tokio::test]
async fn staging_faults_remove_partial_files_and_preserve_independent_results() {
    for failure in [WriteBoundary::Write, WriteBoundary::FileSync] {
        let root = tempfile::tempdir().unwrap();
        let config = configuration(root.path(), &["before", "fault", "after"]);
        let mut writes = 0;
        let planned = plan_with_staging(&config, names::key, inode, |at| {
            if at == WriteBoundary::Write {
                writes += 1;
            }
            if writes == 4 && at == failure {
                bail!("injected staging failure");
            }
            Ok(())
        })
        .await
        .unwrap();
        let stage = planned.staging_dir().to_owned();
        assert!(planned.files[0].result.is_ok());
        assert!(planned.files[1].result.is_err());
        assert!(planned.files[2].result.is_ok());
        assert_eq!(fs::read_dir(&stage).unwrap().count(), 2);
        assert!(!persist_plan(&planned, false, |_| std::future::ready(()), |_| Ok(())).await);
        for name in ["before", "after"] {
            assert_eq!(
                fs::read(root.path().join(name)).unwrap(),
                "PRIVATE_PAYLOAD".repeat(4096).as_bytes()
            );
        }
        assert!(!root.path().join("fault").exists());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
        drop(planned);
        assert!(!stage.exists());
    }
}

#[test]
fn fault_reports_distinguish_replacement_and_never_include_contents() {
    let output = assert_cmd::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "materialization::adversarial::populated_temporary_faults_preserve_bytes_and_best_effort_status",
            "--nocapture",
        ])
        .timeout(Duration::from_secs(15))
        .assert()
        .success()
        .get_output()
        .clone();
    let report = String::from_utf8_lossy(&output.stderr);
    assert_eq!(report.matches("failed before replacement:").count(), 6);
    assert_eq!(
        report
            .matches("failed: replacement completed; durability uncertain:")
            .count(),
        2
    );
    assert_eq!(report.matches("good: written").count(), 8);
    assert!(!report.contains("PRIVATE_PAYLOAD") && !report.contains("ORIGINAL_PRIVATE"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_PAYLOAD"));
}

#[tokio::test]
async fn populated_temporary_faults_preserve_bytes_and_best_effort_status() {
    for existing in [false, true] {
        for failure in [
            WriteBoundary::Write,
            WriteBoundary::FileSync,
            WriteBoundary::Replace,
            WriteBoundary::DirectorySync,
        ] {
            let root = tempfile::tempdir().unwrap();
            let target = root.path().join("fault");
            if existing {
                fs::write(&target, b"ORIGINAL_PRIVATE").unwrap();
            }
            let planned = plan(&configuration(root.path(), &["fault", "good"]))
                .await
                .unwrap();
            let stage = planned.staging_dir().to_owned();
            let mut injected = false;
            let mut writes = 0;
            let success = persist_plan(
                &planned,
                true,
                |_| std::future::ready(()),
                |at| {
                    if at == WriteBoundary::Write {
                        writes += 1;
                    }
                    if !injected && at == failure && (at != WriteBoundary::Write || writes == 2) {
                        let temporary = fs::read_dir(root.path())?
                            .map(|entry| entry.unwrap().path())
                            .find(|path| {
                                path.file_name()
                                    .unwrap()
                                    .to_string_lossy()
                                    .starts_with(".psst-write-")
                            });
                        if at == WriteBoundary::DirectorySync {
                            assert!(temporary.is_none());
                            assert_eq!(fs::metadata(&target)?.len(), 61440);
                        } else {
                            let temporary = temporary.unwrap();
                            // Write failure is injected after a real partial copy.
                            assert_eq!(fs::metadata(&temporary)?.mode() & 0o7777, 0o600);
                            assert_eq!(
                                fs::metadata(&temporary)?.len(),
                                if at == WriteBoundary::Write {
                                    16384
                                } else {
                                    61440
                                }
                            );
                        }
                        injected = true;
                        bail!("injected persistence failure");
                    }
                    Ok(())
                },
            )
            .await;
            assert!(injected && !success);
            if failure == WriteBoundary::DirectorySync {
                assert_eq!(
                    fs::read(&target).unwrap(),
                    "PRIVATE_PAYLOAD".repeat(4096).as_bytes()
                );
            } else if existing {
                assert_eq!(fs::read(&target).unwrap(), b"ORIGINAL_PRIVATE");
            } else {
                assert!(!target.exists());
            }
            assert_eq!(
                fs::read(root.path().join("good")).unwrap(),
                "PRIVATE_PAYLOAD".repeat(4096).as_bytes()
            );
            assert_eq!(
                fs::read_dir(root.path()).unwrap().count(),
                usize::from(target.exists()) + 1
            );
            drop(planned);
            assert!(!stage.exists());
        }
    }
}

#[tokio::test]
async fn appeared_and_atomically_replaced_targets_reject_stale_consent() {
    for existing in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target");
        if existing {
            fs::write(&target, b"ORIGINAL_PRIVATE").unwrap();
        }
        let planned = plan(&configuration(root.path(), &["target", "good"]))
            .await
            .unwrap();
        let mut mutated = false;
        let success = persist_plan(
            &planned,
            true,
            |_| std::future::ready(()),
            |at| {
                if at == WriteBoundary::Replace && !mutated {
                    // Even identical original bytes do not authorize replacing
                    // an inode installed by another writer since planning.
                    fs::write(root.path().join("competing"), b"ORIGINAL_PRIVATE")?;
                    fs::rename(root.path().join("competing"), &target)?;
                    mutated = true;
                }
                Ok(())
            },
        )
        .await;
        assert!(mutated && !success);
        assert_eq!(fs::read(&target).unwrap(), b"ORIGINAL_PRIVATE");
        assert_eq!(
            fs::read(root.path().join("good")).unwrap(),
            "PRIVATE_PAYLOAD".repeat(4096).as_bytes()
        );
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
        let staging = planned.staging_dir().to_owned();
        drop(planned);
        assert!(!staging.exists());
    }
}

#[tokio::test]
async fn symlink_races_at_final_boundary_refuse_changed_paths_only() {
    use std::os::unix::fs::symlink;
    for ancestor in [false, true] {
        for existing in [false, true] {
            let root = tempfile::tempdir().unwrap();
            fs::create_dir(root.path().join("parent")).unwrap();
            fs::create_dir(root.path().join("outside")).unwrap();
            let target = root.path().join("parent/file");
            let outside = root.path().join("outside/file");
            fs::write(&outside, b"EXTERNAL_PRIVATE").unwrap();
            if existing {
                fs::write(&target, b"ORIGINAL_PRIVATE").unwrap();
            }
            let planned = plan(&configuration(root.path(), &["parent/file", "good"]))
                .await
                .unwrap();
            let mut mutated = false;
            let success = persist_plan(
                &planned,
                true,
                |_| std::future::ready(()),
                |at| {
                    if at == WriteBoundary::Replace && !mutated {
                        mutated = true;
                        if ancestor {
                            fs::rename(root.path().join("parent"), root.path().join("moved"))?;
                            symlink(root.path().join("outside"), root.path().join("parent"))?;
                        } else {
                            if existing {
                                fs::remove_file(&target)?;
                            }
                            symlink(&outside, &target)?;
                        }
                    }
                    Ok(())
                },
            )
            .await;
            assert!(mutated && !success);
            assert_eq!(fs::read(&outside).unwrap(), b"EXTERNAL_PRIVATE");
            if ancestor {
                let moved = root.path().join("moved");
                assert_eq!(fs::read_dir(&moved).unwrap().count(), usize::from(existing));
                if existing {
                    assert_eq!(fs::read(moved.join("file")).unwrap(), b"ORIGINAL_PRIVATE");
                }
            } else {
                assert!(
                    fs::symlink_metadata(&target)
                        .unwrap()
                        .file_type()
                        .is_symlink()
                );
                assert_eq!(fs::read_dir(root.path().join("parent")).unwrap().count(), 1);
            }
            assert!(root.path().join("good").is_file());
            let stage = planned.staging_dir().to_owned();
            drop(planned);
            assert!(!stage.exists());
        }
    }
}

#[test]
fn ctrl_c_between_writes_retains_completed_file_and_cleans_staging() {
    let root = tempfile::tempdir().unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "materialization::adversarial::between_writes_child",
            "--nocapture",
        ])
        .env("PSST_BETWEEN_WRITES_FIXTURE", root.path())
        .env("TMPDIR", root.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while !root.path().join("waiting").exists() {
        assert!(child.try_wait().unwrap().is_none());
        if start.elapsed() > Duration::from_secs(10) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("write barrier not reached");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    // The private barrier is after the first complete commit, before the next.
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGINT) }, 0);
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > Duration::from_secs(15) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("cancellation did not finish");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(130));
    let report = String::from_utf8_lossy(&output.stderr);
    assert!(report.contains("first: written"), "{report}");
    assert!(!report.contains("PRIVATE_PAYLOAD"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_PAYLOAD"));
    assert_eq!(
        fs::read(root.path().join("first")).unwrap(),
        "PRIVATE_PAYLOAD".repeat(4096).as_bytes()
    );
    assert!(!report.contains("second: written"));
    assert!(!root.path().join("second").exists());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
}

#[tokio::test]
async fn between_writes_child() {
    let Some(root) = std::env::var_os("PSST_BETWEEN_WRITES_FIXTURE") else {
        return;
    };
    let root = Path::new(&root);
    let planned = plan(&configuration(root, &["first", "second"]))
        .await
        .unwrap();
    let operation = persist_plan(
        &planned,
        true,
        |index| async move {
            if index == 1 {
                fs::write(root.join("waiting"), b"").unwrap();
                std::future::pending::<()>().await;
            }
        },
        |_| Ok(()),
    );
    tokio::select! {
        biased;
        _ = tokio::signal::ctrl_c() => {}
        _ = operation => panic!("barrier unexpectedly completed"),
    }
    drop(planned);
    std::process::exit(130);
}
