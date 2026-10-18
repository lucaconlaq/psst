#![cfg(unix)]

use super::{OUTPUT_LIMIT, TransferState, exact, output};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;
use zeroize::Zeroizing;

struct Fixture(tempfile::TempDir);

impl Fixture {
    fn new(script: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fake-provider");
        fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(dir)
    }

    fn program(&self) -> String {
        self.0.path().join("fake-provider").to_str().unwrap().into()
    }

    fn dir(&self) -> &Path {
        self.0.path()
    }

    async fn run(
        &self,
        payload: Option<Zeroizing<Vec<u8>>>,
        state: &mut TransferState,
    ) -> anyhow::Result<Zeroizing<String>> {
        tokio::time::timeout(
            Duration::from_secs(10),
            exact(
                &self.program(),
                &["literal-argument"],
                self.dir(),
                payload,
                state,
            ),
        )
        .await
        .expect("subprocess test exceeded deadline")
    }
}

#[tokio::test]
async fn exact_text_and_legacy_trim_are_distinct() {
    for value in ["", " \n雪🔑\t\n\n", "one\ntwo\n"] {
        let fake = Fixture::new(&format!("printf '%s' '{value}'"));
        let mut state = TransferState::NotStarted;
        assert_eq!(fake.run(None, &mut state).await.unwrap().as_str(), value);
        assert_eq!(state, TransferState::Succeeded);
        assert_eq!(
            output(&fake.program(), &[], fake.dir())
                .await
                .unwrap()
                .as_str(),
            value.trim()
        );
    }
}

#[tokio::test]
async fn stdin_is_exact_closes_at_eof_and_never_appears_in_argv_or_files() {
    let fake = Fixture::new("printf '%s\\n' \"$@\" > argv\ncat");
    for payload in ["", "PRIVATE-stdin-marker\n雪\n \t\n"] {
        let mut state = TransferState::NotStarted;
        let result = fake
            .run(
                Some(Zeroizing::new(payload.as_bytes().to_vec())),
                &mut state,
            )
            .await
            .unwrap();
        assert_eq!(result.as_str(), payload);
        assert_eq!(
            fs::read_to_string(fake.dir().join("argv")).unwrap(),
            "literal-argument\n"
        );
        let mut names: Vec<_> = fs::read_dir(fake.dir())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        names.sort();
        assert_eq!(names, ["argv", "fake-provider"]);
    }
}

#[tokio::test]
async fn simultaneous_pipe_pressure_does_not_deadlock() {
    let fake = Fixture::new("head -c 1048576 /dev/zero | tr '\\000' x\ncat");
    let payload = Zeroizing::new(vec![b'y'; 1024 * 1024]);
    let mut state = TransferState::NotStarted;
    let text = fake.run(Some(payload), &mut state).await.unwrap();
    assert_eq!(text.len(), 2 * 1024 * 1024);
    assert!(text.as_bytes()[..1024 * 1024].iter().all(|b| *b == b'x'));
    assert!(text.as_bytes()[1024 * 1024..].iter().all(|b| *b == b'y'));
}

#[tokio::test]
async fn input_and_output_limits_include_the_boundary() {
    let fake = Fixture::new("cat");
    let mut state = TransferState::NotStarted;
    let text = fake
        .run(
            Some(Zeroizing::new(vec![b'x'; OUTPUT_LIMIT as usize])),
            &mut state,
        )
        .await
        .unwrap();
    assert_eq!(text.len() as u64, OUTPUT_LIMIT);
    let error = fake
        .run(
            Some(Zeroizing::new(vec![b'x'; OUTPUT_LIMIT as usize + 1])),
            &mut state,
        )
        .await
        .unwrap_err();
    assert_eq!(state, TransferState::NotStarted);
    assert!(error.to_string().contains("input exceeded"));
    let fake = Fixture::new("head -c 4194305 /dev/zero | tr '\\000' x");
    let error = fake.run(None, &mut state).await.unwrap_err();
    assert_eq!(state, TransferState::Started);
    assert!(error.to_string().contains("output exceeded"));
}

#[tokio::test]
async fn failures_are_content_free_and_retain_start_evidence() {
    for script in [
        "printf '\\377'",
        "printf '\\000'",
        "printf 'PRIVATE-error-marker' >&2; exit 9",
        "exec 0<&-; sleep 0.05; exit 0",
    ] {
        let fake = Fixture::new(script);
        let mut state = TransferState::NotStarted;
        let error = fake
            .run(
                Some(Zeroizing::new(vec![b'x'; OUTPUT_LIMIT as usize])),
                &mut state,
            )
            .await
            .unwrap_err();
        assert_eq!(state, TransferState::Started);
        let diagnostic = format!("{error:#}");
        assert!(!diagnostic.contains("PRIVATE-error-marker"));
        assert!(!diagnostic.contains(&"x".repeat(100)));
    }
    // No stdin pressure masks validation of invalid captured output.
    for (script, message) in [("printf '\\377'", "non-UTF8"), ("printf '\\000'", "NUL")] {
        let fake = Fixture::new(script);
        let mut state = TransferState::NotStarted;
        assert!(
            fake.run(None, &mut state)
                .await
                .unwrap_err()
                .to_string()
                .contains(message)
        );
    }
    let fake = Fixture::new("printf 'PRIVATE-error-marker' >&2; exit 9");
    let mut state = TransferState::NotStarted;
    let error = fake.run(None, &mut state).await.unwrap_err();
    assert!(error.to_string().contains("Provider failed"));
    assert!(!format!("{error:#}").contains("PRIVATE-error-marker"));
    let error = exact(
        "/missing/PRIVATE-program-marker",
        &[],
        fake.dir(),
        Some(Zeroizing::new(b"PRIVATE-payload-marker".to_vec())),
        &mut state,
    )
    .await
    .unwrap_err();
    assert_eq!(state, TransferState::NotStarted);
    assert!(!format!("{error:#}").contains("PRIVATE"));
}

async fn wait_for_pids(dir: &Path) -> Vec<u32> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(text) = fs::read_to_string(dir.join("pids")) {
                let pids: Vec<u32> = text
                    .split_whitespace()
                    .filter_map(|p| p.parse().ok())
                    .collect();
                if pids.len() == 2 {
                    return pids;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("fixture did not publish parent and descendant PIDs")
}

async fn assert_stopped(pids: &[u32]) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let running = pids.iter().any(|pid| {
                let result = std::process::Command::new("ps")
                    .args(["-o", "stat=", "-p", &pid.to_string()])
                    .output()
                    .unwrap();
                let status = String::from_utf8_lossy(&result.stdout);
                !status.trim().is_empty() && !status.trim().starts_with('Z')
            });
            if !running {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("owned subprocess work survived cleanup");
}

// Both shell and descendant remain in the new subprocess group. No plaintext
// is staged: only public PIDs are written to synchronize cancellation.
const WAIT_SCRIPT: &str = "sleep 60 &\nprintf '%s %s\\n' \"$$\" \"$!\" > pids\nwait";

#[tokio::test]
async fn cancellation_drops_owned_work_and_kills_descendants() {
    let fake = Fixture::new(WAIT_SCRIPT);
    let mut state = TransferState::NotStarted;
    let program = fake.program();
    let work = exact(
        &program,
        &[],
        fake.dir(),
        Some(Zeroizing::new(vec![b's'; OUTPUT_LIMIT as usize])),
        &mut state,
    );
    let pids;
    {
        tokio::pin!(work);
        pids = tokio::select! {
            pids = wait_for_pids(fake.dir()) => pids,
            result = &mut work => panic!("fixture exited before cancellation: {result:?}"),
        };
        // Dropping this future owns and drops both pipe futures, the payload,
        // the fixed capture buffer, child, and group; there is no writer task.
    }
    assert_eq!(state, TransferState::Started);
    assert_stopped(&pids).await;
}

#[tokio::test]
async fn timeout_kills_descendants() {
    if std::env::var_os("PSST_PROCESS_TIMEOUT_CASE").is_none() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "process::tests::timeout_kills_descendants",
                "--nocapture",
            ])
            .env("PSST_PROCESS_TIMEOUT_CASE", "1")
            .env("PSST_TIMEOUT", "1")
            .env_remove("SSH_AUTH_SOCK")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
        return;
    }
    let fake = Fixture::new(WAIT_SCRIPT);
    let mut state = TransferState::NotStarted;
    let error = fake
        .run(
            Some(Zeroizing::new(vec![b's'; OUTPUT_LIMIT as usize])),
            &mut state,
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("timed out"));
    assert_eq!(state, TransferState::Started);
    assert_stopped(&wait_for_pids(fake.dir()).await).await;
}

#[tokio::test]
async fn success_and_failure_paths_clean_up_descendants() {
    for (action, payload, expected) in [
        ("printf ok", false, None),
        ("printf '\\377'", false, Some("non-UTF8")),
        ("printf '\\000'", false, Some("NUL")),
        ("exit 9", false, Some("Provider failed")),
        (
            "head -c 4194305 /dev/zero | tr '\\000' x",
            false,
            Some("output exceeded"),
        ),
        (
            "exec 0<&-; sleep 0.05; exit 0",
            true,
            Some("Cannot write provider stdin"),
        ),
    ] {
        let fake = Fixture::new(&format!(
            "sleep 60 </dev/null >/dev/null 2>&1 &\nprintf '%s %s\\n' \"$$\" \"$!\" > pids\n{action}"
        ));
        let mut state = TransferState::NotStarted;
        let payload = payload.then(|| Zeroizing::new(vec![b's'; OUTPUT_LIMIT as usize]));
        let result = fake.run(payload, &mut state).await;
        if let Some(message) = expected {
            assert!(
                result.unwrap_err().to_string().contains(message),
                "{action}"
            );
            assert_eq!(state, TransferState::Started);
        } else {
            assert_eq!(result.unwrap().as_str(), "ok");
            assert_eq!(state, TransferState::Succeeded);
        }
        assert_stopped(&wait_for_pids(fake.dir()).await).await;
    }
}

#[tokio::test]
async fn subprocess_has_no_controlling_terminal() {
    let fake = Fixture::new("if (exec 3<>/dev/tty) 2>/dev/null; then exit 9; fi\nprintf safe");
    let mut state = TransferState::NotStarted;
    assert_eq!(fake.run(None, &mut state).await.unwrap().as_str(), "safe");
}
