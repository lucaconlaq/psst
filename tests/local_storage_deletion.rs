#![cfg(unix)]
use std::fs;
use std::os::unix::fs::symlink;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use psst::providers;
use psst::providers::local_storage::{PublicKey, StoredKey};

#[allow(dead_code)]
mod support;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deletion_selection_and_snapshot_safety() {
    let fake = support::agent::FakeAgent::with_seeds(&[7, 8]).await;
    let identities = fake.state.identities.lock().unwrap().clone();
    let other = PublicKey::from_blob(&identities[1]).unwrap();
    for mode in [
        "qualified",
        "sole",
        "ambiguous",
        "missing",
        "missing_key",
        "refused",
        "malformed",
        "symlink",
        "lock",
        "conflict",
    ] {
        *fake.state.identities.lock().unwrap() = identities.clone();
        fake.state
            .listing_fault
            .store(0, std::sync::atomic::Ordering::SeqCst);
        let root = std::env::temp_dir().canonicalize().unwrap();
        let dir = tempfile::tempdir_in(root).unwrap();
        for (key, id) in [
            (&fake.key, "id"),
            (&fake.key, "unrelated"),
            (&other, if mode == "sole" { "other" } else { "id" }),
        ] {
            StoredKey::load(dir.path(), key.clone())
                .unwrap()
                .set(&fake.client, id, "NEVER_RENDER")
                .await
                .unwrap();
        }
        let path = dir
            .path()
            .join(".psst/local-storage")
            .join(fake.key.filename());
        if mode == "malformed" {
            fs::write(&path, b"{}").unwrap();
        }
        if mode == "symlink" {
            fs::rename(&path, path.with_extension("original")).unwrap();
            symlink(path.with_extension("original"), &path).unwrap();
        }
        let before = fs::read(&path).unwrap();
        let other_path = path.with_file_name(other.filename());
        let other_before = fs::read(&other_path).unwrap();
        let signs = fake.signs();
        if mode == "missing_key" {
            fake.state.identities.lock().unwrap().remove(0);
        }
        if mode == "refused" {
            fake.state.listing_fault.store(
                support::agent::Fault::Refuse as usize,
                std::sync::atomic::Ordering::SeqCst,
            );
        }
        let mut child = Command::new(std::env::current_exe().unwrap())
            .env_remove("SSH_AUTH_SOCK")
            .env("SSH_AUTH_SOCK", fake.socket())
            .env("PSST_TIMEOUT", "2")
            .env("PSST_DELETE_ROOT", dir.path())
            .env("PSST_DELETE_MODE", mode)
            .env("PSST_DELETE_FP", fake.key.fingerprint())
            .args(["--exact", "deletion_worker", "--nocapture"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let start = Instant::now();
        while child.try_wait().unwrap().is_none() {
            if start.elapsed() > Duration::from_secs(10) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("Deletion worker timed out");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fake.signs(), signs); // Selection/deletion never decrypts.
        assert_eq!(fs::read(&other_path).unwrap(), other_before);
        if !matches!(mode, "qualified" | "sole" | "conflict") {
            assert_eq!(fs::read(&path).unwrap(), before);
        }
        if matches!(mode, "qualified" | "sole") {
            let remaining = StoredKey::load(dir.path(), fake.key.clone()).unwrap();
            assert_eq!(remaining.records().len(), 1);
            assert_eq!(remaining.records()[0].id, "unrelated");
        }
    }
}

#[tokio::test]
async fn deletion_worker() {
    use fs2::FileExt;
    let Some(root) = std::env::var_os("PSST_DELETE_ROOT") else {
        return;
    };
    let dir = std::path::PathBuf::from(root);
    let mode = std::env::var("PSST_DELETE_MODE").unwrap();
    let fp = std::env::var("PSST_DELETE_FP").unwrap();
    let value = match mode.as_str() {
        "sole" | "ambiguous" => "id".to_owned(),
        "missing" => format!("absent#{fp}"),
        _ => format!("id#{fp}"),
    };
    let author = providers::get("local-storage").unwrap().author().unwrap();
    let result = author.prepare_deletion(&dir, &value).await;
    if matches!(
        mode.as_str(),
        "ambiguous" | "missing" | "missing_key" | "refused" | "malformed" | "symlink"
    ) {
        assert!(result.is_err());
        return;
    }
    let mut selected = result.unwrap();
    let key = psst::providers::local_storage::Agent::from_env()
        .unwrap()
        .identities()
        .await
        .unwrap()
        .remove(0);
    let path = dir.join(".psst/local-storage").join(key.filename());
    let lock = if mode == "lock" {
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path.with_extension("lock"))
            .unwrap();
        file.lock_exclusive().unwrap();
        Some(file)
    } else {
        None
    };
    if mode == "conflict" {
        let mut bytes = fs::read(&path).unwrap();
        bytes.push(b' ');
        fs::write(&path, &bytes).unwrap();
    }
    let before = fs::read(&path).unwrap();
    let result = selected.remove(&mut |_| Ok(()));
    if matches!(mode.as_str(), "lock" | "conflict") {
        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    } else {
        result.unwrap();
    }
    drop(lock);
}
