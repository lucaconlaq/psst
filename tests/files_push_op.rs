#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Output, Stdio},
    time::Duration,
};

use psst::providers::local_storage::StoredKey;
use serde_json::{Value, json};
use tokio::process::{Child, Command};

#[allow(dead_code)]
mod support;

const VAULT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaa";
const ITEM: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbb";
const OTHER: &str = "cccccccccccccccccccccccccc";

fn mapping(reference: &str) -> Value {
    json!({"type":"file", "source":"op", "value":reference})
}

fn item() -> Value {
    json!({
        "id": ITEM,
        "title": "Note",
        "version": 3,
        "vault": {"id": VAULT, "name": "Vault"},
        "category": "SECURE_NOTE",
        "last_edited_by": "fixture-user",
        "created_at": "2025-01-01T00:00:00Z",
        "updated_at": "2025-01-02T00:00:00Z",
        "favorite": true,
        "tags": ["one", "two"],
        "fields": [
            {"id":"notesPlain", "type":"STRING", "label":"notesPlain", "value":"OLD_PRIVATE", "purpose":"NOTES"},
            {"id":"secret", "type":"CONCEALED", "label":"Hidden", "value":"HIDDEN_PRIVATE", "reference":format!("op://{VAULT}/{ITEM}/secret")},
            {"id":"untouched", "type":"STRING", "label":"Untouched", "value":"UNRELATED_PRIVATE"}
        ]
    })
}

fn write_json(root: &Path, name: &str, value: &Value) {
    fs::write(root.join(name), serde_json::to_vec(value).unwrap()).unwrap();
}

fn fixture(root: &Path, mappings: Value) {
    fs::create_dir(root.join("tools")).unwrap();
    let executable = root.join("tools/op");
    fs::write(
        &executable,
        r#"#!/bin/sh
printf '%s\n' "$*" >> argv.log
case "$1 $2" in
  'vault list') cat vaults.json ;;
  'item list') cat items.json ;;
  'item get')
    count=0
    if [ -f get-count ]; then read -r count < get-count; fi
    count=$((count + 1))
    printf '%s\n' "$count" > get-count
    if [ -f "hook-$count" ]; then /bin/sh "hook-$count"; fi
    if [ "$3" = cccccccccccccccccccccccccc ]; then cat other.json; else cat item.json; fi ;;
  'item edit')
    cat > captured.json
    printf '%s\n' "$3" >> edits.log
    if [ -f slow-edit ]; then
      sleep 30 &
      printf '%s\n' "$!" > edit-child
      printf '%s\n' "$$" > edit-pid
      wait
    else
      printf '%s\n' "$$" > edit-pid
    fi
    if [ -f fail-edit ] || { [ -f fail-first-edit ] && [ "$3" = bbbbbbbbbbbbbbbbbbbbbbbbbb ]; }; then printf '%s\n' 'ERROR_PRIVATE' >&2; exit 1; fi
    cat captured.json ;;
  'read '*) cat fetch.txt ;;
  *) exit 2 ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(executable, fs::Permissions::from_mode(0o700)).unwrap();
    write_json(root, "vaults.json", &json!([{"id":VAULT,"name":"Vault"}]));
    write_json(root, "items.json", &json!([{"id":ITEM,"title":"Note"}]));
    write_json(root, "item.json", &item());
    write_json(root, "custom.json", &mappings);
    fs::write(root.join("one"), "NEW_PRIVATE").unwrap();
}

fn command(root: &Path) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("psst"));
    command
        .env_clear()
        .env(
            "PATH",
            format!(
                "{}:{}",
                root.join("tools").display(),
                std::env::var("PATH").unwrap()
            ),
        )
        .env("PSST_CONFIG", root.join("custom.json"))
        .env("PSST_TIMEOUT", "2")
        .env("TMPDIR", root)
        .current_dir(root)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

async fn finish(child: Child) -> Output {
    tokio::time::timeout(Duration::from_secs(30), child.wait_with_output())
        .await
        .unwrap()
        .unwrap()
}

async fn push(root: &Path) -> Output {
    finish(
        command(root)
            .args(["--files-push", "--yes"])
            .spawn()
            .unwrap(),
    )
    .await
}

fn report(output: &Output) -> String {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!text.contains("PRIVATE"), "{text}");
    text
}

fn captured(root: &Path) -> Value {
    serde_json::from_slice(&fs::read(root.join("captured.json")).unwrap()).unwrap()
}

fn clean(root: &Path) {
    let args = fs::read_to_string(root.join("argv.log")).unwrap_or_default();
    assert!(!args.contains("PRIVATE"));
    assert!(!args.contains("--template"));
    for entry in fs::read_dir(root).unwrap() {
        let name = entry.unwrap().file_name();
        let name = name.to_string_lossy();
        assert!(!name.starts_with("psst-files-") && !name.starts_with(".psst-write-"));
    }
}

#[tokio::test]
async fn exact_values_aliases_raw_unchanged_and_content_free_single_approval() {
    for text in ["", " \t\n", " \nNEW_PRIVATE 🔑\t\n", "OLD_PRIVATE"] {
        let root = support::tempdir();
        let root = root.path();
        fixture(
            root,
            json!({"one":mapping("op://Vault/Note/notesPlain"), "alias":mapping(&format!("op://{VAULT}/{ITEM}/notesPlain"))}),
        );
        fs::write(root.join("one"), text).unwrap();
        fs::write(root.join("alias"), text).unwrap();
        let original = fs::read(root.join("custom.json")).unwrap();
        let output = push(root).await;
        assert!(output.status.success(), "{}", report(&output));
        let report = report(&output);
        if text == "OLD_PRIVATE" {
            assert!(report.contains("one: unchanged") && report.contains("alias: unchanged"));
            assert!(!report.contains("Push local → provider:"));
            assert!(!root.join("captured.json").exists());
        } else {
            assert!(report.contains("one: saved") && report.contains("alias: saved"));
            assert_eq!(report.matches("Push local → provider:").count(), 1);
            assert!(report.contains("Shared target: one, alias"));
            assert!(report.contains("op edits may race with other writers."));
            let mut expected = item();
            expected["fields"][0]["value"] = json!(text);
            assert_eq!(captured(root), expected);
            // The complete exact JSON payload went over stdin, not assignments.
            assert_eq!(
                fs::read(root.join("captured.json")).unwrap(),
                serde_json::to_vec(&expected).unwrap()
            );
            assert_eq!(
                fs::read_to_string(root.join("edits.log"))
                    .unwrap()
                    .lines()
                    .count(),
                1
            );
        }
        assert_eq!(fs::read(root.join("custom.json")).unwrap(), original);
        assert_eq!(fs::read(root.join("one")).unwrap(), text.as_bytes());
        clean(root);
    }
}

#[tokio::test]
async fn omitted_empty_field_values_are_unchanged_or_updated_without_creation() {
    for field_id in ["notesPlain", "secret"] {
        let root = support::tempdir();
        let root = root.path();
        fixture(
            root,
            json!({"one":mapping(&format!("op://Vault/Note/{field_id}"))}),
        );
        let mut value = item();
        let field = value["fields"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|field| field["id"] == field_id)
            .unwrap();
        field.as_object_mut().unwrap().shift_remove("value");
        write_json(root, "item.json", &value);
        fs::write(root.join("one"), "").unwrap();
        let output = push(root).await;
        assert!(output.status.success(), "{}", report(&output));
        assert!(report(&output).contains("one: unchanged"));
        assert!(!root.join("captured.json").exists());
        fs::write(root.join("one"), "NEW_PRIVATE").unwrap();
        let output = push(root).await;
        assert!(output.status.success(), "{}", report(&output));
        let captured = captured(root);
        let fields = captured["fields"].as_array().unwrap();
        assert_eq!(fields.len(), 3);
        assert_eq!(
            fields.iter().find(|field| field["id"] == field_id).unwrap()["value"],
            "NEW_PRIVATE"
        );
        clean(root);
    }
}

#[tokio::test]
async fn optional_cli_summary_round_trips_without_disclosure() {
    for summary in [None, Some(""), Some(" \nSUMMARY_PRIVATE 🔑\t")] {
        let root = support::tempdir();
        let root = root.path();
        fixture(root, json!({"one":mapping("op://Vault/Note/notesPlain")}));
        let mut value = item();
        if let Some(summary) = summary {
            value["additional_information"] = json!(summary);
        }
        write_json(root, "item.json", &value);
        let output = push(root).await;
        assert!(output.status.success(), "{}", report(&output));
        value["fields"][0]["value"] = json!("NEW_PRIVATE");
        assert_eq!(captured(root), value);
        assert!(
            !fs::read_to_string(root.join("argv.log"))
                .unwrap()
                .contains("SUMMARY_PRIVATE")
        );
        clean(root);
    }
}

#[tokio::test]
async fn normalized_pull_is_not_raw_unchanged_detection_and_empty_pull_still_fails() {
    for raw in [" \nNEW_PRIVATE\t", ""] {
        let root = support::tempdir();
        let root = root.path();
        fixture(root, json!({"one":mapping("op://Vault/Note/notesPlain")}));
        // Pull requires an already-identical destination to be private,
        // independently of the runner's umask.
        fs::set_permissions(root.join("one"), fs::Permissions::from_mode(0o600)).unwrap();
        let mut value = item();
        value["fields"][0]["value"] = json!(raw);
        write_json(root, "item.json", &value);
        fs::write(root.join("fetch.txt"), raw).unwrap();
        let pulled = finish(
            command(root)
                .args(["--files-pull", "--yes"])
                .spawn()
                .unwrap(),
        )
        .await;
        if raw.is_empty() {
            assert!(!pulled.status.success());
            assert!(report(&pulled).contains("failed"));
        } else {
            assert!(pulled.status.success(), "{}", report(&pulled));
            assert_eq!(fs::read(root.join("one")).unwrap(), b"NEW_PRIVATE");
            let output = push(root).await;
            assert!(output.status.success(), "{}", report(&output));
            assert!(report(&output).contains("one: saved"));
            assert_eq!(captured(root)["fields"][0]["value"], "NEW_PRIVATE");
        }
        clean(root);
    }
}

#[tokio::test]
async fn conflicts_include_unchanged_alias_and_manual_work_continues() {
    let root = support::tempdir();
    let root = root.path();
    fixture(
        root,
        json!({
            "one":mapping("op://Vault/Note/notesPlain"),
            "alias":mapping(&format!("op://{VAULT}/{ITEM}/notesPlain")),
            "manual":{"type":"file","source":"manual","value":"old"}
        }),
    );
    fs::write(root.join("alias"), "OLD_PRIVATE").unwrap();
    fs::write(root.join("manual"), "MANUAL_PRIVATE").unwrap();
    let output = push(root).await;
    assert!(!output.status.success());
    let report = report(&output);
    assert!(
        report.contains("one: failed: conflicting")
            && report.contains("alias: failed: conflicting")
    );
    assert!(report.contains("manual: saved") && report.contains("partially completed"));
    assert!(!root.join("captured.json").exists());
    clean(root);
}

#[tokio::test]
async fn different_fields_combine_from_latest_item_preserving_all_other_data() {
    let root = support::tempdir();
    let root = root.path();
    fixture(
        root,
        json!({"one":mapping("op://Vault/Note/notesPlain"), "two":mapping("op://Vault/Note/Hidden")}),
    );
    fs::write(root.join("two"), " \nTWO_PRIVATE🔑\n").unwrap();
    let mut latest = item();
    latest["version"] = json!(8);
    latest["title"] = json!("Latest title");
    latest["favorite"] = json!(false);
    latest["tags"] = json!(["latest"]);
    latest["updated_at"] = json!("2026-01-01T00:00:00Z");
    latest["additional_information"] = json!("LATEST_SUMMARY_PRIVATE");
    latest["fields"][0]["value"] = json!("LATEST_PRIVATE");
    latest["fields"][2]["value"] = json!("LATEST_UNRELATED_PRIVATE");
    write_json(root, "latest.json", &latest);
    fs::write(root.join("hook-7"), "cp latest.json item.json\n").unwrap();
    let output = push(root).await;
    assert!(output.status.success(), "{}", report(&output));
    assert!(report(&output).contains("one: saved") && report(&output).contains("two: saved"));
    latest["fields"][0]["value"] = json!("NEW_PRIVATE");
    latest["fields"][1]["value"] = json!(" \nTWO_PRIVATE🔑\n");
    assert_eq!(captured(root), latest);
    assert_eq!(
        fs::read_to_string(root.join("edits.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    clean(root);
}

#[tokio::test]
async fn unsafe_unsupported_missing_and_ambiguous_items_fail_closed() {
    let mut exclusions = Vec::new();
    for category in ["LOGIN", "SSH_KEY", "PASSWORD", "DOCUMENT", "unknown"] {
        let mut value = item();
        value["category"] = json!(category);
        exclusions.push(value);
    }
    for key in ["passkeys", "files", "ssh_key", "future_property"] {
        let mut value = item();
        value[key] = json!([]);
        exclusions.push(value);
    }
    for summary in [Value::Null, json!(42), json!([]), json!({})] {
        let mut value = item();
        value["additional_information"] = summary;
        exclusions.push(value);
    }
    for field_type in ["OTP", "SSHKEY", "FILE", "DATE", "unknown"] {
        let mut value = item();
        value["fields"][1]["type"] = json!(field_type);
        exclusions.push(value);
    }
    let mut ambiguous = item();
    ambiguous["fields"][1]["label"] = json!("notesPlain");
    exclusions.push(ambiguous);
    let mut duplicate = item();
    duplicate["fields"][1]["id"] = json!("notesPlain");
    exclusions.push(duplicate);
    let mut missing = item();
    missing["fields"] = json!([]);
    exclusions.push(missing);
    let mut wrong_identity = item();
    wrong_identity["id"] = json!(OTHER);
    exclusions.push(wrong_identity);
    let mut null_value = item();
    null_value["fields"][1]["value"] = Value::Null;
    exclusions.push(null_value);
    let mut null_metadata = item();
    null_metadata["favorite"] = Value::Null;
    exclusions.push(null_metadata);
    let mut nul_value = item();
    nul_value["fields"][2]["value"] = json!("PRIVATE\0");
    exclusions.push(nul_value);
    let mut unsupported_purpose = item();
    unsupported_purpose["fields"][1]["purpose"] = json!("PASSWORD");
    exclusions.push(unsupported_purpose);
    for value in exclusions {
        let root = support::tempdir();
        let root = root.path();
        fixture(root, json!({"one":mapping("op://Vault/Note/notesPlain")}));
        write_json(root, "item.json", &value);
        let output = push(root).await;
        assert!(!output.status.success(), "{value}");
        assert!(report(&output).contains("one: failed"));
        assert!(!root.join("captured.json").exists());
        assert!(!report(&output).contains("Push local → provider:"));
        clean(root);
    }
    for list in ["vaults.json", "items.json"] {
        let root = support::tempdir();
        let root = root.path();
        fixture(root, json!({"one":mapping("op://Vault/Note/notesPlain")}));
        write_json(
            root,
            list,
            &json!([
                {"id":VAULT,"name":"Vault","title":"Note"},
                {"id":OTHER,"name":"Vault","title":"Note"}
            ]),
        );
        let output = push(root).await;
        assert!(!output.status.success());
        assert!(report(&output).contains("one: failed"));
        assert!(!root.join("captured.json").exists());
    }
}

#[tokio::test]
async fn latest_removals_retargets_unsafe_items_and_local_changes_fail_independently() {
    for mutation in [
        "removed",
        "retargeted",
        "unsafe",
        "local",
        "final-local",
        "field-removed",
    ] {
        let root = support::tempdir();
        let root = root.path();
        let mappings = json!({"one":mapping("op://Vault/Note/notesPlain"), "manual":{"type":"file","source":"manual","value":"old"}});
        fixture(root, mappings.clone());
        fs::write(root.join("manual"), "MANUAL_PRIVATE").unwrap();
        let mut changed = mappings;
        let mut latest = item();
        let hook = match mutation {
            "removed" => {
                changed.as_object_mut().unwrap().shift_remove("one");
                "cp changed.json custom.json\n"
            }
            "retargeted" => {
                changed["one"] = mapping("op://Vault/Note/Hidden");
                "cp changed.json custom.json\n"
            }
            "unsafe" => {
                latest["files"] = json!([]);
                "cp latest.json item.json\n"
            }
            "field-removed" => {
                latest["fields"].as_array_mut().unwrap().remove(0);
                "cp latest.json item.json\n"
            }
            _ => "printf '%s' CHANGED_PRIVATE > one\n",
        };
        write_json(root, "changed.json", &changed);
        write_json(root, "latest.json", &latest);
        fs::write(
            root.join(if mutation == "final-local" {
                "hook-4"
            } else {
                "hook-3"
            }),
            hook,
        )
        .unwrap();
        let output = push(root).await;
        assert!(!output.status.success(), "{mutation}: {}", report(&output));
        assert!(
            report(&output).contains("one: failed") && report(&output).contains("manual: saved")
        );
        assert!(!root.join("captured.json").exists(), "{mutation}");
        clean(root);
    }
}

#[tokio::test]
async fn changed_shared_input_rejects_whole_field_but_other_fields_still_save() {
    let root = support::tempdir();
    let root = root.path();
    fixture(
        root,
        json!({
            "one":mapping("op://Vault/Note/notesPlain"),
            "alias":mapping(&format!("op://{VAULT}/{ITEM}/notesPlain")),
            "two":mapping("op://Vault/Note/Hidden")
        }),
    );
    fs::write(root.join("alias"), "NEW_PRIVATE").unwrap();
    fs::write(root.join("two"), "TWO_PRIVATE").unwrap();
    // Three initial resolutions, two raw compares, then three resolutions and
    // the final item load. Change at the final boundary to exercise rebuilding.
    fs::write(root.join("hook-9"), "printf '%s' CHANGED_PRIVATE > alias\n").unwrap();
    let output = push(root).await;
    assert!(!output.status.success(), "{}", report(&output));
    let report = report(&output);
    assert!(
        report.contains("one: failed")
            && report.contains("alias: failed")
            && report.contains("two: saved"),
        "{report}"
    );
    assert_eq!(captured(root)["fields"][0]["value"], "OLD_PRIVATE");
    assert_eq!(captured(root)["fields"][1]["value"], "TWO_PRIVATE");
    clean(root);
}

#[tokio::test]
async fn provider_limits_include_oversized_aliases_unrelated_data_and_serialized_payloads() {
    for limit in [
        "field", "alias", "item", "summary", "payload", "output", "list",
    ] {
        let root = support::tempdir();
        let root = root.path();
        fixture(
            root,
            if limit == "alias" {
                json!({"one":mapping("op://Vault/Note/notesPlain"), "alias":mapping(&format!("op://{VAULT}/{ITEM}/notesPlain"))})
            } else {
                json!({"one":mapping("op://Vault/Note/notesPlain")})
            },
        );
        match limit {
            "field" => fs::write(root.join("one"), vec![b'x'; 65_537]).unwrap(),
            "alias" => fs::write(root.join("alias"), vec![b'x'; 65_537]).unwrap(),
            "item" | "output" => {
                let mut value = item();
                value["fields"][2]["value"] = json!("x".repeat(if limit == "item" {
                    1_048_576
                } else {
                    4_194_305
                }));
                write_json(root, "item.json", &value);
            }
            "summary" => {
                let mut value = item();
                value["additional_information"] = json!("x".repeat(1_048_576));
                write_json(root, "item.json", &value);
            }
            "payload" => {
                let mut value = item();
                value["fields"][2]["value"] = json!("x".repeat(1_048_000));
                write_json(root, "item.json", &value);
                fs::write(root.join("one"), vec![b'\n'; 65_536]).unwrap();
            }
            "list" => fs::write(root.join("vaults.json"), vec![b' '; 4_194_305]).unwrap(),
            _ => unreachable!(),
        }
        let output = push(root).await;
        assert!(!output.status.success(), "{limit}: {}", report(&output));
        assert!(report(&output).contains("one: failed"));
        if limit == "alias" {
            assert!(report(&output).contains("alias: failed"));
        }
        assert!(!root.join("captured.json").exists());
        clean(root);
    }
    // The exact field limit is accepted.
    let root = support::tempdir();
    fixture(
        root.path(),
        json!({"one":mapping("op://Vault/Note/notesPlain")}),
    );
    fs::write(root.path().join("one"), vec![b'x'; 65_536]).unwrap();
    assert!(push(root.path()).await.status.success());
}

#[tokio::test]
async fn unavailable_consent_unsupported_sources_and_reference_creation_never_edit() {
    let root = support::tempdir();
    let root = root.path();
    fixture(root, json!({"one":mapping("op://Vault/Note/notesPlain")}));
    let output = finish(command(root).arg("--files-push").spawn().unwrap()).await;
    assert!(!output.status.success());
    assert!(report(&output).contains("approval refused or unavailable"));
    assert!(!root.join("captured.json").exists());
    for source in ["op 2FA", "shell", "unknown", "vault", "terraform"] {
        write_json(
            root,
            "custom.json",
            &json!({"one":{"type":"file","source":source,"value":"touch EXECUTED"}}),
        );
        let output = push(root).await;
        assert!(!output.status.success());
        assert!(report(&output).contains("provider does not support push"));
        assert!(!root.join("EXECUTED").exists());
    }
    for reference in [
        "op://missing/Note/notesPlain",
        "op://Vault/missing/notesPlain",
        "op://Vault/Note/newField",
        "op://Vault/Note/notesPlain?attribute=otp",
        "op://Vault/Note/%6eotesPlain",
    ] {
        write_json(root, "custom.json", &json!({"one":mapping(reference)}));
        let output = push(root).await;
        assert!(!output.status.success());
        assert!(report(&output).contains("one: failed"));
        assert!(!root.join("captured.json").exists());
    }
    clean(root);
}

#[tokio::test]
async fn failed_and_timed_out_remote_edits_are_uncertain_and_manual_work_continues() {
    for marker in ["fail-edit", "slow-edit"] {
        let root = support::tempdir();
        let root = root.path();
        fixture(
            root,
            json!({"one":mapping("op://Vault/Note/notesPlain"), "manual":{"type":"file","source":"manual","value":"old"}}),
        );
        fs::write(root.join("manual"), "MANUAL_PRIVATE").unwrap();
        fs::write(root.join(marker), "").unwrap();
        let output = push(root).await;
        assert!(!output.status.success());
        let report = report(&output);
        assert!(
            report.contains("one: failed: remote edit failed or interrupted; outcome uncertain"),
            "{report}"
        );
        assert!(report.contains("manual: saved") && report.contains("partially completed"));
        assert!(!report.contains("one: saved") && !report.contains("rolled back"));
        assert!(root.join("captured.json").exists());
        if marker == "slow-edit" {
            for name in ["edit-pid", "edit-child"] {
                let pid = fs::read_to_string(root.join(name))
                    .unwrap()
                    .trim()
                    .parse()
                    .unwrap();
                assert_process_dead(pid).await;
            }
        }
        clean(root);
    }
}

#[tokio::test]
async fn independent_items_continue_after_uncertain_failure_and_report_every_file() {
    let root = support::tempdir();
    let root = root.path();
    fixture(
        root,
        json!({
            "one":mapping("op://Vault/Note/notesPlain"),
            "two":mapping(&format!("op://{VAULT}/{OTHER}/notesPlain"))
        }),
    );
    let mut other = item();
    other["id"] = json!(OTHER);
    other["title"] = json!("Other");
    write_json(root, "other.json", &other);
    write_json(
        root,
        "items.json",
        &json!([{"id":ITEM,"title":"Note"},{"id":OTHER,"title":"Other"}]),
    );
    fs::write(root.join("two"), "TWO_PRIVATE").unwrap();
    fs::write(root.join("fail-first-edit"), "").unwrap();
    let output = push(root).await;
    assert!(!output.status.success());
    let report = report(&output);
    assert!(report.contains("one: failed: remote edit failed or interrupted; outcome uncertain"));
    assert!(report.contains("two: saved") && report.contains("partially completed"));
    assert_eq!(
        fs::read_to_string(root.join("edits.log")).unwrap(),
        format!("{ITEM}\n{OTHER}\n")
    );
    assert_eq!(captured(root)["fields"][0]["value"], "TWO_PRIVATE");
    clean(root);
}

#[tokio::test]
async fn section_id_and_label_aliases_preserve_section_metadata_and_reject_ambiguity() {
    for ambiguous in [false, true] {
        let root = support::tempdir();
        let root = root.path();
        fixture(
            root,
            json!({
                "one":mapping("op://Vault/Note/Section/Hidden"),
                "alias":mapping(&format!("op://{VAULT}/{ITEM}/section1/secret"))
            }),
        );
        fs::write(root.join("alias"), "NEW_PRIVATE").unwrap();
        let mut value = item();
        value["sections"] = json!([{"id":"section1","label":"Section"}]);
        value["fields"][1]["section"] = json!({"id":"section1","label":"Section"});
        if ambiguous {
            value["sections"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"section2","label":"Section"}));
        }
        write_json(root, "item.json", &value);
        let output = push(root).await;
        if ambiguous {
            assert!(!output.status.success());
            assert!(report(&output).contains("one: failed"));
            // The explicit ID remains independently valid.
            assert!(report(&output).contains("alias: saved"));
        } else {
            assert!(output.status.success(), "{}", report(&output));
            assert!(
                report(&output).contains("one: saved") && report(&output).contains("alias: saved")
            );
        }
        value["fields"][1]["value"] = json!("NEW_PRIVATE");
        assert_eq!(captured(root), value);
        clean(root);
    }
}

#[tokio::test]
async fn aggregate_cap_is_counted_before_op_deduplication_or_provider_reads() {
    let root = support::tempdir();
    let root = root.path();
    fixture(root, json!({}));
    fs::write(root.join("one"), vec![b'x'; 4_194_304]).unwrap();
    let mut mappings = json!({});
    for index in 0..17 {
        let name = format!("input-{index}");
        fs::hard_link(root.join("one"), root.join(&name)).unwrap();
        mappings[name] = mapping("op://Vault/Note/notesPlain");
    }
    write_json(root, "custom.json", &mappings);
    let output = push(root).await;
    assert!(!output.status.success());
    assert_eq!(
        report(&output)
            .matches("batch exceeds 64 MiB; no source writes")
            .count(),
        17
    );
    assert!(!root.join("argv.log").exists());
    assert!(!root.join("captured.json").exists());
    clean(root);
}

#[tokio::test]
async fn raw_item_boundary_and_bounded_list_counts() {
    let root = support::tempdir();
    let root = root.path();
    fixture(root, json!({"one":mapping("op://Vault/Note/notesPlain")}));
    let mut bytes = serde_json::to_vec(&item()).unwrap();
    bytes.resize(1_048_576, b' ');
    fs::write(root.join("item.json"), bytes).unwrap();
    let output = push(root).await;
    assert!(output.status.success(), "{}", report(&output));
    fs::remove_file(root.join("captured.json")).unwrap();
    let rows: Vec<_> = (0..4097)
        .map(|index| json!({"id":format!("{index:026}"), "name":"Vault"}))
        .collect();
    write_json(root, "vaults.json", &json!(rows));
    let output = push(root).await;
    assert!(!output.status.success());
    assert!(report(&output).contains("one: failed"));
    assert!(!root.join("captured.json").exists());
    clean(root);
}

async fn assert_process_dead(pid: i32) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            // Match existing subprocess tests: a zombie is terminated, though
            // the host's reaper can retain its PID after the CLI has exited.
            let zombie = fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
                stat.split(')')
                    .nth(1)
                    .is_some_and(|state| state.trim_start().starts_with('Z'))
            });
            if unsafe { libc::kill(pid, 0) } != 0 || zombie {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("owned subprocess survived cancellation/timeout");
}

async fn wait_for(path: &Path) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn cancellation_during_edit_reports_uncertainty_and_kills_owned_process() {
    let root = support::tempdir();
    let root = root.path();
    fixture(
        root,
        json!({"one":mapping("op://Vault/Note/notesPlain"), "alias":mapping(&format!("op://{VAULT}/{ITEM}/notesPlain"))}),
    );
    fs::write(root.join("alias"), "NEW_PRIVATE").unwrap();
    fs::write(root.join("slow-edit"), "").unwrap();
    let child = command(root)
        .env("PSST_TIMEOUT", "20")
        .args(["--files-push", "--yes"])
        .spawn()
        .unwrap();
    wait_for(&root.join("edit-pid")).await;
    let pid: i32 = fs::read_to_string(root.join("edit-pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    // Only this test's child receives the signal.
    unsafe {
        libc::kill(child.id().unwrap() as i32, libc::SIGTERM);
    }
    let output = finish(child).await;
    assert!(!output.status.success());
    let report = report(&output);
    assert_eq!(
        report
            .matches("failed or interrupted remote edit; outcome uncertain")
            .count(),
        2,
        "{report}"
    );
    assert!(!report.contains("saved") && !report.contains("rolled back"));
    assert_process_dead(pid).await;
    let descendant: i32 = fs::read_to_string(root.join("edit-child"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_process_dead(descendant).await;
    clean(root);
}

#[tokio::test]
async fn cancellation_before_edit_is_not_uncertain_and_completed_items_remain() {
    for completed in [false, true] {
        let root = support::tempdir();
        let root = root.path();
        fixture(
            root,
            if completed {
                json!({"one":mapping("op://Vault/Note/notesPlain"), "two":mapping(&format!("op://{VAULT}/{OTHER}/notesPlain"))})
            } else {
                json!({"one":mapping("op://Vault/Note/notesPlain")})
            },
        );
        let mut other = item();
        other["id"] = json!(OTHER);
        other["title"] = json!("Other");
        write_json(root, "other.json", &other);
        if completed {
            write_json(
                root,
                "items.json",
                &json!([{"id":ITEM,"title":"Note"},{"id":OTHER,"title":"Other"}]),
            );
            fs::write(root.join("two"), "TWO_PRIVATE").unwrap();
        }
        // Pause while re-resolving the second item, or the first before writes.
        fs::write(
            root.join(if completed { "hook-7" } else { "hook-3" }),
            "printf '%s' ready > paused\nsleep 30\n",
        )
        .unwrap();
        let child = command(root)
            .env("PSST_TIMEOUT", "20")
            .args(["--files-push", "--yes"])
            .spawn()
            .unwrap();
        wait_for(&root.join("paused")).await;
        unsafe {
            libc::kill(child.id().unwrap() as i32, libc::SIGTERM);
        }
        let output = finish(child).await;
        assert!(!output.status.success());
        let report = report(&output);
        assert!(!report.contains("uncertain"), "{report}");
        if completed {
            assert!(
                report.contains("one: saved") && report.contains("two: failed: cancelled"),
                "{report}"
            );
            assert_eq!(
                fs::read_to_string(root.join("edits.log"))
                    .unwrap()
                    .lines()
                    .count(),
                1
            );
            assert_eq!(captured(root)["fields"][0]["value"], "NEW_PRIVATE");
        } else {
            assert!(report.contains("one: failed: cancelled"));
            assert!(!root.join("captured.json").exists());
        }
        clean(root);
    }
}

// Mixed-provider acceptance stays at the same isolated CLI/fake-op boundary.
async fn mixed_storage(root: &Path, fake: &support::agent::FakeAgent) -> Vec<u8> {
    StoredKey::load(root, fake.key.clone())
        .unwrap()
        .set(&fake.client, "id", "OLD_PRIVATE")
        .await
        .unwrap();
    fs::read(root.join(".psst").join(fake.key.filename())).unwrap()
}

async fn mixed_raw(root: &Path, fake: &support::agent::FakeAgent) -> String {
    StoredKey::load(root, fake.key.clone()).unwrap().records()[0]
        .decrypt(&fake.client, &fake.key, "id")
        .await
        .unwrap()
        .to_string()
}

fn local_mapping() -> Value {
    json!({"type":"file", "source":"local-storage", "value":"id"})
}

fn manual_mapping() -> Value {
    json!({"type":"file", "source":"manual", "value":"OLD_PRIVATE"})
}

fn mixed_inputs(root: &Path) {
    for name in ["encrypted", "one", "manual", "manual-two"] {
        fs::write(root.join(name), "NEW_PRIVATE").unwrap();
        fs::set_permissions(root.join(name), fs::Permissions::from_mode(0o644)).unwrap();
    }
}

fn assert_inputs(root: &Path, names: &[&str]) {
    for name in names {
        assert_eq!(fs::read_to_string(root.join(name)).unwrap(), "NEW_PRIVATE");
        assert_eq!(
            fs::metadata(root.join(name)).unwrap().permissions().mode() & 0o777,
            0o644
        );
    }
    clean(root);
    assert!(fs::read_dir(root.join(".psst")).unwrap().all(|entry| {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        name.ends_with(".json") || name.ends_with(".lock")
    }));
}

#[tokio::test]
async fn mixed_batch_reports_every_file_and_preserves_environment_and_independent_targets() {
    let fake = support::agent::FakeAgent::new().await;
    let root = support::tempdir();
    let root = root.path();
    fixture(
        root,
        json!({
            "ENV": {"source":"manual", "value":"ENV_PRIVATE"},
            "encrypted": local_mapping(),
            "encrypted-alias": {"type":"file", "source":"local-storage", "value":format!("id#{}", fake.key.fingerprint())},
            "one": mapping("op://Vault/Note/notesPlain"),
            "op-alias": mapping(&format!("op://{VAULT}/{ITEM}/notesPlain")),
            "hidden": mapping("op://Vault/Note/Hidden"),
            "manual": manual_mapping(),
            "manual-two": manual_mapping(),
            "unchanged": {"type":"file", "source":"manual", "value":"SAME_PRIVATE"},
            "missing": manual_mapping(),
            "unsupported": {"type":"file", "source":"shell", "value":"touch should-not-execute"},
            "conflict-a": mapping("op://Vault/Note/Untouched"),
            "conflict-b": mapping("op://Vault/Note/Untouched")
        }),
    );
    mixed_storage(root, &fake).await;
    mixed_inputs(root);
    for name in ["encrypted-alias", "op-alias", "hidden"] {
        fs::write(root.join(name), "NEW_PRIVATE").unwrap();
        fs::set_permissions(root.join(name), fs::Permissions::from_mode(0o644)).unwrap();
    }
    fs::write(root.join("unchanged"), "SAME_PRIVATE").unwrap();
    fs::write(root.join("unsupported"), "UNSUPPORTED_PRIVATE").unwrap();
    fs::write(root.join("conflict-a"), "A_PRIVATE").unwrap();
    fs::write(root.join("conflict-b"), "B_PRIVATE").unwrap();
    let output = finish(
        command(root)
            .env("SSH_AUTH_SOCK", fake.socket())
            .args(["--files-push", "--yes"])
            .spawn()
            .unwrap(),
    )
    .await;
    assert!(!output.status.success());
    let text = report(&output);
    assert_eq!(text.matches("Push local → provider:").count(), 1);
    assert_eq!(text.matches(": saved").count(), 7, "{text}");
    assert_eq!(text.matches(": unchanged").count(), 1, "{text}");
    assert_eq!(text.matches(": failed:").count(), 4, "{text}");
    for name in [
        "encrypted",
        "encrypted-alias",
        "one",
        "op-alias",
        "hidden",
        "manual",
        "manual-two",
    ] {
        assert!(text.contains(&format!("{name}: saved")), "{text}");
    }
    assert!(text.contains("partially completed"));
    assert!(!text.contains("/ENV:") && !root.join("should-not-execute").exists());
    assert_eq!(mixed_raw(root, &fake).await, "NEW_PRIVATE");
    let saved = captured(root);
    assert_eq!(saved["fields"][0]["value"], "NEW_PRIVATE");
    assert_eq!(saved["fields"][1]["value"], "NEW_PRIVATE");
    assert_eq!(saved["fields"][2]["value"], "UNRELATED_PRIVATE");
    assert_eq!(
        fs::read_to_string(root.join("edits.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    let latest: Value =
        serde_json::from_slice(&fs::read(root.join("custom.json")).unwrap()).unwrap();
    assert_eq!(latest["ENV"]["value"], "ENV_PRIVATE");
    assert_eq!(latest["manual"]["value"], "NEW_PRIVATE");
    assert_eq!(latest["manual-two"]["value"], "NEW_PRIVATE");
    assert_inputs(
        root,
        &[
            "encrypted",
            "encrypted-alias",
            "one",
            "op-alias",
            "hidden",
            "manual",
            "manual-two",
        ],
    );
}

#[tokio::test]
async fn mixed_unavailable_consent_and_no_change_runs_never_write() {
    for unchanged in [false, true] {
        let fake = support::agent::FakeAgent::new().await;
        let root = support::tempdir();
        let root = root.path();
        fixture(
            root,
            json!({
                "encrypted": local_mapping(), "one": mapping("op://Vault/Note/notesPlain"),
                "manual": manual_mapping(), "missing": manual_mapping(),
                "unsupported": {"type":"file", "source":"vault", "value":"ignored"}
            }),
        );
        let before_storage = mixed_storage(root, &fake).await;
        for name in ["encrypted", "one", "manual"] {
            fs::write(
                root.join(name),
                if unchanged {
                    "OLD_PRIVATE"
                } else {
                    "NEW_PRIVATE"
                },
            )
            .unwrap();
        }
        fs::write(root.join("unsupported"), "PRIVATE").unwrap();
        let before_config = fs::read(root.join("custom.json")).unwrap();
        let output = finish(
            command(root)
                .env("SSH_AUTH_SOCK", fake.socket())
                .stdin(Stdio::null())
                .arg("--files-push")
                .spawn()
                .unwrap(),
        )
        .await;
        assert!(!output.status.success());
        let text = report(&output);
        assert_eq!(
            text.matches(": unchanged").count(),
            if unchanged { 3 } else { 0 }
        );
        assert_eq!(
            text.matches(": failed:").count(),
            if unchanged { 2 } else { 5 }
        );
        assert_eq!(
            text.contains("Push local → provider:"),
            !unchanged,
            "{text}"
        );
        assert!(!root.join("captured.json").exists());
        assert_eq!(fs::read(root.join("custom.json")).unwrap(), before_config);
        assert_eq!(
            fs::read(root.join(".psst").join(fake.key.filename())).unwrap(),
            before_storage
        );
        clean(root);
    }
}

#[tokio::test]
async fn mixed_provider_failures_do_not_discard_later_independent_groups() {
    for (encrypted_failure, remote_failure, manual_failure) in [
        (true, false, false),
        (false, true, false),
        (true, true, false),
        (false, false, true),
        (true, false, true),
        (false, true, true),
    ] {
        let fake = support::agent::FakeAgent::new().await;
        let root = support::tempdir();
        let root = root.path();
        fixture(
            root,
            json!({"manual":manual_mapping(), "one":mapping("op://Vault/Note/notesPlain"),
            "encrypted":local_mapping(), "manual-two":manual_mapping()}),
        );
        let before_storage = mixed_storage(root, &fake).await;
        mixed_inputs(root);
        if encrypted_failure {
            // op planning follows encrypted raw comparison, before any write.
            fs::write(
                root.join("hook-1"),
                format!("chmod 644 .psst/{}\n", fake.key.filename()),
            )
            .unwrap();
        }
        if remote_failure {
            fs::write(root.join("fail-edit"), "").unwrap();
        }
        if manual_failure {
            // Fits the input bound, but JSON escaping exceeds config's bound.
            fs::write(root.join("manual"), vec![b'\n'; 3 * 1024 * 1024]).unwrap();
        }
        let before_config = fs::read(root.join("custom.json")).unwrap();
        let output = finish(
            command(root)
                .env("SSH_AUTH_SOCK", fake.socket())
                .args(["--files-push", "--yes"])
                .spawn()
                .unwrap(),
        )
        .await;
        assert!(!output.status.success());
        let text = report(&output);
        for (name, failure) in [
            ("encrypted", encrypted_failure),
            ("one", remote_failure),
            ("manual", manual_failure),
            ("manual-two", manual_failure),
        ] {
            assert!(
                text.contains(&format!(
                    "{name}: {}",
                    if failure { "failed:" } else { "saved" }
                )),
                "{text}"
            );
        }
        assert_eq!(
            text.matches(": saved").count() + text.matches(": failed:").count(),
            4
        );
        assert!(text.contains("partially completed"));
        assert!(!text.contains("rolled back"));
        if remote_failure {
            assert!(text.contains("outcome uncertain"));
        }
        let path = root.join(".psst").join(fake.key.filename());
        if encrypted_failure {
            assert_eq!(fs::read(&path).unwrap(), before_storage);
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        assert_eq!(
            mixed_raw(root, &fake).await,
            if encrypted_failure {
                "OLD_PRIVATE"
            } else {
                "NEW_PRIVATE"
            }
        );
        let latest = fs::read(root.join("custom.json")).unwrap();
        if manual_failure {
            assert_eq!(latest, before_config);
        } else {
            let latest: Value = serde_json::from_slice(&latest).unwrap();
            assert_eq!(latest["manual-two"]["value"], "NEW_PRIVATE");
        }
        assert_eq!(captured(root)["fields"][0]["value"], "NEW_PRIVATE");
        clean(root);
    }
}

#[tokio::test]
async fn mixed_changed_shared_inputs_fail_target_wide_while_later_providers_save() {
    for mutation in [
        "content",
        "identity",
        "ancestor",
        "symlink",
        "protected",
        "retarget",
        "removed",
    ] {
        let fake = support::agent::FakeAgent::new().await;
        let root = support::tempdir();
        let root = root.path();
        fixture(
            root,
            json!({
                "nested/encrypted":local_mapping(), "nested/alias":local_mapping(),
                "one":mapping("op://Vault/Note/notesPlain"), "manual":manual_mapping()
            }),
        );
        let before = mixed_storage(root, &fake).await;
        fs::create_dir(root.join("nested")).unwrap();
        for name in ["nested/encrypted", "nested/alias", "manual"] {
            fs::write(root.join(name), "NEW_PRIVATE").unwrap();
        }
        let hook = match mutation {
            "content" => "printf CHANGED_PRIVATE > nested/alias\n".to_owned(),
            "identity" => "cp nested/alias replacement; mv replacement nested/alias\n".to_owned(),
            "ancestor" => {
                "mv nested old-nested; mkdir nested; cp old-nested/* nested/\n".to_owned()
            }
            "symlink" => "mv nested/alias old-alias; ln -s ../old-alias nested/alias\n".to_owned(),
            "protected" => "rm nested/alias; ln custom.json nested/alias\n".to_owned(),
            _ => {
                let mut latest: Value =
                    serde_json::from_slice(&fs::read(root.join("custom.json")).unwrap()).unwrap();
                if mutation == "retarget" {
                    latest["nested/alias"]["value"] = json!("missing");
                } else {
                    latest.as_object_mut().unwrap().remove("nested/alias");
                }
                latest["ADDED"] = json!({"source":"manual", "value":"KEEP_PRIVATE"});
                write_json(root, "latest.json", &latest);
                "cp latest.json custom.json\n".to_owned()
            }
        };
        // This fake-op read occurs after encrypted grouping/raw comparison but
        // before encrypted persistence. No timing-sensitive sleeps are needed.
        fs::write(root.join("hook-1"), hook).unwrap();
        let output = finish(
            command(root)
                .env("SSH_AUTH_SOCK", fake.socket())
                .args(["--files-push", "--yes"])
                .spawn()
                .unwrap(),
        )
        .await;
        assert!(!output.status.success());
        let text = report(&output);
        assert!(
            text.contains("nested/encrypted: failed") && text.contains("nested/alias: failed"),
            "{mutation}: {text}"
        );
        assert!(
            text.contains("one: saved") && text.contains("manual: saved"),
            "{mutation}: {text}"
        );
        assert!(text.contains("partially completed"));
        assert_eq!(
            fs::read(root.join(".psst").join(fake.key.filename())).unwrap(),
            before
        );
        assert_eq!(captured(root)["fields"][0]["value"], "NEW_PRIVATE");
        let latest: Value =
            serde_json::from_slice(&fs::read(root.join("custom.json")).unwrap()).unwrap();
        assert_eq!(latest["manual"]["value"], "NEW_PRIVATE");
        if matches!(mutation, "retarget" | "removed") {
            assert_eq!(latest["ADDED"]["value"], "KEEP_PRIVATE");
        }
        if mutation == "content" {
            assert_eq!(
                fs::read_to_string(root.join("nested/alias")).unwrap(),
                "CHANGED_PRIVATE"
            );
        }
        clean(root);
    }
}

#[tokio::test]
async fn mixed_aggregate_cap_aborts_before_deduplication_or_any_provider_work() {
    let fake = support::agent::FakeAgent::new().await;
    let root = support::tempdir();
    let root = root.path();
    let mut mappings =
        json!({"encrypted":local_mapping(), "one":mapping("op://Vault/Note/notesPlain")});
    for index in 0..15 {
        mappings[format!("manual-{index}")] = manual_mapping();
    }
    for index in 1..4 {
        mappings[format!("encrypted-{index}")] = local_mapping();
    }
    fixture(root, mappings);
    let before_storage = mixed_storage(root, &fake).await;
    let signs = fake.signs();
    // 60 MiB manual + four identical 1 MiB encrypted aliases + op input.
    // Deduplicating encrypted targets first would wrongly permit this batch.
    fs::write(root.join("encrypted"), vec![b'y'; 1024 * 1024]).unwrap();
    for index in 1..4 {
        fs::hard_link(
            root.join("encrypted"),
            root.join(format!("encrypted-{index}")),
        )
        .unwrap();
    }
    fs::write(root.join("manual-0"), vec![b'x'; 4 * 1024 * 1024]).unwrap();
    for index in 1..15 {
        fs::hard_link(root.join("manual-0"), root.join(format!("manual-{index}"))).unwrap();
    }
    let before_config = fs::read(root.join("custom.json")).unwrap();
    let output = finish(
        command(root)
            .env("SSH_AUTH_SOCK", fake.socket())
            .args(["--files-push", "--yes"])
            .spawn()
            .unwrap(),
    )
    .await;
    assert!(!output.status.success());
    let text = report(&output);
    assert_eq!(text.matches("batch exceeds 64 MiB").count(), 20, "{text}");
    assert!(!text.contains("Push local → provider:"));
    assert_eq!(fake.signs(), signs);
    assert!(!root.join("argv.log").exists() && !root.join("captured.json").exists());
    assert_eq!(fs::read(root.join("custom.json")).unwrap(), before_config);
    assert_eq!(
        fs::read(root.join(".psst").join(fake.key.filename())).unwrap(),
        before_storage
    );
    assert_eq!(
        fs::metadata(root.join("manual-0")).unwrap().len(),
        4 * 1024 * 1024
    );
    clean(root);
}

#[tokio::test]
async fn mixed_cancellation_during_agent_work_closes_sockets_without_starting_other_writes() {
    use std::sync::atomic::Ordering;

    for saving in [false, true] {
        let fake = support::agent::FakeAgent::new().await;
        let root = support::tempdir();
        let root = root.path();
        fixture(
            root,
            json!({"encrypted":local_mapping(), "one":mapping("op://Vault/Note/notesPlain"),
            "manual":manual_mapping(), "manual-two":manual_mapping()}),
        );
        let before_storage = mixed_storage(root, &fake).await;
        let before_config = fs::read(root.join("custom.json")).unwrap();
        mixed_inputs(root);
        let pause_after = fake.signs() + usize::from(saving);
        fake.state.delay_after.store(pause_after, Ordering::SeqCst);
        fake.delay();
        let child = command(root)
            .env("SSH_AUTH_SOCK", fake.socket())
            .env("PSST_TIMEOUT", "20")
            .args(["--files-push", "--yes"])
            .spawn()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while fake.signs() <= pause_after {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        unsafe {
            libc::kill(child.id().unwrap() as i32, libc::SIGTERM);
        }
        let output = finish(child).await;
        assert!(!output.status.success());
        let text = report(&output);
        assert_eq!(text.matches(": failed:").count(), 4, "{text}");
        assert!(
            !text.contains(": saved")
                && !text.contains("uncertain")
                && !text.contains("rolled back")
        );
        assert_eq!(text.contains("Push local → provider:"), saving, "{text}");
        assert_eq!(
            fs::read(root.join(".psst").join(fake.key.filename())).unwrap(),
            before_storage
        );
        assert_eq!(fs::read(root.join("custom.json")).unwrap(), before_config);
        assert!(!root.join("captured.json").exists());
        tokio::time::timeout(Duration::from_secs(5), async {
            while fake.state.connections.load(Ordering::SeqCst)
                != fake.state.closed.load(Ordering::SeqCst)
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        fake.release();
        StoredKey::load(root, fake.key.clone())
            .unwrap()
            .set(&fake.client, "id", "RETRY_PRIVATE")
            .await
            .unwrap();
        assert_inputs(root, &["encrypted", "one", "manual", "manual-two"]);
    }
}

#[tokio::test]
async fn mixed_cancellation_retains_encrypted_writes_and_distinguishes_remote_uncertainty() {
    for phase in ["planning", "between-groups", "edit"] {
        let fake = support::agent::FakeAgent::new().await;
        let root = support::tempdir();
        let root = root.path();
        fixture(
            root,
            json!({"encrypted":local_mapping(), "one":mapping("op://Vault/Note/notesPlain"),
            "manual":manual_mapping(), "manual-two":manual_mapping()}),
        );
        let before_storage = mixed_storage(root, &fake).await;
        mixed_inputs(root);
        let before_config = fs::read(root.join("custom.json")).unwrap();
        if phase == "edit" {
            fs::write(root.join("slow-edit"), "").unwrap();
        } else {
            fs::write(
                root.join(if phase == "planning" {
                    "hook-1"
                } else {
                    "hook-3"
                }),
                "sleep 30 &\nprintf '%s\\n' $! > hook-child\nprintf '%s\\n' $$ > hook-pid\nprintf ready > paused\nwait\n",
            )
            .unwrap();
        }
        let child = command(root)
            .env("SSH_AUTH_SOCK", fake.socket())
            .env("PSST_TIMEOUT", "20")
            .args(["--files-push", "--yes"])
            .spawn()
            .unwrap();
        wait_for(&root.join(if phase == "edit" {
            "edit-pid"
        } else {
            "paused"
        }))
        .await;
        unsafe {
            libc::kill(child.id().unwrap() as i32, libc::SIGTERM);
        }
        let output = finish(child).await;
        assert!(!output.status.success());
        let text = report(&output);
        assert_eq!(
            text.matches(": saved").count(),
            usize::from(phase != "planning"),
            "{text}"
        );
        assert_eq!(
            text.matches(": failed:").count(),
            if phase == "planning" { 4 } else { 3 },
            "{text}"
        );
        assert_eq!(text.contains("uncertain"), phase == "edit", "{text}");
        assert_eq!(
            text.contains("partially completed"),
            phase != "planning",
            "{text}"
        );
        assert!(!text.contains("rolled back"));
        assert_eq!(fs::read(root.join("custom.json")).unwrap(), before_config);
        if phase == "planning" {
            assert_eq!(
                fs::read(root.join(".psst").join(fake.key.filename())).unwrap(),
                before_storage
            );
        } else {
            assert_eq!(mixed_raw(root, &fake).await, "NEW_PRIVATE");
        }
        let owned_pid = root.join(if phase == "edit" {
            "edit-pid"
        } else {
            "hook-pid"
        });
        let pid = fs::read_to_string(owned_pid)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_process_dead(pid).await;
        let descendant = root.join(if phase == "edit" {
            "edit-child"
        } else {
            "hook-child"
        });
        let pid = fs::read_to_string(descendant)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_process_dead(pid).await;
        // Reacquiring both lock families proves cancellation released ownership.
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("custom.json.lock"))
            .unwrap();
        fs2::FileExt::try_lock_exclusive(&lock).unwrap();
        fs2::FileExt::unlock(&lock).unwrap();
        StoredKey::load(root, fake.key.clone())
            .unwrap()
            .set(&fake.client, "id", "RETRY_PRIVATE")
            .await
            .unwrap();
        assert_inputs(root, &["encrypted", "one", "manual", "manual-two"]);
    }
}
