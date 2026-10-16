use psst::config::{Config, Entries, Entry, MappingKind};
use std::fs;

fn mapping(kind: MappingKind, value: &str) -> Entry {
    Entry {
        kind,
        source: "future-provider".into(),
        value: value.into(),
    }
}

#[test]
fn legacy_and_mixed_round_trips_preserve_order_kinds_and_literal_destinations() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("psst.json");
    fs::write(&path, r#"{"TOKEN":{"source":"manual","value":"legacy"},"./TOKEN":{"type":"file","source":"future-provider","value":"reference"},"~/$HOME/../file":{"type":"file","source":"shell","value":"exit 9"}}"#).unwrap();
    let mut config = Config::load(path.clone()).unwrap();
    assert!(config.entries["TOKEN"].kind == MappingKind::Env);
    assert!(config.entries["./TOKEN"].kind == MappingKind::File);
    let original = config.entries.clone();
    config.save(original.clone()).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["TOKEN"]["type"], "env");
    assert_eq!(saved["./TOKEN"]["type"], "file");
    let loaded = Config::load(path).unwrap();
    assert!(loaded.entries == original);
    assert_eq!(
        loaded
            .entries
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["TOKEN", "./TOKEN", "~/$HOME/../file"]
    );
    assert!(!dir.path().join("TOKEN").exists());
    assert!(!dir.path().join("~").exists());
}

#[test]
fn invalid_schema_names_destinations_and_cross_kind_duplicates_are_private() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("psst.json");
    for data in [
        r#"{"A":{"type":"DO_NOT_LOG_CONTENTS","source":"manual","value":"DO_NOT_LOG_CONTENTS"}}"#,
        r#"{"A":{"type":null,"source":"manual","value":"DO_NOT_LOG_CONTENTS"}}"#,
        r#"{"A":{"type":1,"source":"manual","value":"DO_NOT_LOG_CONTENTS"}}"#,
        r#"{"A":{"type":{},"source":"manual","value":"DO_NOT_LOG_CONTENTS"}}"#,
        r#"{"A":{"type":{"env":null},"source":"manual","value":"DO_NOT_LOG_CONTENTS"}}"#,
        r#"{"A":{"type":{"file":null},"source":"manual","value":"DO_NOT_LOG_CONTENTS"}}"#,
        r#"{"A":{"type":"file","type":"env","source":"manual","value":"DO_NOT_LOG_CONTENTS"}}"#,
        r#"{"A":{"type":"env","source":"manual","value":"DO_NOT_LOG_CONTENTS"},"A":{"type":"file","source":"manual","value":"DO_NOT_LOG_CONTENTS"}}"#,
        r#"{"A":{"source":1,"value":"DO_NOT_LOG_CONTENTS"}}"#,
        r#"{"A":{"source":"manual","value":{}}}"#,
        r#"{"A":{"source":"manual"}}"#,
        r#"{"./TOKEN":{"source":"manual","value":"DO_NOT_LOG_CONTENTS"}}"#,
        r#"{"1BAD":{"type":"env","source":"manual","value":"DO_NOT_LOG_CONTENTS"}}"#,
        r#"{"":{"type":"file","source":"manual","value":"DO_NOT_LOG_CONTENTS"}}"#,
        r#"{"bad\npath":{"type":"file","source":"manual","value":"DO_NOT_LOG_CONTENTS"}}"#,
        r#"{"bad\u007fpath":{"type":"file","source":"manual","value":"DO_NOT_LOG_CONTENTS"}}"#,
        r#"{"bad\u0085path":{"type":"file","source":"manual","value":"DO_NOT_LOG_CONTENTS"}}"#,
        r#"{"path":{"type":"file","source":"bad\u0000source","value":"DO_NOT_LOG_CONTENTS"}}"#,
        r#"{"path":{"type":"file","source":"manual","value":"DO_NOT_LOG_CONTENTS\u0000"}}"#,
    ] {
        fs::write(&path, data).unwrap();
        let error = Config::load(path.clone())
            .err()
            .expect("invalid config accepted");
        assert!(!format!("{error:#}").contains("DO_NOT_LOG_CONTENTS"));
    }
}

#[test]
fn save_validation_and_conflicts_leave_mixed_configuration_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("psst.json");
    let mut config = Config::load(path.clone()).unwrap();
    let entries = Entries::from([
        ("TOKEN".into(), mapping(MappingKind::Env, "entry")),
        ("./TOKEN".into(), mapping(MappingKind::File, "file")),
    ]);
    config.save(entries.clone()).unwrap();
    let bytes = fs::read(&path).unwrap();
    for (key, entry) in [
        ("", mapping(MappingKind::File, "file")),
        ("bad\tpath", mapping(MappingKind::File, "file")),
        ("bad-name", mapping(MappingKind::Env, "entry")),
        ("valid", mapping(MappingKind::File, "bad\0value")),
    ] {
        let mut invalid = entries.clone();
        invalid.insert(key.into(), entry);
        assert!(config.save(invalid).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(config.entries == entries);
    }
    fs::write(&path, b"{}").unwrap();
    assert!(config.save(entries.clone()).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"{}");
    assert!(config.entries == entries);
}

#[test]
fn mixed_config_size_limit_remains_enforced_on_load_and_save() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("psst.json");
    let mut config = Config::load(path.clone()).unwrap();
    let entries = Entries::from([(
        "path".into(),
        mapping(MappingKind::File, &"x".repeat(4 * 1024 * 1024)),
    )]);
    assert!(config.save(entries).is_err());
    assert!(!path.exists());
    fs::write(&path, vec![b' '; 4 * 1024 * 1024 + 1]).unwrap();
    assert!(Config::load(path).is_err());
}
