#![cfg(unix)]

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;
use std::sync::atomic::Ordering;

use data_encoding::BASE64;
use futures::FutureExt;
use psst::config::{Entries, Entry};
use psst::providers;
use psst::providers::local_storage::{Envelope, FILE_LIMIT, KeyFile, PublicKey};
use support::agent::{FakeAgent, Fault};
use tokio::process::Command;

mod support;

fn key(seed: u8) -> PublicKey {
    let signing = ed25519_dalek::SigningKey::from_bytes(&[seed; 32]);
    let mut blob = Vec::new();
    for bytes in [
        b"ssh-ed25519".as_slice(),
        signing.verifying_key().as_bytes(),
    ] {
        blob.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        blob.extend_from_slice(bytes);
    }
    PublicKey::from_blob(&blob).unwrap()
}

fn store(dir: &Path, key: &PublicKey, records: Vec<Envelope>) -> std::path::PathBuf {
    let root = dir.join(".psst");
    fs::create_dir_all(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let mut file = KeyFile::new(key);
    file.records = records;
    let path = root.join(key.filename());
    fs::write(&path, file.encode(key).unwrap()).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    path
}

fn config(dir: &Path, value: &str) -> std::path::PathBuf {
    let path = dir.join("psst.json");
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "GOOD": {"source": "manual", "value": "success"},
            "SECRET": {"source": "local-storage", "value": value}
        }))
        .unwrap(),
    )
    .unwrap();
    path
}

fn command(config: &Path, cwd: &Path, agent: Option<&FakeAgent>) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("psst"));
    command
        .current_dir(cwd)
        .env_remove("PSST_SSH_AUTH_SOCK")
        .env_remove("SSH_AUTH_SOCK")
        .env("PSST_CONFIG", config)
        .env("PSST_TIMEOUT", "1")
        .env("SECRET", "inherited");
    if let Some(agent) = agent {
        command.env("SSH_AUTH_SOCK", agent.socket());
    }
    command
}

async fn output(mut command: Command) -> std::process::Output {
    tokio::time::timeout(std::time::Duration::from_secs(8), command.output())
        .await
        .unwrap()
        .unwrap()
}

async fn fail_stop(config: &Path, cwd: &Path, agent: Option<&FakeAgent>) {
    let marker = cwd.join("launched");
    let mut cmd = command(config, cwd, agent);
    cmd.args(["--", "sh", "-c", "touch launched"]);
    assert!(!output(cmd).await.status.success());
    assert!(!marker.exists());
    let env = cwd.join(".env");
    for existing in [false, true] {
        if existing {
            fs::write(&env, "unchanged").unwrap();
        }
        let mut cmd = command(config, cwd, agent);
        cmd.arg("--write");
        assert!(!output(cmd).await.status.success());
        if existing {
            assert_eq!(fs::read_to_string(&env).unwrap(), "unchanged");
            fs::remove_file(&env).unwrap();
        } else {
            assert!(!env.exists());
        }
    }
    let bin = cwd.join("bin");
    fs::create_dir_all(&bin).unwrap();
    for helper in ["wl-copy", "xclip", "xsel", "pbcopy", "op"] {
        let path = bin.join(helper);
        fs::write(
            &path,
            format!("#!/bin/sh\nprintf launched > '{}'\n", marker.display()),
        )
        .unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut cmd = command(config, cwd, agent);
    cmd.env("PATH", &bin).arg("--copy").arg("SECRET");
    assert!(!output(cmd).await.status.success());
    assert!(!marker.exists());
}

#[tokio::test]
async fn failures_stop_all_consumers_before_launch_or_export() {
    let agent = FakeAgent::new().await;
    let root = std::env::temp_dir().canonicalize().unwrap();
    let dir = tempfile::tempdir_in(&root).unwrap();
    let caller = tempfile::tempdir_in(&root).unwrap();
    let fixture: KeyFile =
        serde_json::from_slice(include_bytes!("fixtures/local-storage-v1.json")).unwrap();
    assert_eq!(
        BASE64.decode(fixture.public_key.as_bytes()).unwrap(),
        agent.key.blob()
    );
    let path = store(dir.path(), &agent.key, fixture.records.clone());
    let cfg = config(dir.path(), "fixture");
    fail_stop(&cfg, caller.path(), None).await;
    let socket = agent.socket();
    let moved = socket.with_extension("moved");
    fs::rename(&socket, &moved).unwrap();
    fail_stop(&cfg, caller.path(), Some(&agent)).await;
    fs::rename(&moved, &socket).unwrap();
    fs::remove_file(&path).unwrap();
    let before = agent.signs();
    fail_stop(&cfg, caller.path(), Some(&agent)).await;
    assert_eq!(agent.signs(), before);
    store(dir.path(), &agent.key, fixture.records.clone());
    for reference in [
        "",
        "bad#selector",
        "fixture#SHA256:bad",
        "missing",
        "fixture#SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    ] {
        config(dir.path(), reference);
        let before = agent.signs();
        fail_stop(&cfg, caller.path(), Some(&agent)).await;
        assert_eq!(agent.signs(), before);
    }
    config(dir.path(), "fixture");
    for fault in [
        Fault::Refuse,
        Fault::Disconnect,
        Fault::Malformed,
        Fault::Oversized,
        Fault::InvalidSignature,
        Fault::WrongAlgorithm,
        Fault::Truncated,
        Fault::Trailing,
        Fault::ShortSignature,
        Fault::EmptyFrame,
        Fault::InvalidIdentity,
        Fault::DuplicateIdentity,
        Fault::ExcessIdentities,
    ] {
        agent.fault(fault);
        fail_stop(&cfg, caller.path(), Some(&agent)).await;
    }
    // Override listing with a non-fault sentinel so failures reach signing.
    agent
        .state
        .listing_fault
        .store(usize::MAX, Ordering::SeqCst);
    for fault in [
        Fault::Refuse,
        Fault::Disconnect,
        Fault::Oversized,
        Fault::Trailing,
    ] {
        agent.fault(fault);
        let before = agent.signs();
        fail_stop(&cfg, caller.path(), Some(&agent)).await;
        assert_eq!(agent.signs(), before + 4);
    }
    agent.state.listing_fault.store(0, Ordering::SeqCst);
    agent.fault(Fault::None);
    agent.delay();
    let before = agent.signs();
    fail_stop(&cfg, caller.path(), Some(&agent)).await;
    assert_eq!(agent.signs(), before + 4);
    agent.release();
    let storage = dir.path().join(".psst");
    let renamed = dir.path().join("storage");
    fs::rename(&storage, &renamed).unwrap();
    symlink(&renamed, &storage).unwrap();
    let before = agent.signs();
    fail_stop(&cfg, caller.path(), Some(&agent)).await;
    assert_eq!(agent.signs(), before);
    fs::remove_file(&storage).unwrap();
    fs::rename(&renamed, &storage).unwrap();
    let mut bad_key = fixture.clone();
    bad_key.fingerprint = key(8).fingerprint();
    let mut bad_version = fixture.clone();
    bad_version.version = 99;
    let mut duplicate = fixture.clone();
    duplicate.records.push(duplicate.records[0].clone());
    for bytes in [
        serde_json::to_vec(&bad_key).unwrap(),
        serde_json::to_vec(&bad_version).unwrap(),
        serde_json::to_vec(&duplicate).unwrap(),
        b"{}".to_vec(),
        vec![b' '; FILE_LIMIT + 1],
        b"[]".to_vec(),
    ] {
        fs::write(&path, bytes).unwrap();
        let before = agent.signs();
        fail_stop(&cfg, caller.path(), Some(&agent)).await;
        assert_eq!(agent.signs(), before);
    }
    store(dir.path(), &agent.key, fixture.records.clone());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let before = agent.signs();
    fail_stop(&cfg, caller.path(), Some(&agent)).await;
    assert_eq!(agent.signs(), before);
    fs::remove_file(&path).unwrap();
    let target = caller.path().join("ciphertext");
    fs::write(&target, serde_json::to_vec(&fixture).unwrap()).unwrap();
    symlink(&target, &path).unwrap();
    fail_stop(&cfg, caller.path(), Some(&agent)).await;
    assert_eq!(agent.signs(), before);
    fs::remove_file(&path).unwrap();
    let mut records = fixture.records;
    let mut bytes = BASE64.decode(records[0].ciphertext.as_bytes()).unwrap();
    bytes[0] ^= 1;
    records[0].ciphertext = BASE64.encode(&bytes);
    store(dir.path(), &agent.key, records);
    fail_stop(&cfg, caller.path(), Some(&agent)).await;
}

async fn fail_stop_copy_only(cfg: &Path, cwd: &Path, agent: &FakeAgent) {
    let bin = cwd.join("bin");
    fs::create_dir_all(&bin).unwrap();
    for helper in ["wl-copy", "xclip", "xsel", "pbcopy", "op"] {
        let path = bin.join(helper);
        fs::write(
            &path,
            format!(
                "#!/bin/sh\nprintf launched > '{}'\n",
                cwd.join("launched").display()
            ),
        )
        .unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut cmd = command(cfg, cwd, Some(agent));
    cmd.env("PATH", &bin).args(["--copy", "SECRET"]);
    assert!(!output(cmd).await.status.success());
    assert!(!cwd.join("launched").exists());
}

#[tokio::test]
async fn valid_legacy_storage_is_ignored_and_never_migrated_or_modified() {
    let agent = FakeAgent::new().await;
    let root = std::env::temp_dir().canonicalize().unwrap();
    let dir = tempfile::tempdir_in(&root).unwrap();
    let caller = tempfile::tempdir_in(&root).unwrap();
    let fixture: KeyFile =
        serde_json::from_slice(include_bytes!("fixtures/local-storage-v1.json")).unwrap();
    let direct = store(dir.path(), &agent.key, fixture.records);
    let legacy = dir.path().join(".psst/local-storage");
    fs::create_dir(&legacy).unwrap();
    fs::set_permissions(&legacy, fs::Permissions::from_mode(0o700)).unwrap();
    let old = legacy.join(agent.key.filename());
    fs::rename(&direct, &old).unwrap();
    let before = fs::read(&old).unwrap();
    let level = providers::get("local-storage").unwrap().browser().unwrap();
    assert!(
        psst::browser::list(&level, dir.path())
            .await
            .unwrap()
            .is_empty()
    );
    let cfg = config(dir.path(), "fixture");
    fail_stop(&cfg, caller.path(), Some(&agent)).await;
    assert_eq!(agent.signs(), 0);
    assert!(!direct.exists());
    let mut stored =
        psst::providers::local_storage::StoredKey::load(dir.path(), agent.key.clone()).unwrap();
    assert!(stored.records().is_empty());
    stored
        .set(&agent.client, "direct-only", "new dummy value")
        .await
        .unwrap();
    let choices = psst::browser::list(&level, dir.path()).await.unwrap();
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].label, "direct-only");
    assert_eq!(fs::read(&old).unwrap(), before);
    assert_eq!(fs::read_dir(&legacy).unwrap().count(), 1);
    assert!(direct.exists());
    let signs = agent.signs();
    fail_stop(&cfg, caller.path(), Some(&agent)).await;
    assert_eq!(agent.signs(), signs);
}

#[tokio::test]
async fn identity_selection_exact_text_and_operation_local_deduplication() {
    let agent = FakeAgent::with_seeds(&[7, 8]).await;
    let root = std::env::temp_dir().canonicalize().unwrap();
    let dir = tempfile::tempdir_in(&root).unwrap();
    let caller = tempfile::tempdir_in(&root).unwrap();
    let other = key(8);
    let first = Envelope::encrypt(&agent.client, &agent.key, "shared", "line\n雪 $\" ")
        .await
        .unwrap();
    let empty = Envelope::encrypt(&agent.client, &agent.key, "empty", "")
        .await
        .unwrap();
    store(dir.path(), &agent.key, vec![first.clone(), empty]);
    let unrelated = Envelope::encrypt(&agent.client, &other, "unrelated", "other")
        .await
        .unwrap();
    store(dir.path(), &other, vec![unrelated]);
    let cfg = dir.path().join("psst.json");
    fs::write(&cfg, format!(r#"{{"A":{{"source":"local-storage","value":"shared"}},"B":{{"source":"local-storage","value":"shared#{}"}},"EMPTY":{{"source":"local-storage","value":"empty"}}}}"#, agent.key.fingerprint())).unwrap();
    for _ in 0..2 {
        let before = agent.signs();
        let mut cmd = command(&cfg, caller.path(), Some(&agent));
        cmd.args([
            "--",
            "sh",
            "-c",
            "printf '%s|%s|%s' \"$A\" \"$B\" \"$EMPTY\"",
        ]);
        let result = output(cmd).await;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            String::from_utf8(result.stdout).unwrap(),
            "line\n雪 $\" |line\n雪 $\" |"
        );
        assert_eq!(agent.signs() - before, 2);
    }
    let mut cmd = command(&cfg, caller.path(), Some(&agent));
    cmd.arg("--write");
    assert!(output(cmd).await.status.success());
    assert_eq!(
        fs::read_to_string(caller.path().join(".env")).unwrap(),
        "A=\"line\\n雪 \\$\\\" \"\nB=\"line\\n雪 \\$\\\" \"\n"
    );
    fs::remove_file(caller.path().join(".env")).unwrap();
    config(dir.path(), "empty");
    fail_stop_copy_only(&cfg, caller.path(), &agent).await;
    config(dir.path(), "shared");
    let bin = caller.path().join("bin");
    for helper in ["wl-copy", "xclip", "xsel", "pbcopy"] {
        let path = bin.join(helper);
        fs::write(
            &path,
            format!(
                "#!/bin/sh\ncat > '{}'\n",
                caller.path().join("clipboard").display()
            ),
        )
        .unwrap();
    }
    let mut cmd = command(&cfg, caller.path(), Some(&agent));
    cmd.env(
        "PATH",
        format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
    )
    .args(["--copy", "SECRET"]);
    assert!(output(cmd).await.status.success());
    assert_eq!(
        fs::read_to_string(caller.path().join("clipboard")).unwrap(),
        "line\n雪 $\" "
    );
    let second = Envelope::encrypt(&agent.client, &other, "shared", "per-user")
        .await
        .unwrap();
    store(dir.path(), &other, vec![second]);
    config(dir.path(), "shared");
    let before = agent.signs();
    fail_stop(&cfg, caller.path(), Some(&agent)).await;
    assert_eq!(agent.signs(), before);
    config(dir.path(), &format!("shared#{}", other.fingerprint()));
    let mut cmd = command(&cfg, caller.path(), Some(&agent));
    cmd.args(["--", "sh", "-c", "printf %s \"$SECRET\""]);
    assert_eq!(output(cmd).await.stdout, b"per-user");
    agent.state.identities.lock().unwrap().clear();
    let before = agent.signs();
    fail_stop(&cfg, caller.path(), Some(&agent)).await;
    assert_eq!(agent.signs(), before);
    assert!(agent.state.connections.load(Ordering::SeqCst) > 0);
}

#[tokio::test]
async fn deadlines_active_and_queued_cancellation_do_not_retry_and_next_operation_works() {
    let agent = FakeAgent::new().await;
    let root = std::env::temp_dir().canonicalize().unwrap();
    let dir = tempfile::tempdir_in(&root).unwrap();
    let caller = tempfile::tempdir_in(&root).unwrap();
    let fixture: KeyFile =
        serde_json::from_slice(include_bytes!("fixtures/local-storage-v1.json")).unwrap();
    store(dir.path(), &agent.key, fixture.records);
    let cfg = dir.path().join("psst.json");
    fs::write(&cfg, format!(r#"{{"A":{{"source":"local-storage","value":"fixture"}},"B":{{"source":"local-storage","value":"fixture#{}"}}}}"#, agent.key.fingerprint())).unwrap();
    agent.fault(Fault::Refuse);
    let connections = agent.state.connections.load(Ordering::SeqCst);
    let mut cmd = command(&cfg, caller.path(), Some(&agent));
    cmd.arg("--write");
    assert!(!output(cmd).await.status.success());
    assert_eq!(
        agent.state.connections.load(Ordering::SeqCst),
        connections + 1
    );
    agent.fault(Fault::None);
    agent.delay();
    let before = agent.signs();
    let mut cmd = command(&cfg, caller.path(), Some(&agent));
    cmd.args(["--", "sh", "-c", "touch launched"]);
    let start = std::time::Instant::now();
    let result = output(cmd).await;
    assert!(!result.status.success());
    assert!(start.elapsed() < std::time::Duration::from_secs(3));
    assert_eq!(agent.signs(), before + 1);
    assert!(!caller.path().join("launched").exists());
    agent.release();
    let mut cmd = command(&cfg, caller.path(), Some(&agent));
    cmd.arg("--write");
    assert!(output(cmd).await.status.success());
    fs::remove_file(caller.path().join(".env")).unwrap();
    // Abort an operation with active work and a shared mapping still queued.
    while agent.state.requested.notified().now_or_never().is_some() {}
    agent.delay();
    let before = agent.signs();
    let mut cmd = command(&cfg, caller.path(), Some(&agent));
    cmd.arg("--write");
    let mut child = cmd.spawn().unwrap();
    agent.requested().await;
    assert_eq!(agent.signs(), before + 1);
    unsafe {
        libc::kill(child.id().unwrap() as i32, libc::SIGTERM);
    }
    let status = tokio::time::timeout(std::time::Duration::from_secs(3), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status.code(), Some(143));
    assert!(!caller.path().join(".env").exists());
    agent.release();
    let mut cmd = command(&cfg, caller.path(), Some(&agent));
    cmd.arg("--write");
    assert!(output(cmd).await.status.success());
    assert_eq!(agent.signs(), before + 2);
}

#[tokio::test]
async fn isolated_operation_cancellation_and_error_order() {
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.env_remove("PSST_SSH_AUTH_SOCK")
        .env_remove("SSH_AUTH_SOCK")
        .env("PSST_OPERATION_WORKER", "1")
        .args(["--exact", "operation_worker", "--test-threads=1"]);
    let result = output(cmd).await;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
}

#[tokio::test]
async fn operation_worker() {
    if std::env::var_os("PSST_OPERATION_WORKER").is_none() {
        return;
    }
    let agent = FakeAgent::new().await;
    let root = std::env::temp_dir().canonicalize().unwrap();
    let dir = tempfile::tempdir_in(&root).unwrap();
    let mut records = Vec::new();
    for id in ["one", "two"] {
        records.push(
            Envelope::encrypt(&agent.client, &agent.key, id, id)
                .await
                .unwrap(),
        );
    }
    store(dir.path(), &agent.key, records);
    // This worker runs alone in an isolated subprocess with a current-thread
    // runtime. No other test or thread reads its environment.
    unsafe {
        std::env::set_var("SSH_AUTH_SOCK", agent.socket());
    }
    let entries: Entries = [
        ("A", "local-storage", "one"),
        ("B", "local-storage", "two"),
        ("C", "local-storage", "missing"),
        ("D", "manual", "legacy"),
    ]
    .into_iter()
    .map(|(name, source, value)| {
        (
            name.into(),
            Entry {
                kind: psst::config::MappingKind::Env,
                source: source.into(),
                value: value.into(),
            },
        )
    })
    .collect();
    while agent.state.requested.notified().now_or_never().is_some() {}
    agent.delay();
    let before = agent.signs();
    {
        let work = providers::all(&entries, dir.path());
        tokio::pin!(work);
        tokio::select! {
            result = &mut work => panic!("unexpected result: {result:?}"),
            _ = agent.requested() => {}
        }
        assert_eq!(agent.signs(), before + 1);
        // Poll again: the second distinct record queues, never signs in parallel.
        tokio::select! {
            result = &mut work => panic!("unexpected result: {result:?}"),
            _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {}
        }
        assert_eq!(agent.signs(), before + 1);
    }
    agent.release();
    let results = providers::all(&entries, dir.path()).await;
    assert_eq!(
        results
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["A", "B", "C", "D"]
    );
    assert_eq!(results[0].1.as_ref().unwrap().as_str(), "one");
    assert_eq!(results[1].1.as_ref().unwrap().as_str(), "two");
    assert!(results[2].1.is_err());
    assert_eq!(results[3].1.as_ref().unwrap().as_str(), "legacy");
    assert_eq!(agent.signs(), before + 3);
}
