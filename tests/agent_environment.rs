#![cfg(unix)]

use std::ffi::OsStr;
use std::sync::atomic::Ordering;
use std::time::Duration;

use psst::providers::local_storage::Agent;
use tokio::process::Command;

#[allow(dead_code)]
mod support;

// Resolve environment settings in fresh processes, never mutate the test
// runner's environment or inherit a personal agent endpoint.
#[tokio::test]
async fn agent_environment_precedence_and_errors() {
    let standard = support::agent::FakeAgent::with_seed(7).await;
    let override_agent = support::agent::FakeAgent::with_seed(8).await;
    let root = support::tempdir();
    let missing = root.path().join("missing.sock");
    let regular = root.path().join("regular");
    std::fs::write(&regular, "not a socket").unwrap();
    let standard_socket = standard.socket();
    let override_socket = override_agent.socket();
    let standard_key = standard.key.fingerprint();
    let override_key = override_agent.key.fingerprint();
    for (override_path, standard_path, expected) in [
        (
            Some(override_socket.as_os_str()),
            Some(standard_socket.as_os_str()),
            override_key.as_str(),
        ),
        (
            Some(override_socket.as_os_str()),
            None,
            override_key.as_str(),
        ),
        (
            None,
            Some(standard_socket.as_os_str()),
            standard_key.as_str(),
        ),
        (
            Some(OsStr::new("")),
            Some(standard_socket.as_os_str()),
            "PSST_SSH_AUTH_SOCK is empty",
        ),
        (
            Some(missing.as_os_str()),
            Some(standard_socket.as_os_str()),
            "Cannot connect to SSH agent",
        ),
        (
            Some(regular.as_os_str()),
            Some(standard_socket.as_os_str()),
            "Cannot connect to SSH agent",
        ),
        (None, Some(OsStr::new("")), "SSH_AUTH_SOCK is empty"),
        (None, None, "SSH_AUTH_SOCK is not set"),
    ] {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .env_remove("PSST_SSH_AUTH_SOCK")
            .env_remove("SSH_AUTH_SOCK")
            .env("PSST_AGENT_EXPECTED", expected)
            .args(["--exact", "agent_environment_worker", "--nocapture"]);
        if let Some(path) = override_path {
            command.env("PSST_SSH_AUTH_SOCK", path);
        }
        if let Some(path) = standard_path {
            command.env("SSH_AUTH_SOCK", path);
        }
        let output = tokio::time::timeout(Duration::from_secs(10), command.output())
            .await
            .unwrap()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    // Invalid/empty overrides must not contact the otherwise valid fallback.
    assert_eq!(standard.state.connections.load(Ordering::SeqCst), 1);
    assert_eq!(override_agent.state.connections.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn agent_environment_worker() {
    let Ok(expected) = std::env::var("PSST_AGENT_EXPECTED") else {
        return;
    };
    let result = async {
        let keys = Agent::from_env()?.identities().await?;
        anyhow::ensure!(keys.len() == 1, "Expected one fixture key");
        Ok::<_, anyhow::Error>(keys[0].fingerprint())
    }
    .await;
    match result {
        Ok(fingerprint) => assert_eq!(fingerprint, expected),
        Err(error) => assert_eq!(error.to_string(), expected),
    }
}
