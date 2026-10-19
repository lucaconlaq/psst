#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Output, Stdio},
    sync::atomic::Ordering,
    time::Duration,
};

use psst::providers::local_storage::{KeyFile, PublicKey, StoredKey, VALUE_LIMIT};
use serde_json::{Value, json};
use tokio::process::{Child, Command};

use support::agent::{FakeAgent, Fault};

#[allow(dead_code)]
mod support;

fn mapping(reference: &str) -> Value {
    json!({"type":"file", "source":"local-storage", "value":reference})
}

fn configuration(root: &Path, mappings: Value) {
    fs::write(root.join("custom.json"), mappings.to_string()).unwrap();
}

fn fixture(root: &Path, fake: &FakeAgent) -> Vec<u8> {
    let storage = root.join(".psst");
    fs::create_dir_all(&storage).unwrap();
    fs::set_permissions(&storage, fs::Permissions::from_mode(0o700)).unwrap();
    let bytes = include_bytes!("fixtures/local-storage-v1.json").to_vec();
    let path = storage.join(fake.key.filename());
    fs::write(&path, &bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    bytes
}

fn command(root: &Path, fake: &FakeAgent) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("psst"));
    command
        .current_dir(root)
        .env("PSST_CONFIG", root.join("custom.json"))
        .env_remove("PSST_SSH_AUTH_SOCK")
        .env("SSH_AUTH_SOCK", fake.socket())
        .env("PSST_TIMEOUT", "2")
        .env("TMPDIR", root)
        .env("PATH", "/nonexistent")
        .args(["--files-push", "--yes"])
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

async fn finish(child: Child) -> Output {
    tokio::time::timeout(Duration::from_secs(60), child.wait_with_output())
        .await
        .unwrap()
        .unwrap()
}

fn report(output: &Output) -> String {
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!report.contains("PRIVATE"), "{report}");
    report
}

async fn raw(root: &Path, fake: &FakeAgent, key: &PublicKey, id: &str) -> String {
    let stored = StoredKey::load(root, key.clone()).unwrap();
    let value = stored
        .records()
        .iter()
        .find(|record| record.id == id)
        .unwrap()
        .decrypt(&fake.client, key, id)
        .await
        .unwrap();
    value.to_string()
}

fn no_staging(root: &Path) {
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        assert!(!name.starts_with("psst-files-") && !name.starts_with(".psst-write-"));
        if entry.file_type().unwrap().is_dir() {
            no_staging(&entry.path());
        }
    }
}

#[tokio::test]
async fn exact_text_raw_unchanged_and_alias_coalescing_from_another_directory() {
    let fake = FakeAgent::new().await;
    let root = support::tempdir();
    let root = root.path();
    fixture(root, &fake);
    let qualified = format!("fixture#{}", fake.key.fingerprint());
    for contents in [" \n🔑\t", "", " \nNEW_PRIVATE 🔑\t\n"] {
        fs::write(root.join("one"), contents).unwrap();
        fs::write(root.join("alias"), contents).unwrap();
        configuration(
            root,
            json!({"one":mapping("fixture"), "alias":mapping(&qualified)}),
        );
        let before = fake.signs();
        let elsewhere = support::tempdir();
        let output = finish(
            command(root, &fake)
                .current_dir(elsewhere.path())
                .spawn()
                .unwrap(),
        )
        .await;
        assert!(output.status.success(), "{}", report(&output));
        let report = report(&output);
        if contents == " \n🔑\t" {
            assert!(report.contains("one: unchanged") && report.contains("alias: unchanged"));
            assert!(!report.contains("Push local → provider:"));
            assert_eq!(fake.signs() - before, 1);
        } else {
            assert!(report.contains("one: saved") && report.contains("alias: saved"));
            assert_eq!(report.matches("Push local → provider:").count(), 1);
            assert!(report.contains("Shared target: one, alias"));
            // One raw read, latest authentication, and one two-signature update.
            assert_eq!(fake.signs() - before, 4);
        }
        assert_eq!(raw(root, &fake, &fake.key, "fixture").await, contents);
        assert!(!elsewhere.path().join(".psst").exists());
        assert!(!root.join(".psst/local-storage").exists());
        assert_eq!(fs::read_to_string(root.join("one")).unwrap(), contents);
        no_staging(root);
    }
}

#[tokio::test]
async fn conflicts_include_unchanged_inputs_and_unrelated_manual_targets_continue() {
    let fake = FakeAgent::new().await;
    let root = support::tempdir();
    let root = root.path();
    let before = fixture(root, &fake);
    fs::write(root.join("same"), " \n🔑\t").unwrap();
    fs::write(root.join("different"), "NEW_PRIVATE").unwrap();
    fs::write(root.join("manual"), "MANUAL_PRIVATE").unwrap();
    configuration(
        root,
        json!({
            "same":mapping("fixture"),
            "different":mapping(&format!("fixture#{}", fake.key.fingerprint())),
            "manual":{"type":"file", "source":"manual", "value":"OLD_PRIVATE"}
        }),
    );
    let output = finish(command(root, &fake).spawn().unwrap()).await;
    assert!(!output.status.success());
    let report = report(&output);
    assert!(report.contains("same: failed") && report.contains("different: failed"));
    assert!(report.contains("manual: saved") && report.contains("partially completed"));
    assert_eq!(fake.signs(), 0);
    assert_eq!(
        fs::read(root.join(".psst").join(fake.key.filename())).unwrap(),
        before
    );
    no_staging(root);
}

#[tokio::test]
async fn oversized_alias_blocks_its_resolved_target_but_not_independent_encrypted_saves() {
    for oversized_qualified in [false, true] {
        for alias_contents in [" \n🔑\t", "ALIAS_PRIVATE"] {
            let fake = FakeAgent::new().await;
            let root = support::tempdir();
            let root = root.path();
            fixture(root, &fake);
            StoredKey::load(root, fake.key.clone())
                .unwrap()
                .set(&fake.client, "independent", "OLD_PRIVATE")
                .await
                .unwrap();
            let path = root.join(".psst").join(fake.key.filename());
            let before: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            let qualified = format!("fixture#{}", fake.key.fingerprint());
            let (oversized_reference, alias_reference) = if oversized_qualified {
                (qualified.as_str(), "fixture")
            } else {
                ("fixture", qualified.as_str())
            };
            let oversized = vec![b'x'; VALUE_LIMIT + 1];
            fs::write(root.join("oversized"), &oversized).unwrap();
            fs::write(root.join("alias"), alias_contents).unwrap();
            fs::write(root.join("independent"), "INDEPENDENT_PRIVATE").unwrap();
            configuration(
                root,
                json!({
                    "oversized":mapping(oversized_reference),
                    "alias":mapping(alias_reference),
                    "independent":mapping("independent")
                }),
            );
            let signs = fake.signs();
            let output = finish(command(root, &fake).spawn().unwrap()).await;
            assert!(!output.status.success());
            let report = report(&output);
            assert!(report.contains("oversized: failed") && report.contains("alias: failed"));
            assert!(
                report.contains("independent: saved") && report.contains("partially completed")
            );
            assert_eq!(fake.signs() - signs, 4);
            let after: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            let record = |file: &Value| {
                file["records"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|record| record["id"] == "fixture")
                    .unwrap()
                    .clone()
            };
            assert_eq!(record(&after), record(&before));
            assert_eq!(raw(root, &fake, &fake.key, "fixture").await, " \n🔑\t");
            assert_eq!(
                raw(root, &fake, &fake.key, "independent").await,
                "INDEPENDENT_PRIVATE"
            );
            assert_eq!(fs::read(root.join("oversized")).unwrap(), oversized);
            assert_eq!(
                fs::read_to_string(root.join("alias")).unwrap(),
                alias_contents
            );
            no_staging(root);
        }
    }
}

#[tokio::test]
async fn provider_limit_is_exact_and_oversized_inputs_never_request_signatures() {
    let fake = FakeAgent::new().await;
    let root = support::tempdir();
    let root = root.path();
    fixture(root, &fake);
    configuration(root, json!({"one":mapping("fixture")}));
    for length in [VALUE_LIMIT + 1, VALUE_LIMIT] {
        fs::write(root.join("one"), vec![b'x'; length]).unwrap();
        let output = finish(command(root, &fake).spawn().unwrap()).await;
        assert_eq!(
            output.status.success(),
            length == VALUE_LIMIT,
            "{}",
            report(&output)
        );
        if length > VALUE_LIMIT {
            assert_eq!(fake.signs(), 0);
        } else {
            assert_eq!(
                raw(root, &fake, &fake.key, "fixture").await.len(),
                VALUE_LIMIT
            );
        }
    }
}

#[tokio::test]
async fn encrypted_aggregate_cap_counts_alias_inputs_before_deduplication() {
    let fake = FakeAgent::new().await;
    let root = support::tempdir();
    let root = root.path();
    fixture(root, &fake);
    fs::write(root.join("input"), vec![b'x'; VALUE_LIMIT]).unwrap();
    for count in [64, 65] {
        let mut mappings = serde_json::Map::new();
        for index in 0..count {
            let name = format!("alias-{index}");
            if !root.join(&name).exists() {
                fs::hard_link(root.join("input"), root.join(&name)).unwrap();
            }
            mappings.insert(name, mapping("fixture"));
        }
        configuration(root, Value::Object(mappings));
        let path = root.join(".psst").join(fake.key.filename());
        let before = fs::read(&path).unwrap();
        let signs = fake.signs();
        let output = finish(
            command(root, &fake)
                .env("PSST_TIMEOUT", "30")
                .spawn()
                .unwrap(),
        )
        .await;
        assert_eq!(output.status.success(), count == 64, "{}", report(&output));
        if count == 65 {
            assert_eq!(fake.signs(), signs);
            assert_eq!(fs::read(path).unwrap(), before);
            assert!(report(&output).contains("batch exceeds 64 MiB"));
            assert!(!report(&output).contains("Push local → provider:"));
        } else {
            assert_eq!(fake.signs() - signs, 4);
            assert_eq!(report(&output).matches(": saved").count(), 64);
        }
        no_staging(root);
    }
}

#[tokio::test]
async fn missing_ineligible_ambiguous_and_qualified_keys_never_create_or_fall_back() {
    for case in [
        "missing-record",
        "missing-key",
        "ineligible",
        "ambiguous",
        "qualified",
        "qualified-missing",
        "refuse",
        "format",
        "integrity",
    ] {
        let fake = FakeAgent::with_seeds(&[7, 8]).await;
        let root = support::tempdir();
        let root = root.path();
        let original = fixture(root, &fake);
        let keys = fake.client.identities().await.unwrap();
        let other = &keys[1];
        if matches!(case, "ambiguous" | "qualified" | "qualified-missing") {
            StoredKey::load(root, other.clone())
                .unwrap()
                .set(&fake.client, "fixture", "OTHER_PRIVATE")
                .await
                .unwrap();
        }
        let path = root.join(".psst").join(fake.key.filename());
        match case {
            "missing-key" | "qualified-missing" => {
                fake.state.identities.lock().unwrap().remove(0);
            }
            "ineligible" => fake.state.identities.lock().unwrap().clear(),
            "refuse" => fake.fault(Fault::Refuse),
            "format" => fs::write(&path, b"{}").unwrap(),
            "integrity" => {
                let mut file: KeyFile = serde_json::from_slice(&original).unwrap();
                file.records[0].ciphertext.replace_range(0..1, "A");
                fs::write(&path, file.encode(&fake.key).unwrap()).unwrap();
            }
            _ => {}
        }
        let reference = match case {
            "missing-record" => "missing".into(),
            "qualified" | "qualified-missing" => format!("fixture#{}", fake.key.fingerprint()),
            _ => "fixture".into(),
        };
        fs::write(root.join("one"), "NEW_PRIVATE").unwrap();
        configuration(root, json!({"one":mapping(&reference)}));
        let before = fs::read(&path).unwrap();
        let output = finish(command(root, &fake).spawn().unwrap()).await;
        assert_eq!(
            output.status.success(),
            case == "qualified",
            "{case}: {}",
            report(&output)
        );
        if case != "qualified" {
            assert_eq!(fs::read(&path).unwrap(), before, "{case}");
        }
        assert!(!report(&output).contains("select"));
        assert!(
            !StoredKey::load(root, other.clone())
                .unwrap()
                .records()
                .iter()
                .any(|record| record.id == "missing")
        );
    }
}

#[tokio::test]
async fn absent_agent_and_storage_fail_without_creating_storage() {
    let fake = FakeAgent::new().await;
    let root = support::tempdir();
    fs::write(root.path().join("one"), "NEW_PRIVATE").unwrap();
    configuration(root.path(), json!({"one":mapping("fixture")}));
    for no_agent in [false, true] {
        let mut command = command(root.path(), &fake);
        if no_agent {
            command.env_remove("SSH_AUTH_SOCK");
        }
        let output = finish(command.spawn().unwrap()).await;
        assert!(!output.status.success());
        assert!(report(&output).contains("one: failed"));
        assert!(!root.path().join(".psst").exists());
    }
    assert_eq!(fake.signs(), 0);
}

#[tokio::test]
async fn timeout_and_cancellation_close_active_agent_work_without_writes() {
    for (cancel, pause_after) in [(false, 0), (true, 0), (false, 1), (true, 3)] {
        let fake = FakeAgent::new().await;
        let root = support::tempdir();
        let before = fixture(root.path(), &fake);
        fs::write(root.path().join("one"), "NEW_PRIVATE").unwrap();
        configuration(root.path(), json!({"one":mapping("fixture")}));
        fake.state.delay_after.store(pause_after, Ordering::SeqCst);
        fake.delay();
        let child = command(root.path(), &fake).spawn().unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while fake.signs() <= pause_after {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        if cancel {
            assert_eq!(
                unsafe { libc::kill(child.id().unwrap() as i32, libc::SIGINT) },
                0
            );
        }
        let output = finish(child).await;
        assert!(!output.status.success());
        if cancel {
            assert_eq!(output.status.code(), Some(130));
        }
        assert!(report(&output).contains("one: failed"));
        assert_eq!(fake.signs(), pause_after + 1);
        assert_eq!(
            fs::read(root.path().join(".psst").join(fake.key.filename())).unwrap(),
            before
        );
        tokio::time::timeout(Duration::from_secs(3), async {
            while fake.state.closed.load(Ordering::SeqCst)
                < fake.state.connections.load(Ordering::SeqCst)
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        no_staging(root.path());
    }
}

#[tokio::test]
async fn final_post_sign_changes_fail_without_clobbering_unrelated_manual_work() {
    for (change, pause_after) in [
        ("input", 1),
        ("mapping", 1),
        ("identity", 1),
        ("ambiguous", 1),
        ("ciphertext", 1),
        ("lock", 1),
        ("input", 3),
        ("mapping", 3),
        ("identity", 3),
        ("ambiguous", 3),
        ("ciphertext", 3),
        ("refuse", 3),
    ] {
        let fake = FakeAgent::with_seeds(&[7, 8]).await;
        let root = support::tempdir();
        let root = root.path();
        fixture(root, &fake);
        fs::write(root.join("one"), "NEW_PRIVATE").unwrap();
        fs::write(root.join("manual"), "MANUAL_PRIVATE").unwrap();
        let mappings = json!({"one":mapping("fixture"), "manual":{"type":"file", "source":"manual", "value":"OLD_PRIVATE"}});
        configuration(root, mappings.clone());
        // Pause on latest-state authentication, after planning's raw read.
        fake.state.delay_after.store(pause_after, Ordering::SeqCst);
        fake.delay();
        let child = command(root, &fake).spawn().unwrap();
        // A notification can belong to planning; wait for the paused request.
        tokio::time::timeout(Duration::from_secs(5), async {
            while fake.signs() <= pause_after {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let mut held_lock = None;
        match change {
            "input" => fs::write(root.join("one"), "CHANGED_PRIVATE").unwrap(),
            "mapping" => {
                let mut changed = mappings;
                changed["one"] = mapping("removed");
                configuration(root, changed);
            }
            "identity" => fake.state.identities.lock().unwrap().clear(),
            "refuse" => fake.fault(Fault::Refuse),
            "ambiguous" => {
                let other = fake.client.identities().await.unwrap().remove(1);
                let mut file = KeyFile::new(&other);
                // Valid envelope metadata is enough to create ambiguity; it is
                // never decrypted or selected as a fallback.
                let original: KeyFile =
                    serde_json::from_slice(include_bytes!("fixtures/local-storage-v1.json"))
                        .unwrap();
                let mut record = original.records[0].clone();
                record.public_key = file.public_key.clone();
                file.records.push(record);
                let path = root.join(".psst").join(other.filename());
                fs::write(&path, file.encode(&other).unwrap()).unwrap();
                fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
            }
            "ciphertext" => {
                let path = root.join(".psst").join(fake.key.filename());
                let mut bytes = fs::read(&path).unwrap();
                bytes.push(b' ');
                fs::write(path, bytes).unwrap();
            }
            _ => {
                use fs2::FileExt;
                let path = root
                    .join(".psst")
                    .join(fake.key.filename())
                    .with_extension("lock");
                let lock = fs::OpenOptions::new()
                    .create(true)
                    .truncate(false)
                    .write(true)
                    .open(&path)
                    .unwrap();
                fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
                lock.try_lock_exclusive().unwrap();
                held_lock = Some(lock);
            }
        }
        let path = root.join(".psst").join(fake.key.filename());
        let before = fs::read(&path).unwrap();
        fake.release();
        let output = finish(child).await;
        assert!(!output.status.success(), "{change}: {}", report(&output));
        assert!(
            report(&output).contains("one: failed"),
            "{change}: {}",
            report(&output)
        );
        assert!(report(&output).contains("manual: saved"));
        assert_eq!(fs::read(&path).unwrap(), before, "{change}");
        drop(held_lock);
        no_staging(root);
    }
}

#[tokio::test]
async fn cancellation_during_a_later_encrypted_target_retains_the_completed_write() {
    let fake = FakeAgent::new().await;
    let root = support::tempdir();
    let root = root.path();
    fixture(root, &fake);
    StoredKey::load(root, fake.key.clone())
        .unwrap()
        .set(&fake.client, "second", "SECOND_OLD_PRIVATE")
        .await
        .unwrap();
    fs::write(root.join("one"), "FIRST_NEW_PRIVATE").unwrap();
    fs::write(root.join("two"), "SECOND_NEW_PRIVATE").unwrap();
    configuration(
        root,
        json!({"one":mapping("fixture"), "two":mapping("second")}),
    );
    // Two planning decryptions, then authentication + two signatures for the
    // first save. Pause on the second save's authentication request.
    let pause_after = fake.signs() + 5;
    fake.state.delay_after.store(pause_after, Ordering::SeqCst);
    fake.delay();
    let child = command(root, &fake).spawn().unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while fake.signs() <= pause_after {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        unsafe { libc::kill(child.id().unwrap() as i32, libc::SIGINT) },
        0
    );
    let output = finish(child).await;
    assert_eq!(output.status.code(), Some(130));
    let report = report(&output);
    assert!(report.contains("one: saved") && report.contains("two: failed"));
    assert!(report.contains("partially completed"));
    fake.release();
    assert_eq!(
        raw(root, &fake, &fake.key, "fixture").await,
        "FIRST_NEW_PRIVATE"
    );
    assert_eq!(
        raw(root, &fake, &fake.key, "second").await,
        "SECOND_OLD_PRIVATE"
    );
    no_staging(root);
}

#[tokio::test]
async fn encrypted_write_survives_failed_manual_configuration_save() {
    let fake = FakeAgent::new().await;
    let root = support::tempdir();
    let root = root.path();
    fixture(root, &fake);
    fs::write(root.join("encrypted"), "ENCRYPTED_PRIVATE").unwrap();
    fs::write(root.join("manual"), vec![b'\n'; 3 * 1024 * 1024]).unwrap();
    configuration(
        root,
        json!({"encrypted":mapping("fixture"), "manual":{"type":"file", "source":"manual", "value":"OLD_PRIVATE"}}),
    );
    let before = fs::read(root.join("custom.json")).unwrap();
    let output = finish(command(root, &fake).spawn().unwrap()).await;
    assert!(!output.status.success());
    let report = report(&output);
    assert!(report.contains("encrypted: saved") && report.contains("manual: failed"));
    assert!(report.contains("partially completed"));
    assert_eq!(
        raw(root, &fake, &fake.key, "fixture").await,
        "ENCRYPTED_PRIVATE"
    );
    assert_eq!(fs::read(root.join("custom.json")).unwrap(), before);
    no_staging(root);
}
