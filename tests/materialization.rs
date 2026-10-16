#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
    time::Duration,
};

use psst::{
    config::{Config, Entry, MappingKind},
    materialization::{Change, Plan, plan},
};
use support::agent::FakeAgent;

#[allow(dead_code)]
mod support;

// Do not run the CPU-heavy resource fixture alongside the one-second provider
// deadline fixture. Child processes remain isolated; timeout semantics stay
// put.
static PROVIDER_FIXTURES: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn mapping(source: &str, value: &str) -> Entry {
    Entry {
        kind: MappingKind::File,
        source: source.into(),
        value: value.into(),
    }
}

fn config(dir: &Path) -> Config {
    Config::load(dir.join("custom.json")).unwrap()
}

fn private(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

fn bytes(plan: &Plan, index: usize) -> Vec<u8> {
    let prepared = plan.files[index]
        .result
        .as_ref()
        .unwrap_or_else(|error| panic!("plan index {index}: {error}"));
    fs::read(prepared.staged_path()).unwrap()
}

#[tokio::test]
async fn preflight_rejects_entire_configuration_before_any_fetch() {
    let root = support::tempdir();
    let dir = root.path();
    let marker = dir.join("fetched");
    private(&dir.join("custom.json"), b"{}");
    private(&dir.join("psst.json"), b"{}");
    private(&dir.join("custom.json.lock"), b"");
    fs::create_dir(dir.join(".psst")).unwrap();
    private(&dir.join(".psst/record.json"), b"{}");
    fs::create_dir(dir.join("sub")).unwrap();
    symlink(dir.join(".psst"), dir.join("storage-link")).unwrap();
    symlink(dir.join("psst.json"), dir.join("config-link")).unwrap();
    symlink(dir.join("missing"), dir.join("dangling")).unwrap();
    fs::hard_link(dir.join("custom.json"), dir.join("hard-config")).unwrap();
    fs::hard_link(dir.join(".psst/record.json"), dir.join("hard-storage")).unwrap();
    let cases = [
        vec!["custom.json"],
        vec!["custom.json.lock"],
        vec!["psst.json"],
        vec![".psst.json"],
        vec!["psst.json.lock"],
        vec![".psst.json.lock"],
        vec![".psst/other"],
        vec!["storage-link/other"],
        vec!["config-link"],
        vec!["hard-config"],
        vec!["hard-storage"],
        vec!["same", "./same"],
        vec!["same", "sub/../same"],
        vec!["missing", "dangling"],
    ];
    for names in cases {
        let mut cfg = config(dir);
        cfg.entries
            .insert("valid".into(), mapping("shell", "printf fetched > fetched"));
        for name in names {
            cfg.entries
                .insert(name.into(), mapping("manual", "private"));
        }
        assert!(plan(&cfg).await.is_err());
        assert!(!marker.exists());
        assert!(!dir.join("valid").exists());
    }
    private(&dir.join("one"), b"data");
    fs::hard_link(dir.join("one"), dir.join("two")).unwrap();
    let mut cfg = config(dir);
    cfg.entries
        .insert("valid".into(), mapping("shell", "printf fetched > fetched"));
    cfg.entries.insert("one".into(), mapping("manual", "x"));
    cfg.entries.insert("two".into(), mapping("manual", "x"));
    assert!(plan(&cfg).await.is_err());
    assert!(!marker.exists());
}

#[tokio::test]
async fn literal_paths_private_staging_outcomes_snapshots_and_cleanup() {
    let root = support::tempdir();
    let dir = root.path().join("config");
    fs::create_dir(&dir).unwrap();
    let mut cfg = config(&dir);
    for name in ["new/child", "~/literal", "$HOME", "../outside"] {
        cfg.entries
            .insert(name.into(), mapping("manual", " \nexact\t"));
    }
    let absolute = root.path().join("absolute");
    cfg.entries.insert(
        absolute.to_str().unwrap().into(),
        mapping("manual", "absolute"),
    );
    private(&dir.join("same"), b"same");
    private(&dir.join("replace"), b"old");
    private(&dir.join("wrong-mode"), b"same");
    fs::set_permissions(dir.join("wrong-mode"), fs::Permissions::from_mode(0o644)).unwrap();
    fs::create_dir(dir.join("directory")).unwrap();
    symlink(dir.join("same"), dir.join("link")).unwrap();
    symlink(&dir, dir.join("linked-parent")).unwrap();
    cfg.entries.insert("same".into(), mapping("manual", "same"));
    cfg.entries
        .insert("replace".into(), mapping("manual", "new"));
    cfg.entries
        .insert("wrong-mode".into(), mapping("manual", "same"));
    cfg.entries
        .insert("directory".into(), mapping("manual", "new"));
    // Separate configurations avoid a configuration-wide alias with `same`.
    let mut unsafe_cfg = config(&dir);
    for name in ["link", "linked-parent/elsewhere", "linked-parent/../escape"] {
        unsafe_cfg
            .entries
            .insert(name.into(), mapping("manual", "new"));
    }
    let unsafe_plan = plan(&unsafe_cfg).await.unwrap();
    assert!(unsafe_plan.files.iter().all(|p| p.result.is_err()));
    let planned = plan(&cfg).await.unwrap();
    let stage = planned.staging_dir().to_owned();
    assert_eq!(
        fs::metadata(&stage).unwrap().permissions().mode() & 0o7777,
        0o700
    );
    assert!(!stage.starts_with(&dir));
    assert_eq!(
        planned.replacements().collect::<Vec<_>>(),
        vec![dir.join("replace").as_path()]
    );
    for index in 0..7 {
        let prepared = planned.files[index].result.as_ref().unwrap();
        assert_eq!(
            fs::metadata(prepared.staged_path())
                .unwrap()
                .permissions()
                .mode()
                & 0o7777,
            0o600
        );
        prepared
            .snapshot
            .check(&planned.files[index].destination)
            .unwrap();
        if index < 4 {
            assert_eq!(bytes(&planned, index), b" \nexact\t");
        }
    }
    assert_eq!(
        planned.files[5].result.as_ref().unwrap().change,
        Change::Unchanged
    );
    assert_eq!(
        planned.files[6].result.as_ref().unwrap().change,
        Change::Replace
    );
    assert!(planned.files[7].result.is_err());
    assert!(planned.files[8].result.is_err());
    assert!(!dir.join("new").exists());
    assert!(!dir.join("~").exists());
    assert!(!absolute.exists());
    assert_eq!(fs::read(dir.join("replace")).unwrap(), b"old");
    private(&dir.join("replace"), b"mutated");
    assert!(
        planned.files[6]
            .result
            .as_ref()
            .unwrap()
            .snapshot
            .check(&dir.join("replace"))
            .is_err()
    );
    fs::create_dir(dir.join("new")).unwrap();
    assert!(
        planned.files[0]
            .result
            .as_ref()
            .unwrap()
            .snapshot
            .check(&dir.join("new/child"))
            .is_err()
    );
    drop(planned);
    assert!(!stage.exists());
}

#[tokio::test]
async fn non_directory_ancestor_does_not_discard_independent_staging() {
    for good_first in [true, false] {
        let root = support::tempdir();
        let dir = root.path();
        private(&dir.join("parent"), b"not a directory");
        let mut cfg = config(dir);
        let names = if good_first {
            ["good", "parent/child"]
        } else {
            ["parent/child", "good"]
        };
        for name in names {
            let command = if name == "good" {
                "printf fetched > good-fetched; printf staged"
            } else {
                "printf fetched > bad-fetched; printf unsafe"
            };
            cfg.entries.insert(name.into(), mapping("shell", command));
        }

        let planned = plan(&cfg).await.unwrap();
        assert_eq!(planned.files.len(), 2);
        let good_index = usize::from(!good_first);
        let bad_index = usize::from(good_first);
        assert_eq!(planned.files[good_index].destination, dir.join("good"));
        assert_eq!(bytes(&planned, good_index), b"staged");
        assert_eq!(
            planned.files[good_index].result.as_ref().unwrap().change,
            Change::New
        );
        assert_eq!(
            planned.files[bad_index].destination,
            dir.join("parent/child")
        );
        assert!(planned.files[bad_index].result.is_err());
        assert_eq!(fs::read(dir.join("good-fetched")).unwrap(), b"fetched");
        assert!(!dir.join("bad-fetched").exists());
        assert!(!dir.join("good").exists());
        assert_eq!(fs::read(dir.join("parent")).unwrap(), b"not a directory");
        let staging = planned.staging_dir().to_owned();
        drop(planned);
        assert!(!staging.exists());
    }
}

#[tokio::test]
async fn nonregular_targets_metadata_bounds_and_empty_plan() {
    let root = support::tempdir();
    let dir = root.path();
    let empty = plan(&config(dir)).await.unwrap();
    assert!(empty.files.is_empty());
    drop(empty);
    let fifo = dir.join("fifo");
    let cpath = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: cpath is a live NUL-terminated path, and mode has no pointers.
    assert_eq!(unsafe { libc::mkfifo(cpath.as_ptr(), 0o600) }, 0);
    let mut cfg = config(dir);
    cfg.entries.insert("fifo".into(), mapping("manual", "x"));
    assert!(plan(&cfg).await.unwrap().files[0].result.is_err());
    cfg.entries.clear();
    for index in 0..4097 {
        cfg.entries.insert(
            format!("file-{index}"),
            mapping("shell", "touch fetched; printf PRIVATE"),
        );
    }
    assert!(plan(&cfg).await.is_err());
    assert!(!dir.join("fetched").exists());
}

// Provider environment is confined to one subprocess. No test mutates global
// environment or uses real credentials. The parent's fake agent stays alive.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn controlled_provider_fixtures() {
    let _fixture = PROVIDER_FIXTURES.lock().await;
    let fake = FakeAgent::new().await;
    let root = support::tempdir();
    let dir = root.path();
    let bin = dir.join("bin");
    fs::create_dir(&bin).unwrap();
    for program in ["sleep", "rm", "head", "tr", "chmod"] {
        let executable = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|dir| dir.join(program))
            .find(|path| path.is_file())
            .unwrap();
        symlink(executable, bin.join(program)).unwrap();
    }
    for program in ["op", "vault", "terraform"] {
        let path = bin.join(program);
        fs::write(&path, concat!(
            "#!/bin/sh\n",
            "[ ! -e \"$PSST_PLAN_FIXTURE/../active\" ] || exit 9\n",
            "printf '%s\\n' \"$PWD\" >> \"$PSST_PLAN_FIXTURE/../calls\"\n",
            "printf x > \"$PSST_PLAN_FIXTURE/../active\"\n",
            "sleep 0.02\n",
            "case \"${0##*/}\" in\n",
            "op) case \"$2\" in\n",
            "  *empty) printf '  \\n';;\n",
            "  *otp) printf 'otpauth://totp/test?secret=JBSWY3DPEHPK3PXP';;\n",
            "  *) printf '  normalized \\n';; esac;;\n",
            "vault) printf '%s' '{\"data\":{\"data\":{\"text\":\" \\nexact\\t\",\"empty\":\"\",\"nul\":\"\\u0000\"}}}';;\n",
            "terraform) printf '{\"text\":{\"value\":\"tf\"},\"empty\":{\"value\":\"\"}}';;\n",
            "esac\n",
            "rm \"$PSST_PLAN_FIXTURE/../active\"\n",
        )).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let work = dir.join("config");
    fs::create_dir(&work).unwrap();
    fs::create_dir(work.join("tf")).unwrap();
    fs::create_dir(work.join(".psst")).unwrap();
    fs::set_permissions(work.join(".psst"), fs::Permissions::from_mode(0o700)).unwrap();
    private(
        &work.join(".psst").join(fake.key.filename()),
        include_bytes!("fixtures/local-storage-v1.json"),
    );
    let output = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "provider_child", "--nocapture"])
        .env("PSST_PLAN_FIXTURE", &work)
        .env("PATH", &bin)
        .env("SSH_AUTH_SOCK", fake.socket())
        .env("PSST_TIMEOUT", "1")
        .env("TMPDIR", dir)
        .current_dir(dir)
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fake.signs() >= 1);
}

#[test]
fn many_near_limit_files_use_private_disk_staging_with_bounded_memory() {
    let _fixture = PROVIDER_FIXTURES.blocking_lock();
    let root = support::tempdir();
    let mut command = assert_cmd::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "large_batch_child", "--nocapture"])
        .env("PSST_LARGE_BATCH_FIXTURE", root.path())
        .env("TMPDIR", root.path())
        .env_remove("SSH_AUTH_SOCK")
        .timeout(Duration::from_secs(60))
        .assert()
        .success();
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    assert_eq!(fs::read_dir(root.path().join("config")).unwrap().count(), 0);
}

#[tokio::test]
async fn large_batch_child() {
    use std::io::Read;
    let Some(root) = std::env::var_os("PSST_LARGE_BATCH_FIXTURE") else {
        return;
    };
    let root = Path::new(&root);
    let dir = root.join("config");
    fs::create_dir(&dir).unwrap();
    let mut cfg = config(&dir);
    let size = 4 * 1024 * 1024 - 128;
    let value = format!("head -c {size} /dev/zero | tr '\\000' x");
    cfg.entries
        .insert("warmup".into(), mapping("shell", &value));
    drop(plan(&cfg).await.unwrap());
    cfg.entries.clear();
    #[cfg(target_os = "linux")]
    let peak_kib = || {
        fs::read_to_string("/proc/self/status")
            .unwrap()
            .lines()
            .find_map(|line| {
                line.strip_prefix("VmHWM:")
                    .map(|v| v.split_whitespace().next().unwrap().parse::<u64>().unwrap())
            })
            .unwrap()
    };
    #[cfg(target_os = "linux")]
    let before = peak_kib();
    for index in 0..32 {
        cfg.entries
            .insert(format!("file-{index}"), mapping("shell", &value));
    }
    let planned = plan(&cfg).await.unwrap();
    assert_eq!(planned.files.len(), 32);
    assert_eq!(fs::read_dir(planned.staging_dir()).unwrap().count(), 32);
    assert!(!planned.staging_dir().starts_with(&dir));
    assert_eq!(
        fs::metadata(planned.staging_dir())
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o700
    );
    for file in &planned.files {
        let staged = file.result.as_ref().unwrap().staged_path();
        assert_eq!(fs::metadata(staged).unwrap().len(), size);
        assert_eq!(
            fs::metadata(staged).unwrap().permissions().mode() & 0o7777,
            0o600
        );
        assert!(!file.destination.exists());
        let mut reader = fs::File::open(staged).unwrap();
        let mut buffer = [0; 16384];
        let mut count = 0;
        loop {
            let read = reader.read(&mut buffer).unwrap();
            if read == 0 {
                break;
            }
            assert!(buffer[..read].iter().all(|byte| *byte == b'x'));
            count += read;
        }
        assert_eq!(count as u64, size);
    }
    // An isolated process excludes other tests' allocations. Allow allocator
    // slack, but not retention of the batch's almost 128 MiB of plaintext.
    #[cfg(target_os = "linux")]
    assert!(
        peak_kib().saturating_sub(before) < 64 * 1024,
        "aggregate plaintext retained"
    );
    let staging = planned.staging_dir().to_owned();
    drop(planned);
    assert!(!staging.exists());
    assert_eq!(fs::read_dir(root).unwrap().count(), 1);
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);

    // Cancel a second large batch while its final provider is active. All
    // previously staged plaintext must disappear without destination writes.
    cfg.entries.insert(
        "wait".into(),
        mapping("shell", "printf ready > waiting; exec sleep 60"),
    );
    let mut pending = Box::pin(plan(&cfg));
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            tokio::select! {
                result = &mut pending => panic!("provider unexpectedly completed: {}", result.is_ok()),
                _ = tokio::time::sleep(Duration::from_millis(5)) => {
                    if dir.join("waiting").exists() { break; }
                }
            }
        }
    }).await.unwrap();
    let stage = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("psst-files-")
        })
        .unwrap();
    assert_eq!(fs::read_dir(&stage).unwrap().count(), 32);
    #[cfg(target_os = "linux")]
    assert!(
        peak_kib().saturating_sub(before) < 64 * 1024,
        "aggregate plaintext retained before cancellation"
    );
    drop(pending);
    assert!(!stage.exists());
    fs::remove_file(dir.join("waiting")).unwrap();
    assert_eq!(fs::read_dir(root).unwrap().count(), 1);
    assert_eq!(fs::read_dir(dir).unwrap().count(), 0);
}

#[tokio::test]
async fn provider_child() {
    let Some(work) = std::env::var_os("PSST_PLAN_FIXTURE") else {
        return;
    };
    let dir = Path::new(&work);
    let mut cfg = config(dir);
    let cases = [
        ("op", "op://value", Some(b"normalized".as_slice())),
        ("op 2FA", "op://otp", None),
        ("vault", "path#text", Some(b" \nexact\t".as_slice())),
        ("terraform", "tf#text", Some(b"tf".as_slice())),
        ("manual", " \nmanual\t", Some(b" \nmanual\t".as_slice())),
        ("shell", "printf '  shell \\n'", Some(b"shell".as_slice())),
        ("local-storage", "fixture", Some(" \n🔑\t".as_bytes())),
        ("shell", "printf ''", Some(b"".as_slice())),
        ("vault", "path#empty", Some(b"".as_slice())),
        ("terraform", "tf#empty", Some(b"".as_slice())),
    ];
    for (index, (source, value, _)) in cases.iter().enumerate() {
        cfg.entries
            .insert(format!("out-{index}"), mapping(source, value));
    }
    let failures = [
        ("manual", ""),
        ("op", "op://empty"),
        ("missing", "PRIVATE"),
        ("vault", "path#nul"),
        ("shell", "printf '\\377'"),
        ("shell", "printf '\\000'"),
        ("shell", "sleep 5"),
        ("local-storage", "absent"),
    ];
    for (index, (source, value)) in failures.iter().enumerate() {
        cfg.entries
            .insert(format!("bad-{index}"), mapping(source, value));
    }
    // A later success survives all failures, including local-storage failure.
    cfg.entries.insert("last".into(), mapping("manual", "last"));
    let planned = plan(&cfg).await.unwrap();
    for (index, (_, _, expected)) in cases.iter().enumerate() {
        let actual = bytes(&planned, index);
        if let Some(expected) = expected {
            assert_eq!(actual, *expected);
        } else {
            assert_eq!(actual.len(), 6);
            assert!(actual.iter().all(u8::is_ascii_digit));
        }
    }
    for file in &planned.files[cases.len()..cases.len() + failures.len()] {
        assert!(file.result.is_err());
        assert!(
            !file
                .result
                .as_ref()
                .err()
                .unwrap()
                .to_string()
                .contains("PRIVATE")
        );
    }
    assert_eq!(bytes(&planned, planned.files.len() - 1), b"last");
    assert!(!dir.join("out-0").exists());
    let calls = fs::read_to_string(dir.parent().unwrap().join("calls")).unwrap();
    let expected_dirs = [
        dir,
        dir,
        dir,
        &dir.join("tf"),
        dir,
        &dir.join("tf"),
        dir,
        dir,
    ];
    assert_eq!(
        calls.lines().collect::<Vec<_>>(),
        expected_dirs
            .iter()
            .map(|p| p.to_str().unwrap())
            .collect::<Vec<_>>()
    );
    drop(planned);

    // Invalid local storage is not a global destination-preflight failure.
    let broken = dir.join("broken-storage");
    fs::create_dir(&broken).unwrap();
    symlink(".psst", broken.join(".psst")).unwrap();
    let mut broken_cfg = config(&broken);
    broken_cfg
        .entries
        .insert("local".into(), mapping("local-storage", "fixture"));
    broken_cfg
        .entries
        .insert("valid".into(), mapping("manual", "independent"));
    let planned = plan(&broken_cfg).await.unwrap();
    assert!(planned.files[0].result.is_err());
    assert_eq!(bytes(&planned, 1), b"independent");
    drop(planned);

    // Controlled staging I/O failure is isolated; a later provider restores
    // access before its own staging. Root bypasses Unix permission checks.
    // SAFETY: geteuid has no arguments or pointers.
    if unsafe { libc::geteuid() } != 0 {
        let mut cfg = config(dir);
        cfg.entries
            .insert("before".into(), mapping("manual", "before"));
        cfg.entries.insert(
            "fault".into(),
            mapping("shell", "chmod 500 \"$TMPDIR\"/psst-files-*; printf failed"),
        );
        cfg.entries.insert(
            "after".into(),
            mapping("shell", "chmod 700 \"$TMPDIR\"/psst-files-*; printf after"),
        );
        let planned = plan(&cfg).await.unwrap();
        assert_eq!(bytes(&planned, 0), b"before");
        assert!(planned.files[1].result.is_err());
        assert_eq!(bytes(&planned, 2), b"after");
        let stage = planned.staging_dir().to_owned();
        drop(planned);
        assert!(!stage.exists());
    }

    // Local storage keeps its stricter 1 MiB bound and successful empty rule.
    let author = psst::providers::get("local-storage")
        .unwrap()
        .author()
        .unwrap();
    let identities = author.identities().await.unwrap();
    let too_large = author.prepare(dir, "oversized", &identities[0]).unwrap();
    assert!(
        too_large
            .save(zeroize::Zeroizing::new(
                "x".repeat(author.value_limit() + 1)
            ))
            .await
            .is_err()
    );
    for (id, value) in [
        ("empty", String::new()),
        ("limit", "x".repeat(author.value_limit())),
    ] {
        let reference = author
            .prepare(dir, id, &identities[0])
            .unwrap()
            .save(zeroize::Zeroizing::new(value.clone()))
            .await
            .unwrap();
        let mut cfg = config(dir);
        cfg.entries
            .insert("local".into(), mapping("local-storage", &reference));
        let planned = plan(&cfg).await.unwrap();
        assert_eq!(bytes(&planned, 0), value.as_bytes());
    }

    // Exact and over-limit provider results, without a larger public limit.
    for source in ["manual", "shell"] {
        let mut cfg = config(dir);
        let exact = if source == "manual" {
            "x".repeat(4 * 1024 * 1024)
        } else {
            "head -c 4194304 /dev/zero | tr '\\000' x".into()
        };
        let over = if source == "manual" {
            "x".repeat(4 * 1024 * 1024 + 1)
        } else {
            "head -c 4194305 /dev/zero | tr '\\000' x".into()
        };
        cfg.entries.insert("exact".into(), mapping(source, &exact));
        cfg.entries.insert("over".into(), mapping(source, &over));
        let planned = plan(&cfg).await.unwrap();
        assert_eq!(
            fs::metadata(planned.files[0].result.as_ref().unwrap().staged_path())
                .unwrap()
                .len(),
            4194304
        );
        assert!(planned.files[1].result.is_err());
    }

    // Many near-limit results remain on disk, not in planning metadata.
    let mut many = config(dir);
    for index in 0..8 {
        many.entries.insert(
            format!("large-{index}"),
            mapping("shell", "head -c 4194304 /dev/zero | tr '\\000' x"),
        );
    }
    let planned = plan(&many).await.unwrap();
    assert_eq!(
        planned
            .files
            .iter()
            .map(
                |file| fs::metadata(file.result.as_ref().unwrap().staged_path())
                    .unwrap()
                    .len()
            )
            .sum::<u64>(),
        8 * 4194304
    );
    let stage = planned.staging_dir().to_owned();
    drop(planned);
    assert!(!stage.exists());

    // Cancellation drops already staged results and the running provider.
    let mut cfg = config(dir);
    cfg.entries
        .insert("first".into(), mapping("manual", "plaintext"));
    cfg.entries.insert(
        "wait".into(),
        mapping("shell", "printf x > waiting; sleep 5"),
    );
    let temp = dir.parent().unwrap();
    let staging_count = || {
        fs::read_dir(temp)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("psst-files-")
            })
            .count()
    };
    assert_eq!(staging_count(), 0);
    let mut pending = Box::pin(plan(&cfg));
    tokio::select! {
        result = &mut pending => panic!("unexpected completion: {}", result.is_ok()),
        _ = tokio::time::sleep(Duration::from_millis(150)) => {}
    }
    assert!(dir.join("waiting").exists());
    assert_eq!(staging_count(), 1);
    drop(pending);
    assert_eq!(staging_count(), 0);
}
