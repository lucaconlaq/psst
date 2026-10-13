#![cfg(unix)]

use std::sync::atomic::Ordering;
use std::time::Duration;

use data_encoding::BASE64;
use psst::providers::local_storage::{
    Envelope, FILE_LIMIT, ID_LIMIT, KeyFile, PublicKey, VALUE_LIMIT, validate_id,
};
use serde_json::{Value, json};
use support::agent::{FakeAgent, Fault};

mod support;

#[tokio::test]
async fn stable_node_crypto_fixture_decrypts_through_fake_agent() {
    let fake = FakeAgent::new().await;
    let filename = "cff7d2bf46744594be2dc71b73a668396c2dfdf6e3d5514919204a41ba3889c1.json";
    assert_eq!(fake.key.filename(), filename);
    let file = KeyFile::parse(
        include_bytes!("fixtures/local-storage-v1.json"),
        &fake.key,
        filename,
    )
    .unwrap();
    assert_eq!(
        file.records[0]
            .decrypt(&fake.client, &fake.key, "fixture")
            .await
            .unwrap()
            .as_str(),
        " \n🔑\t"
    );
    assert_eq!(fake.signs(), 1);
}

#[tokio::test]
async fn exact_text_round_trips_and_fresh_writes() {
    let fake = FakeAgent::new().await;
    let identities = fake.client.identities().await.unwrap();
    assert_eq!(identities.len(), 1); // RSA was not offered as eligible.
    assert!(identities[0] == fake.key);
    let boundary = "x".repeat(VALUE_LIMIT);
    for text in [
        "",
        "\nline1\r\nline2\n",
        " \t\n ",
        "🔑é日本語",
        boundary.as_str(),
    ] {
        let envelope = Envelope::encrypt(&fake.client, &fake.key, "Exact ID", text)
            .await
            .unwrap();
        assert_eq!(
            envelope
                .decrypt(&fake.client, &fake.key, "Exact ID")
                .await
                .unwrap()
                .as_str(),
            text
        );
        let mut file = KeyFile::new(&fake.key);
        file.records.push(envelope);
        let bytes = file.encode(&fake.key).unwrap();
        let parsed = KeyFile::parse(&bytes, &fake.key, &fake.key.filename()).unwrap();
        assert_eq!(
            parsed.records[0]
                .decrypt(&fake.client, &fake.key, "Exact ID")
                .await
                .unwrap()
                .as_str(),
            text
        );
    }
    let a = Envelope::encrypt(&fake.client, &fake.key, "same", "value")
        .await
        .unwrap();
    let b = Envelope::encrypt(&fake.client, &fake.key, "same", "value")
        .await
        .unwrap();
    assert_ne!(a.salt, b.salt);
    assert_ne!(a.nonce, b.nonce);
    assert_ne!(a.ciphertext, b.ciphertext);
    assert_eq!(fake.signs(), 24);
}

#[tokio::test]
async fn malformed_identity_lists_are_rejected() {
    let fake = FakeAgent::new().await;
    for fault in [
        Fault::Refuse,
        Fault::Disconnect,
        Fault::Oversized,
        Fault::DuplicateIdentity,
        Fault::InvalidIdentity,
        Fault::ExcessIdentities,
        Fault::Trailing,
    ] {
        fake.fault(fault);
        assert!(fake.client.identities().await.is_err());
    }
    assert_eq!(fake.signs(), 0);
    fake.fault(Fault::None);
    assert_eq!(fake.client.identities().await.unwrap().len(), 1);
}

#[tokio::test]
async fn invalid_inputs_and_metadata_do_not_sign() {
    let fake = FakeAgent::new().await;
    let large = "x".repeat(VALUE_LIMIT + 1);
    for (id, value) in [
        ("", "ok"),
        ("has#selector", "ok"),
        ("control\n", "ok"),
        ("id", "a\0b"),
        ("id", large.as_str()),
    ] {
        assert!(
            Envelope::encrypt(&fake.client, &fake.key, id, value)
                .await
                .is_err()
        );
    }
    assert_eq!(fake.signs(), 0);
    assert!(validate_id(&"x".repeat(ID_LIMIT)).is_ok());
    assert!(validate_id(&"x".repeat(ID_LIMIT + 1)).is_err());
    for id in ["Case", "case", " space ", "../not-a-path", "é", "é\u{301}"] {
        assert!(validate_id(id).is_ok());
    }
    for id in ["\0", "\r", "\u{7f}", "\u{85}"] {
        assert!(validate_id(id).is_err());
    }

    let good = Envelope::encrypt(&fake.client, &fake.key, "id", "text")
        .await
        .unwrap();
    let mut other_blob = fake.key.blob();
    other_blob[19..].copy_from_slice(
        ed25519_dalek::SigningKey::from_bytes(&[8; 32])
            .verifying_key()
            .as_bytes(),
    );
    let other = PublicKey::from_blob(&other_blob).unwrap();
    let before = fake.signs();
    assert!(good.decrypt(&fake.client, &fake.key, "ID").await.is_err());
    assert!(good.decrypt(&fake.client, &other, "id").await.is_err());
    for (field, value) in [
        ("version", json!(2)),
        ("id", json!("")),
        ("public_key", json!(BASE64.encode(&other.blob()))),
        ("salt", json!(BASE64.encode(&[1; 31]))),
        ("nonce", json!(BASE64.encode(&[1; 13]))),
        ("ciphertext", json!(BASE64.encode(&[1; 15]))),
        ("salt", json!("not base64")),
        (
            "ciphertext",
            json!(BASE64.encode(&vec![0; VALUE_LIMIT + 17])),
        ),
    ] {
        let mut value_json = serde_json::to_value(&good).unwrap();
        value_json[field] = value;
        let invalid: Envelope = serde_json::from_value(value_json).unwrap();
        assert!(
            invalid
                .decrypt(&fake.client, &fake.key, &invalid.id)
                .await
                .is_err(),
            "accepted {field}"
        );
    }
    assert_eq!(fake.signs(), before);
}

#[tokio::test]
async fn tampering_and_agent_failures_are_errors_without_fallback() {
    let fake = FakeAgent::new().await;
    let good = Envelope::encrypt(&fake.client, &fake.key, "id", "text")
        .await
        .unwrap();
    for field in ["ciphertext", "salt", "nonce", "id"] {
        let mut value = serde_json::to_value(&good).unwrap();
        if field == "id" {
            value[field] = json!("changed");
        } else {
            let mut bytes = BASE64
                .decode(value[field].as_str().unwrap().as_bytes())
                .unwrap();
            bytes[0] ^= 1;
            value[field] = json!(BASE64.encode(&bytes));
        }
        let changed: Envelope = serde_json::from_value(value).unwrap();
        assert!(
            changed
                .decrypt(&fake.client, &fake.key, &changed.id)
                .await
                .is_err()
        );
    }
    let mut tag = good.clone();
    let mut bytes = BASE64.decode(tag.ciphertext.as_bytes()).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    tag.ciphertext = BASE64.encode(&bytes);
    assert!(tag.decrypt(&fake.client, &fake.key, "id").await.is_err());
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
    ] {
        fake.fault(fault);
        let before = fake.signs();
        assert!(good.decrypt(&fake.client, &fake.key, "id").await.is_err());
        assert_eq!(fake.signs(), before + 1);
    }
    fake.fault(Fault::Nonrepeatable);
    let before = fake.signs();
    let error = match Envelope::encrypt(&fake.client, &fake.key, "id", "text").await {
        Ok(_) => panic!("accepted nonrepeatable signing"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("not repeatable"));
    assert_eq!(fake.signs(), before + 2);
    fake.fault(Fault::None);
    assert_eq!(
        good.decrypt(&fake.client, &fake.key, "id")
            .await
            .unwrap()
            .as_str(),
        "text"
    );
    let absent = psst::providers::local_storage::Agent::at("/nonexistent/psst-test-agent.sock");
    assert!(good.decrypt(&absent, &fake.key, "id").await.is_err());
    assert!(PublicKey::from_blob(b"unsupported").is_err());
}

#[tokio::test]
async fn strict_key_files_bound_reads_and_writes() {
    let fake = FakeAgent::new().await;
    let mut file = KeyFile::new(&fake.key);
    file.records.push(
        Envelope::encrypt(&fake.client, &fake.key, "id", "text")
            .await
            .unwrap(),
    );
    let good = file.encode(&fake.key).unwrap();
    let parse = |bytes: &[u8]| KeyFile::parse(bytes, &fake.key, &fake.key.filename());
    assert!(parse(b"not JSON").is_err());
    assert!(parse(&vec![b' '; FILE_LIMIT + 1]).is_err());
    let mut at_limit = good.clone();
    at_limit.resize(FILE_LIMIT, b' ');
    assert!(parse(&at_limit).is_ok());
    assert!(KeyFile::parse(&good, &fake.key, "wrong.json").is_err());
    for (field, value) in [
        ("version", json!(2)),
        ("fingerprint", json!("wrong")),
        ("public_key", json!("wrong")),
        ("extra", json!(true)),
    ] {
        let mut bad = serde_json::to_value(&file).unwrap();
        bad[field] = value;
        assert!(parse(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    // Typed serde structs reject repeated fields rather than keeping the last.
    let text = String::from_utf8(good.clone()).unwrap();
    let repeated = text.replacen("\"version\":1", "\"version\":1,\"version\":1", 1);
    assert!(parse(repeated.as_bytes()).is_err());
    let mut record: Value = serde_json::to_value(&file.records[0]).unwrap();
    record["unknown"] = json!(true);
    let mut bad = serde_json::to_value(&file).unwrap();
    bad["records"][0] = record;
    assert!(parse(&serde_json::to_vec(&bad).unwrap()).is_err());
    let repeated_record = text.replacen("\"id\":\"id\"", "\"id\":\"id\",\"id\":\"id\"", 1);
    assert!(parse(repeated_record.as_bytes()).is_err());
    file.records.push(file.records[0].clone());
    assert!(file.encode(&fake.key).is_err());
    assert!(parse(&serde_json::to_vec(&file).unwrap()).is_err());
    file.records.pop();
    let large = Envelope::encrypt(&fake.client, &fake.key, "a", &"x".repeat(VALUE_LIMIT))
        .await
        .unwrap();
    file.records = (0..4)
        .map(|i| {
            let mut record = large.clone();
            record.id = i.to_string();
            record
        })
        .collect();
    assert!(file.encode(&fake.key).is_err());
    assert_eq!(fake.signs(), 4); // File validation itself never contacts the agent.
}

#[tokio::test]
async fn positional_json_arrays_are_rejected_before_signing() {
    let fake = FakeAgent::new().await;
    let object: Value =
        serde_json::from_slice(include_bytes!("fixtures/local-storage-v1.json")).unwrap();
    let record = &object["records"][0];
    let positional_record = json!([
        record["version"],
        record["id"],
        record["public_key"],
        record["salt"],
        record["nonce"],
        record["ciphertext"],
    ]);
    let positional_file = json!([
        object["version"],
        object["public_key"],
        object["fingerprint"],
        object["records"],
    ]);
    let mut nested = object.clone();
    nested["records"][0] = positional_record.clone();
    let positional_both = json!([
        object["version"],
        object["public_key"],
        object["fingerprint"],
        [positional_record.clone()],
    ]);

    // Exercise direct Envelope deserialization as well as the bounded file API.
    let result = async {
        let envelope: Envelope = serde_json::from_slice(&serde_json::to_vec(&positional_record)?)?;
        envelope.decrypt(&fake.client, &fake.key, "fixture").await
    }
    .await;
    assert!(result.is_err());
    assert_eq!(fake.signs(), 0);

    for invalid in [positional_file, nested, positional_both] {
        let bytes = serde_json::to_vec(&invalid).unwrap();
        assert!(serde_json::from_slice::<KeyFile>(&bytes).is_err());
        let result = async {
            let file = KeyFile::parse(&bytes, &fake.key, &fake.key.filename())?;
            file.records[0]
                .decrypt(&fake.client, &fake.key, "fixture")
                .await
        }
        .await;
        assert!(result.is_err());
        assert_eq!(fake.signs(), 0);
    }

    // The same named-field fixture remains usable after malformed input.
    let file = KeyFile::parse(
        &serde_json::to_vec(&object).unwrap(),
        &fake.key,
        &fake.key.filename(),
    )
    .unwrap();
    assert_eq!(
        file.records[0]
            .decrypt(&fake.client, &fake.key, "fixture")
            .await
            .unwrap()
            .as_str(),
        " \n🔑\t"
    );
    assert_eq!(fake.signs(), 1);
}

#[tokio::test]
async fn cancellation_releases_active_and_queued_work() {
    let fake = FakeAgent::new().await;
    let good = Envelope::encrypt(&fake.client, &fake.key, "id", "text")
        .await
        .unwrap();
    fake.delay();
    let client = fake.client.clone();
    let key = fake.key.clone();
    let record = good.clone();
    let active = tokio::spawn(async move { record.decrypt(&client, &key, "id").await });
    fake.requested().await; // Discard the earlier notification from creation if present.
    while fake.signs() != 3 {
        tokio::task::yield_now().await;
    }
    let client = fake.client.clone();
    let key = fake.key.clone();
    let record = good.clone();
    let queued = tokio::spawn(async move { record.decrypt(&client, &key, "id").await });
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(fake.signs(), 3);
    queued.abort();
    assert!(queued.await.unwrap_err().is_cancelled());
    active.abort();
    assert!(active.await.unwrap_err().is_cancelled());
    fake.release();
    assert_eq!(
        good.decrypt(&fake.client, &fake.key, "id")
            .await
            .unwrap()
            .as_str(),
        "text"
    );
    assert_eq!(fake.signs(), 4);
    tokio::time::timeout(Duration::from_secs(2), async {
        while fake.state.closed.load(Ordering::SeqCst) < 3 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

// Environment-driven deadlines are tested in a fresh process, not by racing
// process-global environment mutations in parallel tests.
#[test]
fn deadlines_include_queue_time() {
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .env_remove("SSH_AUTH_SOCK")
        .env("PSST_TIMEOUT", "1")
        .env("PSST_TEST_DEADLINE_WORKER", "1")
        .args(["--exact", "deadline_worker", "--nocapture"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn timeout_environment_semantics() {
    for (value, expected) in [
        (None, 30),
        (Some(""), 30),
        (Some("0"), 30),
        (Some("-1"), 30),
        (Some("invalid"), 30),
        (Some("2"), 2),
    ] {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .env_remove("SSH_AUTH_SOCK")
            .env_remove("PSST_TIMEOUT")
            .env("PSST_TEST_EXPECT_TIMEOUT", expected.to_string())
            .args(["--exact", "timeout_environment_worker"]);
        if let Some(value) = value {
            command.env("PSST_TIMEOUT", value);
        }
        assert!(command.output().unwrap().status.success());
    }
}

#[test]
fn timeout_environment_worker() {
    if let Ok(expected) = std::env::var("PSST_TEST_EXPECT_TIMEOUT") {
        assert_eq!(
            psst::process::timeout().as_secs(),
            expected.parse::<u64>().unwrap()
        );
    }
}

#[tokio::test]
async fn deadline_worker() {
    if std::env::var_os("PSST_TEST_DEADLINE_WORKER").is_none() {
        return;
    }
    let fake = FakeAgent::new().await;
    let good = Envelope::encrypt(&fake.client, &fake.key, "id", "text")
        .await
        .unwrap();
    fake.delay();
    let client = fake.client.clone();
    let key = fake.key.clone();
    let record = good.clone();
    let active = tokio::spawn(async move { record.decrypt(&client, &key, "id").await });
    while fake.signs() != 3 {
        tokio::task::yield_now().await;
    }
    let start = std::time::Instant::now();
    let queued = good.decrypt(&fake.client, &fake.key, "id");
    // The active phase times out first. Keep the next phase delayed as well.
    assert!(queued.await.unwrap_err().to_string().contains("timed out"));
    assert!(
        active
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("timed out")
    );
    assert!(start.elapsed() < Duration::from_millis(1600));
    assert!(fake.signs() <= 4); // No retry of either operation.
    fake.release();
    assert_eq!(
        good.decrypt(&fake.client, &fake.key, "id")
            .await
            .unwrap()
            .as_str(),
        "text"
    );
}
