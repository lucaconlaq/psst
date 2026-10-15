use super::app::{App, Browser, Message, Tasks, View};
use super::editor::{Creator, Editor};
use super::render::{ACCENT, BG, MASK, PANEL, block, centered, clean, draw, shortcut_lines};
use crate::{
    browser::{Choice, Target},
    config::{Config, Entry},
    providers,
};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::prelude::*;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use zeroize::Zeroizing;
fn app() -> App {
    let t = tempfile::tempdir().unwrap().keep().canonicalize().unwrap();
    App::new(Config::load(t.join(".psst.json")).unwrap())
}
fn ctrl(app: &mut App, character: char) {
    let (tx, _) = mpsc::unbounded_channel();
    app.key(
        KeyEvent::new(KeyCode::Char(character), KeyModifiers::CONTROL),
        &tx,
        &mut Tasks(vec![]),
    );
}
fn key(app: &mut App, code: KeyCode) {
    let (tx, _) = mpsc::unbounded_channel();
    app.key(
        KeyEvent::new(code, KeyModifiers::NONE),
        &tx,
        &mut Tasks(vec![]),
    );
}
struct TestDeletion(crate::providers::local_storage::StoredKey);
impl providers::PreparedDelete for TestDeletion {
    fn remove(
        &mut self,
        boundary: &mut dyn FnMut(crate::config::WriteBoundary) -> Result<()>,
    ) -> Result<()> {
        self.0.remove_checked("fixture", boundary)
    }
}

fn deletion_fixture() -> (App, std::path::PathBuf, Box<dyn providers::PreparedDelete>) {
    use crate::providers::local_storage::{KeyFile, PublicKey, StoredKey};
    use data_encoding::BASE64;
    use std::os::unix::fs::PermissionsExt;
    let mut app = app();
    let bytes = include_bytes!("../../tests/fixtures/local-storage-v1.json");
    let document: KeyFile = serde_json::from_slice(bytes).unwrap();
    let key =
        PublicKey::from_blob(&BASE64.decode(document.public_key.as_bytes()).unwrap()).unwrap();
    let dir = app.config.dir().join(".psst");
    std::fs::create_dir_all(&dir).unwrap();
    for path in [dir.parent().unwrap(), &dir] {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let path = dir.join(key.filename());
    std::fs::write(&path, bytes).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let id = document.records[0].id.clone();
    assert_eq!(id, "fixture");
    let entry = Entry {
        source: "local-storage".into(),
        value: id,
    };
    app.config
        .save(
            [("FIRST".into(), entry.clone()), ("SHARED".into(), entry)]
                .into_iter()
                .collect(),
        )
        .unwrap();
    let stored = StoredKey::load(app.config.dir(), key).unwrap();
    (app, path, Box::new(TestDeletion(stored)))
}

fn browser_deletion_fixture() -> (App, std::path::PathBuf) {
    let (mut app, path, _) = deletion_fixture();
    let document: providers::local_storage::KeyFile =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let reference = format!("fixture#{}", document.fingerprint);
    app.view = View::Browser {
        editor: Editor::new(
            None,
            "UNSAVED".into(),
            Entry {
                source: "local-storage".into(),
                value: "previous".into(),
            },
        ),
        browser: Browser {
            stack: vec![providers::get("local-storage").unwrap().browser().unwrap()],
            choices: vec![Choice {
                label: "fixture".into(),
                detail: document.fingerprint,
                target: Target::Reference(reference),
            }],
            query: "fix".into(),
            selected: 0,
            loading: false,
        },
    };
    if let View::Browser { editor, .. } = &mut app.view {
        editor.preview = Some(Zeroizing::new("CACHED_SECRET".into()));
        editor.preview_at = Some(Instant::now());
    }
    (app, path)
}

#[test]
fn browser_deletion_cancel_confirm_and_shared_warning_keep_configuration_and_draft() {
    let (mut app, path) = browser_deletion_fixture();
    let config = std::fs::read(&app.config.path).unwrap();
    let ciphertext = std::fs::read(&path).unwrap();
    if let View::Browser { browser, .. } = &mut app.view {
        browser.choices[0].label = "fix🔑".repeat(100);
    }
    for cancel in [KeyCode::Esc, KeyCode::Char('n')] {
        ctrl(&mut app, 'd');
        assert!(app.browser_delete.is_some());
        for (width, height) in [(45, 12), (120, 30)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            let text = screen_text(terminal.backend().buffer());
            assert!(text.contains("Mappings kept; references will fail."));
            assert!(text.contains("Possible references: 2"));
            assert!(text.contains("Loaded configuration only."));
            assert!(text.contains("Other configurations may also use it."));
            assert!(text.contains("Enter deletes"));
            assert!(text.contains("Esc cancels"));
        }
        key(&mut app, cancel);
        assert!(app.browser_delete.is_none());
        assert_eq!(std::fs::read(&path).unwrap(), ciphertext);
        assert_eq!(std::fs::read(&app.config.path).unwrap(), config);
    }
    ctrl(&mut app, 'd');
    key(&mut app, KeyCode::Enter);
    let document: providers::local_storage::KeyFile =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(document.records.is_empty());
    assert_eq!(std::fs::read(&app.config.path).unwrap(), config);
    assert!(app.status.contains("configuration unchanged"));
    assert!(matches!(&app.view, View::Browser { editor, browser }
        if editor.name == "UNSAVED" && editor.value.as_str() == "previous"
        && editor.preview.is_none() && editor.preview_at.is_none()
        && browser.choices.is_empty() && browser.query == "fix" && browser.selected == 0));
    ctrl(&mut app, 'd');
    assert!(app.browser_delete.is_none());
}

#[test]
fn control_chords_do_not_confirm_destructive_dialogs() {
    let (mut app, path) = browser_deletion_fixture();
    let before = std::fs::read(&path).unwrap();
    ctrl(&mut app, 'd');
    ctrl(&mut app, 'y');
    assert!(app.browser_delete.is_some());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    key(&mut app, KeyCode::Esc);
    app.view = View::Main;
    ctrl(&mut app, 'd');
    ctrl(&mut app, 'y');
    assert!(app.delete.is_some());
    assert_eq!(app.config.entries.len(), 2);
    key(&mut app, KeyCode::Esc);
    creator(&mut app);
    if let View::Create(c) = &mut app.view {
        c.pending = Some(Box::new(TestSave {
            path: app.config.dir().join("replacement"),
            failure: None,
            replacing: true,
        }));
    }
    ctrl(&mut app, 'y');
    assert!(matches!(&app.view, View::Create(c) if c.pending.is_some() && !c.busy));
    key(&mut app, KeyCode::Esc);
    assert!(matches!(&app.view, View::Create(c) if c.pending.is_none()));
}

#[test]
fn dismissing_failure_details_never_confirms_hidden_browser_deletion() {
    let (mut app, path) = browser_deletion_fixture();
    let before = std::fs::read(&path).unwrap();
    let old_generation = app.generation;
    app.loading.insert("pending reveal".into());
    ctrl(&mut app, 'd');
    assert!(app.loading.is_empty());
    app.message(Message::Reveal {
        generation: old_generation,
        name: "pending reveal".into(),
        result: Err(anyhow::anyhow!("late failure")),
    });
    assert!(!app.error_panel);
    // Even an unrelated current-generation failure cannot turn dismissal into
    // consent for the confirmation panel underneath it.
    app.message(Message::Reveal {
        generation: app.generation,
        name: "unrelated".into(),
        result: Err(anyhow::anyhow!("current failure")),
    });
    assert!(app.error_panel);
    key(&mut app, KeyCode::Enter);
    assert!(!app.error_panel && app.browser_delete.is_some());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    key(&mut app, KeyCode::Enter);
    assert!(app.browser_delete.is_none());
}

#[tokio::test]
async fn editing_and_browser_selection_reset_cancelled_operation_state() {
    let (mut app, _) = browser_deletion_fixture();
    app.loading.insert("pending".into());
    app.busy_copy = true;
    key(&mut app, KeyCode::Enter);
    assert!(app.loading.is_empty() && !app.busy_copy && !app.preview_busy);
    ctrl(&mut app, 'y');
    assert!(app.busy_copy);
    key(&mut app, KeyCode::Char('x'));
    assert!(!app.busy_copy);
    ctrl(&mut app, 'y');
    assert!(app.busy_copy);
    app.paste("changed".into());
    assert!(!app.busy_copy && !app.preview_busy);
    key(&mut app, KeyCode::F(1));
    assert!(matches!(app.view, View::Help));
    key(&mut app, KeyCode::Esc);
    assert!(matches!(app.view, View::Editor(_)));
}

#[test]
fn browser_delete_is_inert_for_new_filtered_empty_and_loading_rows() {
    let (mut app, _) = browser_deletion_fixture();
    for state in 0..3 {
        if let View::Browser { browser, .. } = &mut app.view {
            browser.selected = usize::from(state == 0);
            browser.query = if state == 1 { "no match" } else { "" }.into();
            browser.loading = state == 2;
        }
        ctrl(&mut app, 'd');
        assert!(app.browser_delete.is_none());
    }
}

#[test]
fn browser_deletion_conflict_and_persistence_errors_report_actual_state() {
    use crate::config::WriteBoundary;
    for failure in [
        WriteBoundary::Write,
        WriteBoundary::FileSync,
        WriteBoundary::Replace,
        WriteBoundary::DirectorySync,
    ] {
        let (mut app, path) = browser_deletion_fixture();
        let config = std::fs::read(&app.config.path).unwrap();
        let before = std::fs::read(&path).unwrap();
        ctrl(&mut app, 'd');
        app.delete_browser_record(|stage| {
            if stage == failure {
                anyhow::bail!("injected failure");
            }
            Ok(())
        });
        assert!(app.error);
        assert_eq!(std::fs::read(&app.config.path).unwrap(), config);
        if failure == WriteBoundary::DirectorySync {
            assert!(
                app.status
                    .contains("removed but storage durability uncertain")
            );
            assert!(
                matches!(&app.view, View::Browser { browser, .. } if browser.choices.is_empty())
            );
        } else {
            assert!(app.status.contains("kept"));
            assert_eq!(std::fs::read(&path).unwrap(), before);
            assert!(
                matches!(&app.view, View::Browser { browser, .. } if browser.choices.len() == 1)
            );
        }
    }
    let (mut app, path) = browser_deletion_fixture();
    ctrl(&mut app, 'd');
    let mut changed = std::fs::read(&path).unwrap();
    changed.push(b' ');
    std::fs::write(&path, &changed).unwrap();
    key(&mut app, KeyCode::Char('y'));
    assert!(app.error);
    assert!(app.status.contains("kept"));
    assert_eq!(std::fs::read(&path).unwrap(), changed);
}

#[test]
fn deletion_persistence_boundaries_report_actual_surviving_files_and_order() {
    use crate::config::WriteBoundary;
    let stages = [
        WriteBoundary::Write,
        WriteBoundary::FileSync,
        WriteBoundary::Replace,
        WriteBoundary::DirectorySync,
    ];
    for config_failure in [None].into_iter().chain(stages.into_iter().map(Some)) {
        for storage_failure in [None].into_iter().chain(stages.into_iter().map(Some)) {
            let (mut app, path, selected) = deletion_fixture();
            let before = std::fs::read(&path).unwrap();
            let config_path = app.config.path.clone();
            let mut storage_called = false;
            app.delete_mapping(
                "FIRST",
                Some(Ok(selected)),
                |stage| {
                    anyhow::ensure!(Some(stage) != config_failure, "Injected config failure");
                    Ok(())
                },
                |stage| {
                    storage_called = true;
                    assert!(
                        !Config::load(config_path.clone())
                            .unwrap()
                            .entries
                            .contains_key("FIRST")
                    );
                    anyhow::ensure!(Some(stage) != storage_failure, "Injected storage failure");
                    Ok(())
                },
            );
            let config_changed =
                config_failure.is_none() || config_failure == Some(WriteBoundary::DirectorySync);
            let storage_changed = config_failure.is_none()
                && (storage_failure.is_none()
                    || storage_failure == Some(WriteBoundary::DirectorySync));
            assert_eq!(
                !Config::load(app.config.path.clone())
                    .unwrap()
                    .entries
                    .contains_key("FIRST"),
                config_changed
            );
            assert_eq!(std::fs::read(&path).unwrap() != before, storage_changed);
            assert_eq!(storage_called, config_failure.is_none());
            assert!(app.config.entries.contains_key("SHARED"));
            assert!(
                app.status
                    .contains(if storage_changed { "removed" } else { "kept" })
            );
            if config_failure == Some(WriteBoundary::DirectorySync)
                || (config_failure.is_none()
                    && storage_failure == Some(WriteBoundary::DirectorySync))
            {
                assert!(app.status.contains("durability uncertain"));
            }
        }
    }
}

#[test]
fn deletion_conflicts_locks_and_swapped_symlinks_keep_ciphertext() {
    use fs2::FileExt;
    for failure in [
        "config_conflict",
        "config_lock",
        "storage_conflict",
        "storage_lock",
        "storage_symlink",
    ] {
        let (mut app, path, selected) = deletion_fixture();
        let before = std::fs::read(&path).unwrap();
        let mut lock = None;
        match failure {
            "config_conflict" => {
                let mut bytes = std::fs::read(&app.config.path).unwrap();
                bytes.push(b' ');
                std::fs::write(&app.config.path, bytes).unwrap();
            }
            "storage_conflict" => {
                let mut bytes = before.clone();
                bytes.push(b' ');
                std::fs::write(&path, bytes).unwrap();
            }
            "storage_symlink" => {
                std::fs::rename(&path, path.with_extension("original")).unwrap();
                std::os::unix::fs::symlink(path.with_extension("original"), &path).unwrap();
            }
            _ => {
                let lock_path = if failure == "config_lock" {
                    app.config.lock_path()
                } else {
                    path.with_extension("lock")
                };
                let file = std::fs::OpenOptions::new()
                    .create(true)
                    .truncate(false)
                    .read(true)
                    .write(true)
                    .open(lock_path)
                    .unwrap();
                file.lock_exclusive().unwrap();
                lock = Some(file);
            }
        }
        let config_before = std::fs::read(&app.config.path).unwrap();
        let storage_before = std::fs::read(&path).unwrap();
        app.delete_mapping("FIRST", Some(Ok(selected)), |_| Ok(()), |_| Ok(()));
        assert!(
            app.error && app.status.contains("ciphertext kept"),
            "{}",
            app.status
        );
        assert_eq!(std::fs::read(&path).unwrap(), storage_before);
        if failure.starts_with("config") {
            assert_eq!(std::fs::read(&app.config.path).unwrap(), config_before);
            assert!(app.status.contains("Configuration unchanged"));
        } else {
            assert!(
                !Config::load(app.config.path.clone())
                    .unwrap()
                    .entries
                    .contains_key("FIRST")
            );
            assert!(app.status.contains("Mapping removed"));
        }
        drop(lock);
    }
}

#[test]
fn deletion_default_cancel_unresolved_and_stale_results_are_safe() {
    let (mut app, path, selected) = deletion_fixture();
    let before = std::fs::read(&path).unwrap();
    ctrl(&mut app, 'd');
    assert!(screen(&mut app).contains("[ ] Remove ciphertext"));
    assert!(screen(&mut app).contains("Known shared references"));
    key(&mut app, KeyCode::Tab);
    assert!(app.delete_remove);
    assert!(screen(&mut app).contains("[x] Remove ciphertext"));
    key(&mut app, KeyCode::Tab);
    assert!(!app.delete_remove);
    for code in [KeyCode::Char('r'), KeyCode::Char(' ')] {
        key(&mut app, code);
        assert!(!app.delete_remove);
    }
    key(&mut app, KeyCode::Tab);
    assert!(app.delete_remove);
    key(&mut app, KeyCode::Esc);
    assert!(app.config.entries.contains_key("FIRST"));
    app.message(Message::Deletion {
        generation: app.generation - 1,
        name: "FIRST".into(),
        result: Ok(selected),
    });
    assert_eq!(std::fs::read(&path).unwrap(), before);
    ctrl(&mut app, 'd');
    key(&mut app, KeyCode::Enter);
    assert!(!app.config.entries.contains_key("FIRST"));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    app.delete_mapping(
        "SHARED",
        Some(Err(anyhow::anyhow!("Ambiguous identity"))),
        |_| Ok(()),
        |_| panic!("Must keep ciphertext"),
    );
    assert!(app.status.contains("Mapping removed; ciphertext kept"));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    ctrl(&mut app, 'n');
    assert!(matches!(app.view, View::Editor(_)));
}

#[test]
fn deletion_long_name_reserves_consent_at_supported_sizes() {
    let (mut app, path, _) = deletion_fixture();
    let before = std::fs::read(&path).unwrap();
    let name = "X".repeat(1000);
    let entry = app.config.entries.shift_remove("FIRST").unwrap();
    app.config.entries.insert(name.clone(), entry);
    app.delete = Some(name.clone());
    for (width, height) in [(78, 11), (45, 12), (120, 30)] {
        // 78×11 describes the popup; the terminal minimum height is 12.
        let mut terminal = Terminal::new(TestBackend::new(width, height.max(12))).unwrap();
        for remove in [false, true] {
            for busy in [false, true] {
                app.delete_remove = remove;
                app.delete_busy = busy;
                terminal.draw(|f| draw(f, &mut app)).unwrap();
                let text = terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|cell| cell.symbol())
                    .collect::<String>();
                for expected in [
                    "…?",
                    "Removes reference from this config.",
                    if remove {
                        "[x] Remove ciphertext"
                    } else {
                        "[ ] Remove ciphertext"
                    },
                    "Tab  toggle",
                    "Known shared references in loaded config",
                    "may be affected.",
                    "Enter  delete",
                    "Esc  cancel",
                ] {
                    assert!(
                        text.contains(expected),
                        "{width}×{height}: missing {expected}: {text}"
                    );
                }
                assert!(!text.contains("Enter / y"));
                assert!(!text.contains("Esc / n"));
                let popup = centered(Rect::new(0, 0, width, height.max(12)), 78, 11);
                let body = block(" Confirm deletion ").inner(popup);
                let hints = shortcut_lines(
                    &[("Enter", "delete"), ("Esc", "cancel"), ("Tab", "toggle")],
                    body.width.into(),
                );
                let footer_y = body.y + body.height - hints.len() as u16;
                let buffer = terminal.backend().buffer();
                for x in body.x..body.x + body.width {
                    assert_eq!(buffer[(x, body.y + 1)].symbol(), " ");
                }
                let title_x = (popup.x..popup.x + popup.width)
                    .find(|x| buffer[(*x, popup.y)].symbol() == "C")
                    .unwrap();
                assert_eq!(buffer[(title_x, popup.y)].fg, Color::LightRed);
                let footer: String = (footer_y..body.y + body.height)
                    .flat_map(|y| (body.x..body.x + body.width).map(move |x| (x, y)))
                    .map(|position| buffer[position].symbol())
                    .collect();
                assert!(footer.contains("Enter  delete"));
                assert!(footer.contains("Esc  cancel"));
                assert!(footer.contains("Tab  toggle"));
                for offset in 1..6 {
                    assert_eq!(buffer[(body.x + offset, footer_y)].fg, Color::LightRed);
                    assert_eq!(buffer[(body.x + offset, footer_y)].bg, PANEL);
                }
                if busy {
                    assert!(text.contains("Selecting identity… Esc cancels."));
                }
            }
        }
    }
    app.delete_busy = false;
    key(&mut app, KeyCode::Esc);
    assert!(app.config.entries.contains_key(&name));
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn empty_actions_and_draft_cancel() {
    let mut app = app();
    for code in [
        KeyCode::Down,
        KeyCode::Backspace,
        KeyCode::Char(' '),
        KeyCode::Char('c'),
    ] {
        key(&mut app, code);
    }
    assert_eq!(app.selected, 0);
    assert!(matches!(app.view, View::Main));
    key(&mut app, KeyCode::Enter);
    assert!(matches!(app.view, View::Editor(_)));
    key(&mut app, KeyCode::Esc);
    assert!(app.config.entries.is_empty());
    assert!(!app.config.path.exists());
}
#[tokio::test]
async fn empty_manual_value_is_a_red_esc_only_prompt() {
    for preview in [false, true] {
        let mut app = legacy_app();
        app.config.entries["SECRET_1"].value.clear();
        let entry = app.config.entries["SECRET_1"].clone();
        let result = providers::fetch(&entry, app.config.dir()).await;
        assert!(result.is_err());
        if preview {
            app.view = View::Editor(Editor::new(
                Some("SECRET_1".into()),
                "SECRET_1".into(),
                entry,
            ));
            app.message(Message::Preview {
                generation: app.generation,
                result,
            });
        } else {
            app.message(Message::Reveal {
                generation: app.generation,
                name: "SECRET_1".into(),
                result,
            });
        }
        assert!(app.error_prompt());
        let (tx, _) = mpsc::unbounded_channel();
        let mut tasks = Tasks(vec![]);
        for code in [
            KeyCode::Enter,
            KeyCode::Char('q'),
            KeyCode::Char('a'),
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::PageDown,
        ] {
            assert!(!app.key(KeyEvent::new(code, KeyModifiers::NONE), &tx, &mut tasks));
            assert!(app.error_prompt());
        }
        app.paste("ignored".into());
        assert!(app.config.entries["SECRET_1"].value.is_empty());
        assert!(!app.config.path.exists());
        for (width, height) in [(45, 12), (120, 30)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            let buffer = terminal.backend().buffer();
            let text = screen_text(buffer);
            assert!(text.contains("Manual secret is empty"));
            assert!(text.contains(if preview { "Esc  back" } else { "Esc  quit" }));
            for absent in [
                "Failure details",
                "PgUp/PgDn",
                "Other keys",
                "Ctrl+S",
                "q  quit",
            ] {
                assert!(!text.contains(absent), "unexpected {absent}: {text}");
            }
            let popup = centered(Rect::new(0, 0, width, height), 45, 7);
            assert_eq!(buffer[(popup.x, popup.y)].fg, Color::LightRed);
            let body = block("").inner(popup);
            assert_eq!(buffer[(body.x, body.y)].fg, Color::LightRed);
            assert_eq!(
                buffer[(body.x + 1, body.y + body.height - 1)].fg,
                Color::LightRed
            );
        }
        assert_eq!(
            app.key(
                KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                &tx,
                &mut tasks
            ),
            !preview
        );
        assert!(matches!(app.view, View::Main));
        if preview {
            assert!(!app.error_panel);
            assert!(!app.error_prompt());
        }
    }
}

#[test]
fn missing_clipboard_helper_is_an_esc_only_prompt() {
    let mut app = legacy_app();
    app.busy_copy = true;
    app.message(Message::Copy {
        generation: app.generation,
        result: Err(anyhow::anyhow!(
            "No clipboard helper found (install wl-copy, xclip or xsel)"
        )),
    });
    assert!(app.error_prompt());
    assert!(!app.busy_copy);
    for (width, height) in [(45, 12), (120, 30)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let text = screen_text(terminal.backend().buffer());
        let compact = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        let body = block("").inner(centered(Rect::new(0, 0, width, height), 45, 7));
        let buffer = terminal.backend().buffer();
        let message: String = (body.y..body.y + body.height - 1)
            .flat_map(|y| (body.x..body.x + body.width).map(move |x| (x, y)))
            .map(|position| buffer[position].symbol())
            .collect();
        assert!(compact(&message).contains(&compact(&app.status)));
        assert!(text.contains("Esc  quit"));
        for absent in ["Failure details", "PgUp/PgDn", "Other keys", "q  quit"] {
            assert!(!text.contains(absent));
        }
    }
    key(&mut app, KeyCode::Char('q'));
    assert!(app.error_prompt());
    let (tx, _) = mpsc::unbounded_channel();
    assert!(app.key(
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        &tx,
        &mut Tasks(vec![]),
    ));
    assert!(matches!(app.view, View::Main));
}

#[test]
fn stale_results_and_control_characters() {
    let mut app = app();
    app.generation = 2;
    app.message(Message::Reveal {
        generation: 1,
        name: "X".into(),
        result: Ok(Zeroizing::new("secret".into())),
    });
    assert!(app.revealed.is_empty());
    assert_eq!(clean("\x1b[31m\n"), " [31m ");
}
#[test]
fn renders_sizes_and_masking() {
    let mut app = app();
    app.config.entries.insert(
        "API_KEY".into(),
        Entry {
            source: "manual".into(),
            value: "NEVER_RENDER_THIS".into(),
        },
    );
    for (w, h) in [(120, 30), (55, 16), (20, 5)] {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(!text.contains("NEVER_RENDER_THIS"));
        assert!(text.contains("psst 🤫"));
        if w >= 45 {
            assert!(text.contains("API_KEY"), "{w}×{h}: {text}");
        }
    }
}
// Equivalents of main:test/Console/{MainView,Secrets,Help}.test.tsx.
fn legacy_app() -> App {
    let mut app = app();
    for (name, value) in [("SECRET_1", "value1"), ("SECRET_2", "value2")] {
        app.config.entries.insert(
            name.into(),
            Entry {
                source: "manual".into(),
                value: value.into(),
            },
        );
    }
    app
}
fn screen(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal.draw(|f| draw(f, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect()
}
fn assert_narrow_failure_readable(app: &mut App) {
    let mut terminal = Terminal::new(TestBackend::new(45, 12)).unwrap();
    let mut text = String::new();
    for offset in 0..200 {
        app.error_scroll = offset;
        terminal.draw(|f| draw(f, app)).unwrap();
        let buffer = terminal.backend().buffer();
        assert!(screen_text(buffer).contains("PgUp/PgDn scroll details"));
        let rows = if app.error_scroll < offset {
            2..8
        } else {
            2..3
        };
        for y in rows {
            for x in 3..42 {
                text.push_str(buffer[(x, y)].symbol());
            }
        }
        if app.error_scroll < offset {
            break;
        }
    }
    let compact = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    assert!(
        compact(&text).contains(&compact(&clean(&app.status))),
        "{text}"
    );
    key(app, KeyCode::PageUp);
    assert!(app.error_panel);
}
fn screen_text(buffer: &Buffer) -> String {
    buffer.content.iter().map(|cell| cell.symbol()).collect()
}
#[test]
fn ctrl_d_y_and_cancel_n() {
    let mut app = legacy_app();
    ctrl(&mut app, 'd');
    assert_eq!(app.delete.as_deref(), Some("SECRET_1"));
    assert!(app.config.entries.contains_key("SECRET_1"));
    assert!(screen(&mut app).contains("Delete SECRET_1?"));
    key(&mut app, KeyCode::Char('n'));
    assert!(app.delete.is_none());
    assert!(app.config.entries.contains_key("SECRET_1"));
    ctrl(&mut app, 'd');
    key(&mut app, KeyCode::Char('y'));
    assert!(app.delete.is_none());
    assert!(!app.config.entries.contains_key("SECRET_1"));
    assert!(app.config.entries.contains_key("SECRET_2"));
    assert!(
        !Config::load(app.config.path.clone())
            .unwrap()
            .entries
            .contains_key("SECRET_1")
    );
}
#[test]
fn dashboard_delete_requires_selection_and_plain_delete_is_inert() {
    let mut app = legacy_app();
    app.config.save(app.config.entries.clone()).unwrap();
    let before = std::fs::read(&app.config.path).unwrap();
    key(&mut app, KeyCode::Delete);
    key(&mut app, KeyCode::Backspace);
    assert!(app.delete.is_none());
    assert!(matches!(app.view, View::Main));
    assert_eq!(app.selected_name().as_deref(), Some("SECRET_1"));
    assert_eq!(app.config.entries.len(), 2);
    assert_eq!(std::fs::read(&app.config.path).unwrap(), before);

    app.paste("no matching entry".into());
    assert!(app.selected_name().is_none());
    ctrl(&mut app, 'd');
    assert!(app.delete.is_none());
    assert!(matches!(app.view, View::Main));
    assert_eq!(app.config.entries.len(), 2);
    assert_eq!(std::fs::read(&app.config.path).unwrap(), before);

    let mut empty = self::app();
    ctrl(&mut empty, 'd');
    assert!(empty.delete.is_none());
    assert!(matches!(empty.view, View::Main));
    assert!(empty.config.entries.is_empty());
    assert!(!empty.config.path.exists());
}

#[test]
fn d_backspace_and_delete_preserve_filter_and_editor_input() {
    let mut app = legacy_app();
    key(&mut app, KeyCode::Char('d'));
    assert_eq!(app.query, "d");
    key(&mut app, KeyCode::Delete);
    assert_eq!(app.query, "d");
    key(&mut app, KeyCode::Backspace);
    assert!(app.query.is_empty());
    assert!(app.delete.is_none());
    key(&mut app, KeyCode::Esc);

    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::End);
    key(&mut app, KeyCode::Char('d'));
    assert!(matches!(&app.view, View::Editor(e) if e.name == "SECRET_1d"));
    key(&mut app, KeyCode::Left);
    key(&mut app, KeyCode::Delete);
    assert!(matches!(&app.view, View::Editor(e) if e.name == "SECRET_1"));
    key(&mut app, KeyCode::Backspace);
    assert!(matches!(&app.view, View::Editor(e) if e.name == "SECRET_"));
    assert!(app.delete.is_none());
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.config.entries.len(), 2);
}

#[test]
fn dashboard_footer_advertises_ctrl_d_delete_at_supported_sizes() {
    let mut app = legacy_app();
    for (width, height) in [(45, 12), (55, 30), (120, 30)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();
        let row = (0..height)
            .find(|y| {
                (0..width - 14).any(|x| {
                    (0..14)
                        .map(|offset| buffer[(x + offset, *y)].symbol())
                        .collect::<String>()
                        == "Ctrl+D  delete"
                })
            })
            .expect("Ctrl+D delete action must be visible on one row");
        let x = (0..width - 6)
            .find(|x| {
                (0..6)
                    .map(|offset| buffer[(*x + offset, row)].symbol())
                    .collect::<String>()
                    == "Ctrl+D"
            })
            .unwrap();
        for offset in 0..6 {
            assert_eq!(buffer[(x + offset, row)].fg, ACCENT);
            assert_eq!(buffer[(x + offset, row)].bg, PANEL);
        }
        assert_eq!(buffer[(x + 8, row)].fg, Color::White);
        assert_eq!(buffer[(x + 8, row)].bg, BG);
    }
}

#[test]
fn legacy_navigation_and_editing_selected_secret() {
    let mut app = legacy_app();
    for (navigate, expected) in [
        (None, "SECRET_1"),
        (Some(KeyCode::Down), "SECRET_2"),
        (Some(KeyCode::Up), "SECRET_1"),
    ] {
        if let Some(code) = navigate {
            key(&mut app, code);
        }
        // Enter deliberately replaces the old Right-arrow edit shortcut.
        key(&mut app, KeyCode::Enter);
        assert!(matches!(&app.view, View::Editor(e) if e.original.as_deref() == Some(expected)));
        key(&mut app, KeyCode::Esc);
    }
}
#[tokio::test]
async fn dashboard_ctrl_r_fetches_then_hides() {
    let mut app = legacy_app();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut tasks = Tasks(vec![]);
    assert!(!screen(&mut app).contains("value1"));
    app.key(
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
        &tx,
        &mut tasks,
    );
    assert!(app.loading.contains("SECRET_1"));
    assert!(screen(&mut app).contains("Fetching…"));
    app.message(
        tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap(),
    );
    assert!(app.loading.is_empty());
    assert!(screen(&mut app).contains("value1"));
    app.key(
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
        &tx,
        &mut tasks,
    );
    assert!(app.revealed.is_empty());
    assert!(!screen(&mut app).contains("value1"));
}
#[test]
fn ctrl_n_opens_a_real_unsaved_draft() {
    let mut app = legacy_app();
    ctrl(&mut app, 'n');
    assert!(matches!(&app.view, View::Editor(e) if e.original.is_none() && e.name == "SECRET_3"));
    assert_eq!(app.config.entries.len(), 2);
    assert!(!app.config.path.exists());
}
#[test]
fn f1_opens_help_and_esc_returns() {
    let mut app = legacy_app();
    key(&mut app, KeyCode::F(1));
    assert!(matches!(app.view, View::Help));
    assert!(screen(&mut app).contains("MANAGE ENTRIES"));
    key(&mut app, KeyCode::Esc);
    assert!(matches!(app.view, View::Main));
}
#[test]
fn help_scrolls_with_arrows_and_keeps_shortcuts_visible() {
    for (width, height) in [(45, 12), (55, 16), (120, 30)] {
        let mut app = legacy_app();
        key(&mut app, KeyCode::F(1));
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut seen = String::new();
        let mut previous = None;
        loop {
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            let text = screen_text(terminal.backend().buffer());
            for hint in ["↑↓  scroll", "Esc  back"] {
                assert!(text.contains(hint), "{width}x{height}: missing {hint}");
            }
            seen.push_str(&text);
            if previous == Some(app.help_scroll) {
                break;
            }
            previous = Some(app.help_scroll);
            key(&mut app, KeyCode::Down);
        }
        assert!(app.help_scroll > 0);
        for section in [
            "MANAGE ENTRIES",
            "EDIT AN ENTRY",
            "CREATE OR UPDATE ENCRYPTED VALUES",
            "BROWSE REFERENCES",
            "COMMAND LINE AND SAFETY",
        ] {
            assert!(seen.contains(section), "missing {section}");
        }
        assert!(seen.contains("Ctrl+C quits from any view."));
        let bottom = app.help_scroll;
        key(&mut app, KeyCode::Up);
        assert_eq!(app.help_scroll, bottom - 1);
        for code in [
            KeyCode::Home,
            KeyCode::End,
            KeyCode::PageDown,
            KeyCode::PageUp,
            KeyCode::Enter,
            KeyCode::Char('h'),
            KeyCode::Char('?'),
            KeyCode::Char('j'),
            KeyCode::Char('k'),
        ] {
            key(&mut app, code);
            assert_eq!(app.help_scroll, bottom - 1);
            assert!(matches!(app.view, View::Help));
        }
        terminal.backend_mut().resize(120, 100);
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert_eq!(app.help_scroll, 0);
        let buffer = terminal.backend().buffer();
        let notes_row = (0..100)
            .find(|y| {
                (2..118)
                    .map(|x| buffer[(x, *y)].symbol())
                    .collect::<String>()
                    .contains("Manual values and .env files are plaintext.")
            })
            .unwrap();
        assert!((2..118).all(|x| buffer[(x, notes_row - 1)].symbol().trim().is_empty()));
        let text = screen_text(buffer);
        for removed in ["PgUp/PgDn", "Home/End", "Esc/Enter"] {
            assert!(!text.contains(removed));
        }
        key(&mut app, KeyCode::Esc);
        assert!(matches!(app.view, View::Main));
        key(&mut app, KeyCode::F(1));
        assert_eq!(app.help_scroll, 0);
        key(&mut app, KeyCode::Esc);
        assert!(matches!(app.view, View::Main));
    }
}

#[test]
fn legacy_all_names_safe_references_and_fixed_length_manual_masks() {
    let mut app = legacy_app();
    app.config.entries["SECRET_1"].value = "a".into();
    app.config.entries["SECRET_2"].value = "verylongsecret".into();
    let text = screen(&mut app);
    assert!(text.contains("SECRET_1") && text.contains("SECRET_2"));
    assert_eq!(text.matches(MASK).count(), 4); // reference + secret, each row
    assert!(!text.contains("verylongsecret"));
    app.config.entries.insert(
        "OP".into(),
        Entry {
            source: "op".into(),
            value: "op://vault/item/field".into(),
        },
    );
    app.config.entries.insert(
        "SHELL".into(),
        Entry {
            source: "shell".into(),
            value: "echo 1".into(),
        },
    );
    let text = screen(&mut app);
    assert!(text.contains("op://vault/item/field"));
    assert!(text.contains("echo 1"));
}
#[test]
fn legacy_manual_shell_and_op_have_usable_editors() {
    for source in ["manual", "shell", "op"] {
        let mut app = app();
        app.config.entries.insert(
            "KEY".into(),
            Entry {
                source: source.into(),
                value: "original".into(),
            },
        );
        key(&mut app, KeyCode::Enter);
        let View::Editor(e) = &mut app.view else {
            panic!("editor missing")
        };
        assert_eq!(e.source, source);
        e.focus(2);
        e.change_text("-edited");
        app.save();
        assert!(matches!(app.view, View::Main));
        assert_eq!(
            Config::load(app.config.path.clone()).unwrap().entries["KEY"].value,
            "original-edited"
        );
    }
}
#[test]
fn editor_reveals_expire_in_editor_and_browser() {
    for in_browser in [false, true] {
        let mut app = app();
        let mut editor = Editor::new(
            None,
            "KEY".into(),
            Entry {
                source: "manual".into(),
                value: "literal".into(),
            },
        );
        editor.show = true;
        editor.shown_at = Some(Instant::now() - Duration::from_secs(31));
        editor.preview = Some(Zeroizing::new("preview".into()));
        editor.preview_at = editor.shown_at;
        app.view = if in_browser {
            View::Browser {
                editor,
                browser: Browser {
                    stack: vec![providers::get("op").unwrap().browser().unwrap()],
                    choices: vec![],
                    query: String::new(),
                    selected: 0,
                    loading: false,
                },
            }
        } else {
            View::Editor(editor)
        };
        app.expire();
        let editor = match &app.view {
            View::Editor(editor) | View::Browser { editor, .. } => editor,
            _ => unreachable!(),
        };
        assert!(!editor.show);
        assert!(editor.shown_at.is_none());
        assert!(editor.preview.is_none());
        assert!(editor.preview_at.is_none());
    }
}
#[test]
fn literal_reveal_toggle_sets_and_clears_expiry_without_fetching() {
    let mut app = legacy_app();
    key(&mut app, KeyCode::Enter);
    let (tx, _) = mpsc::unbounded_channel();
    let mut tasks = Tasks(vec![]);
    for expected in [true, false] {
        app.key(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
            &tx,
            &mut tasks,
        );
        assert!(tasks.0.is_empty());
        assert!(!app.preview_busy);
        let View::Editor(editor) = &app.view else {
            panic!("editor missing")
        };
        assert_eq!(editor.show, expected);
        assert_eq!(editor.shown_at.is_some(), expected);
    }
}
#[test]
fn vault_editor_label_identifies_the_provider() {
    let mut app = app();
    app.view = View::Editor(Editor::new(
        None,
        "KEY".into(),
        Entry {
            source: "vault".into(),
            value: "/secret/project#password".into(),
        },
    ));
    assert!(screen(&mut app).contains("Vault (KV v2) · path#field · Ctrl+B browse"));
}
#[test]
fn shortcut_badges_wrap_between_actions() {
    let lines = shortcut_lines(&[("Enter", "edit"), ("Ctrl+S", "save")], 16);
    assert_eq!(lines.len(), 2);
    for line in lines {
        assert!(line.width() <= 16);
        assert_eq!(line.spans[0].style.fg, Some(ACCENT));
        assert_eq!(line.spans[0].style.bg, Some(PANEL));
        assert_eq!(line.spans[1].style.fg, Some(Color::White));
        assert_eq!(line.spans[1].style.bg, None);
    }
}
#[test]
fn footer_renders_key_badges_in_each_view() {
    let mut app = legacy_app();
    for width in [55, 120] {
        for view in 0..6 {
            app.query.clear();
            app.delete = None;
            app.view = match view {
                2 => View::Editor(Editor::new(
                    None,
                    "KEY".into(),
                    Entry {
                        source: "manual".into(),
                        value: String::new(),
                    },
                )),
                3 => View::Browser {
                    editor: Editor::new(
                        None,
                        "KEY".into(),
                        Entry {
                            source: "op".into(),
                            value: String::new(),
                        },
                    ),
                    browser: Browser {
                        stack: vec![providers::get("op").unwrap().browser().unwrap()],
                        choices: vec![],
                        query: String::new(),
                        selected: 0,
                        loading: false,
                    },
                },
                4 => View::Help,
                _ => View::Main,
            };
            if view == 1 {
                app.query = "SECRET".into();
            }
            if view == 5 {
                app.delete = Some("SECRET_1".into());
            }
            let mut terminal = Terminal::new(TestBackend::new(width, 30)).unwrap();
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            let buffer = terminal.backend().buffer();
            let row = (25..30).find(|y| buffer[(1, *y)].bg == PANEL).unwrap();
            let key = &buffer[(1, row)];
            assert_eq!(key.fg, ACCENT);
            let label_x = (2..width - 1)
                .find(|x| buffer[(*x, row)].bg != PANEL)
                .unwrap();
            assert_eq!(buffer[(label_x, row)].bg, BG);
            assert_eq!(buffer[(label_x, row)].fg, Color::White);
        }
    }
}
fn creator(app: &mut App) {
    app.view = View::Create(Creator {
        editor: Editor::new(
            None,
            "KEY".into(),
            Entry {
                source: "local-storage".into(),
                value: "id".into(),
            },
        ),
        secret: Zeroizing::new(String::new()),
        field: 2,
        cursor: 0,
        keys: vec![],
        selected: 0,
        chosen: None,
        busy: false,
        pending: None,
        return_browser: None,
        return_value: None,
    });
}
#[test]
fn encrypted_editor_preserves_exact_text_and_is_masked_by_default() {
    let mut app = app();
    creator(&mut app);
    for text in ["", " \n🔑\t", "PASTED_SECRET"] {
        app.paste(text.into());
    }
    key(&mut app, KeyCode::Enter);
    let expected = " \n🔑\tPASTED_SECRET\n";
    assert!(matches!(&app.view, View::Create(c) if c.secret.as_str() == expected));
    let (tx, _) = mpsc::unbounded_channel();
    for code in ['p', 'b'] {
        app.key(
            KeyEvent::new(KeyCode::Char(code), KeyModifiers::CONTROL),
            &tx,
            &mut Tasks(vec![]),
        );
    }
    assert!(!screen(&mut app).contains("PASTED_SECRET"));
    app.paste("bad\0paste".into());
    key(&mut app, KeyCode::Char('\0'));
    assert!(app.error && app.status.contains("NUL rejected"));
    assert!(matches!(&app.view, View::Create(c) if c.secret.as_str() == expected));
    key(&mut app, KeyCode::Backspace);
    key(&mut app, KeyCode::Home);
    key(&mut app, KeyCode::Delete);
    assert!(matches!(&app.view, View::Create(c) if c.secret.as_str() == "\n🔑\tPASTED_SECRET"));
    key(&mut app, KeyCode::Esc);
    assert!(matches!(app.view, View::Main));
    assert!(!app.config.path.exists());
}
#[test]
fn encrypted_input_limit_is_exact_bytes_including_newline_and_paste() {
    let mut app = app();
    creator(&mut app);
    app.paste("x".repeat(providers::local_storage::VALUE_LIMIT));
    for code in [KeyCode::Enter, KeyCode::Char('é')] {
        key(&mut app, code);
    }
    app.paste("extra".into());
    assert!(app.error);
    assert!(
        matches!(&app.view, View::Create(c) if c.secret.len() == providers::local_storage::VALUE_LIMIT)
    );
    assert!(!screen(&mut app).contains("xxxxxxxx"));
}
#[test]
fn encrypted_focused_fields_and_full_fingerprints_at_supported_sizes() {
    for (width, height) in [(45, 12), (45, 16), (55, 12), (80, 12), (45, 30), (120, 30)] {
        let mut app = app();
        creator(&mut app);
        app.paste("NEVER_RENDER_THIS\n🔑 ".into());
        let fingerprints = (0..8)
            .map(|i| format!("SHA256:{}{i}", "a".repeat(42)))
            .collect::<Vec<_>>();
        app.message(Message::Identities {
            generation: 0,
            result: Ok(fingerprints
                .iter()
                .map(|fingerprint| providers::Identity {
                    fingerprint: fingerprint.clone(),
                    token: vec![],
                })
                .collect()),
        });
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        for field in 0..3 {
            if let View::Create(c) = &mut app.view {
                c.focus(field);
            }
            terminal.draw(|f| draw(f, &mut app)).unwrap();
            let text = screen_text(terminal.backend().buffer());
            assert!(
                text.contains(["Environment name", "Local storage ID", "Exact UTF-8 value"][field]),
                "{width}×{height}: {text}"
            );
            assert!(!text.contains("NEVER_RENDER_THIS"));
            assert!(text.contains("Ctrl+S") && text.contains("Ctrl+R") && text.contains("Esc"));
            if field == 2 {
                assert!(text.contains(MASK));
            }
        }
        key(&mut app, KeyCode::Tab);
        for (index, fingerprint) in fingerprints.iter().enumerate() {
            terminal.draw(|f| draw(f, &mut app)).unwrap();
            let buffer = terminal.backend().buffer();
            // Join the list interior across wrapped rows, excluding borders.
            let mut interior = String::new();
            for y in 0..height {
                for x in 2..width - 2 {
                    let symbol = buffer[(x, y)].symbol();
                    if !symbol.trim().is_empty() && symbol != "│" {
                        interior.push_str(symbol);
                    }
                }
            }
            assert!(
                interior.contains(fingerprint),
                "{width}×{height}, key {index}: {interior}"
            );
            assert!(matches!(&app.view, View::Create(c) if c.chosen == index.checked_sub(1)));
            key(&mut app, KeyCode::Enter);
            assert!(matches!(&app.view, View::Create(c) if c.chosen == Some(index)));
            key(&mut app, KeyCode::Down);
        }
        assert!(
            matches!(&app.view, View::Create(c) if c.secret.as_str() == "NEVER_RENDER_THIS\n🔑 ")
        );
    }
}
#[test]
fn encrypted_identity_selection_errors_and_stale_listing() {
    for count in [0, 1, 2] {
        let mut app = app();
        creator(&mut app);
        let keys: Vec<_> = (0..count)
            .map(|i| providers::Identity {
                fingerprint: format!("SHA256:key{i}"),
                token: vec![],
            })
            .collect();
        app.message(Message::Identities {
            generation: 0,
            result: Ok(keys),
        });
        assert!(matches!(&app.view, View::Create(c) if c.chosen == (count == 1).then_some(0)));
        if count == 2 {
            if let View::Create(c) = &mut app.view {
                c.focus(3);
            }
            key(&mut app, KeyCode::Down);
            key(&mut app, KeyCode::Enter);
            assert!(matches!(&app.view, View::Create(c) if c.chosen == Some(1)));
        }
    }
    let mut app = app();
    creator(&mut app);
    app.message(Message::Identities {
        generation: 0,
        result: Err(anyhow::anyhow!("SSH agent refused identity listing")),
    });
    assert!(app.error && app.status.contains("refused"));
    key(&mut app, KeyCode::Esc);
    creator(&mut app);
    app.message(Message::Identities {
        generation: 0,
        result: Ok(vec![providers::Identity {
            fingerprint: "old".into(),
            token: vec![],
        }]),
    });
    assert!(matches!(&app.view, View::Create(c) if c.keys.is_empty()));
}
struct TestSave {
    path: std::path::PathBuf,
    failure: Option<crate::config::WriteBoundary>,
    replacing: bool,
}
impl providers::PreparedSave for TestSave {
    fn replacing(&self) -> bool {
        self.replacing
    }
    fn matches_reference(&self, reference: &str) -> bool {
        reference == "id"
    }
    fn save(
        self: Box<Self>,
        _value: Zeroizing<String>,
    ) -> futures::future::BoxFuture<'static, Result<String>> {
        Box::pin(async move {
            crate::config::atomic_write_checked(&self.path, b"complete ciphertext", |stage| {
                anyhow::ensure!(Some(stage) != self.failure, "Injected storage failure");
                Ok(())
            })?;
            Ok("id".into())
        })
    }
}
#[test]
fn encrypted_ctrl_u_is_inert() {
    let mut app = app();
    creator(&mut app);
    app.paste("DRAFT".into());
    let (tx, _) = mpsc::unbounded_channel();
    let mut tasks = Tasks(vec![]);
    app.key(
        KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
        &tx,
        &mut tasks,
    );
    assert!(
        matches!(&app.view, View::Create(c) if c.secret.as_str() == "DRAFT" && c.pending.is_none() && !c.busy)
    );
    assert!(!app.config.path.exists());
    assert!(tasks.0.is_empty());
    assert!(!screen(&mut app).contains("Ctrl+U"));
}

#[tokio::test]
async fn encrypted_creation_faults_report_actual_surviving_files() {
    use crate::config::WriteBoundary;
    let boundaries = [
        WriteBoundary::Write,
        WriteBoundary::FileSync,
        WriteBoundary::Replace,
        WriteBoundary::DirectorySync,
    ];
    for existing in [false, true] {
        for stage in 0..2 {
            for failure in boundaries {
                let mut app = app();
                creator(&mut app);
                let storage = app.config.dir().join("ciphertext");
                if existing {
                    std::fs::write(&storage, b"old ciphertext").unwrap();
                }
                if let View::Create(c) = &mut app.view {
                    c.pending = Some(Box::new(TestSave {
                        path: storage.clone(),
                        failure: (stage == 0).then_some(failure),
                        replacing: existing,
                    }));
                }
                let (tx, mut rx) = mpsc::unbounded_channel();
                let mut tasks = Tasks(vec![]);
                app.save_creation(&tx, &mut tasks);
                let message = tokio::time::timeout(Duration::from_secs(2), rx.recv())
                    .await
                    .unwrap()
                    .unwrap();
                // Configuration is never written by the storage task.
                assert!(!app.config.path.exists());
                app.message_checked(message, |boundary| {
                    anyhow::ensure!(stage != 1 || boundary != failure, "Injected config failure");
                    Ok(())
                });
                let storage_changed = stage == 1 || failure == WriteBoundary::DirectorySync;
                let config_changed = stage == 1 && failure == WriteBoundary::DirectorySync;
                assert_eq!(app.config.path.exists(), config_changed);
                assert_eq!(app.config.entries.contains_key("KEY"), config_changed);
                assert_eq!(
                    std::fs::read(&storage).ok(),
                    if storage_changed {
                        Some(b"complete ciphertext".to_vec())
                    } else if existing {
                        Some(b"old ciphertext".to_vec())
                    } else {
                        None
                    }
                );
                assert!(app.error);
                if stage == 0 {
                    assert!(app.status.contains(if storage_changed {
                        "Ciphertext replaced but durability uncertain"
                    } else {
                        "Ciphertext unchanged"
                    }));
                } else {
                    assert!(app.status.contains(if config_changed {
                        "Ciphertext and configuration replaced"
                    } else {
                        "Ciphertext saved; configuration unchanged"
                    }));
                }
                assert_narrow_failure_readable(&mut app);
                // Failure leaves a usable UI, with truthful loaded state.
                key(&mut app, KeyCode::Esc);
                ctrl(&mut app, 'n');
                assert!(matches!(app.view, View::Editor(_)));
            }
        }
    }
}
#[tokio::test]
async fn encrypted_cancelled_save_reports_completed_storage_without_touching_new_draft() {
    let mut app = app();
    creator(&mut app);
    let storage = app.config.dir().join("ciphertext");
    if let View::Create(c) = &mut app.view {
        c.pending = Some(Box::new(TestSave {
            path: storage.clone(),
            failure: None,
            replacing: false,
        }));
    }
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut tasks = Tasks(vec![]);
    app.save_creation(&tx, &mut tasks);
    let message = rx.recv().await.unwrap();
    key(&mut app, KeyCode::Esc);
    creator(&mut app);
    app.paste("NEW_DRAFT".into());
    app.message(message);
    assert!(storage.exists());
    assert!(!app.config.path.exists());
    assert!(
        app.status
            .contains("ciphertext was saved; configuration unchanged")
    );
    assert!(matches!(&app.view, View::Create(c) if c.secret.as_str() == "NEW_DRAFT"));
}
#[test]
fn replacement_can_be_declined_without_writes() {
    let mut app = app();
    creator(&mut app);
    let storage = app.config.dir().join("ciphertext");
    if let View::Create(c) = &mut app.view {
        c.pending = Some(Box::new(TestSave {
            path: storage.clone(),
            failure: None,
            replacing: true,
        }));
    }
    for index in 0..100 {
        app.config.entries.insert(
            format!("LONG_ENVIRONMENT_REFERENCE_NAME_{index}"),
            Entry {
                source: "local-storage".into(),
                value: "id".into(),
            },
        );
    }
    for (width, height) in [(120, 30), (45, 12)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let text = screen_text(terminal.backend().buffer());
        for expected in [
            "ALL references",
            "Loaded config only:",
            "possible references.",
            "Enter replaces",
            "Esc cancels",
        ] {
            assert!(text.contains(expected), "missing {expected}: {text}");
        }
    }
    key(&mut app, KeyCode::Char('x'));
    assert!(!storage.exists());
    key(&mut app, KeyCode::Esc);
    assert!(matches!(&app.view, View::Create(c) if c.pending.is_none()));
    assert!(!storage.exists() && !app.config.path.exists());
}
#[tokio::test]
async fn local_browser_creation_is_independent_of_environment_mapping() {
    let mut app = app();
    app.view = View::Editor(Editor::new(
        None,
        "invalid name".into(),
        Entry {
            source: "local-storage".into(),
            value: "previous".into(),
        },
    ));
    assert!(screen(&mut app).contains("Ctrl+B"));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut tasks = Tasks(vec![]);
    app.key(
        KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
        &tx,
        &mut tasks,
    );
    assert!(matches!(app.view, View::Editor(_)));
    app.key(
        KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL),
        &tx,
        &mut tasks,
    );
    app.message(Message::Browse {
        generation: app.generation,
        result: Ok(vec![]),
    });
    assert!(matches!(app.view, View::Browser { .. }));
    // Inject listing rather than ever consulting the user's SSH_AUTH_SOCK.
    app.create_with_identities(async { Ok(vec![]) }, &tx, &mut tasks);
    assert!(matches!(&app.view, View::Create(c) if c.field == 1));
    assert!(!screen(&mut app).contains("Environment name"));
    if let View::Create(c) = &mut app.view {
        c.busy = false;
    }
    app.key(
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
        &tx,
        &mut tasks,
    );
    assert!(app.status.contains("Choose an SSH fingerprint"));
    // Close the error panel before continuing the draft.
    key(&mut app, KeyCode::Tab);
    if let View::Create(c) = &mut app.view {
        c.focus(1);
    }
    app.message(Message::Identities {
        generation: app.generation,
        result: Ok(vec![providers::Identity {
            fingerprint: "SHA256:test".into(),
            token: vec![],
        }]),
    });
    for expected in [2, 3, 1] {
        key(&mut app, KeyCode::Tab);
        assert!(matches!(&app.view, View::Create(c) if c.field == expected));
    }
    let storage = app.config.dir().join("ciphertext");
    if let View::Create(c) = &mut app.view {
        c.editor.value = Zeroizing::new("id".into());
        c.pending = Some(Box::new(TestSave {
            path: storage.clone(),
            failure: None,
            replacing: false,
        }));
    }
    app.save_creation(&tx, &mut tasks);
    loop {
        let message = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let created = matches!(message, Message::Created { .. });
        app.message(message);
        if created {
            break;
        }
    }
    assert!(storage.exists());
    assert!(!app.config.path.exists());
    assert!(
        matches!(&app.view, View::Browser { editor, browser } if editor.value.as_str() == "previous" && browser.choices.len() == 1)
    );
    key(&mut app, KeyCode::Enter);
    assert!(matches!(&app.view, View::Editor(e) if e.value.as_str() == "id#SHA256:test"));
    if let View::Editor(e) = &mut app.view {
        e.name = "NEW_MAPPING".into();
    }
    app.save();
    assert_eq!(app.config.entries["NEW_MAPPING"].value, "id#SHA256:test");
}

#[tokio::test]
async fn local_browser_cancel_preserves_reference_and_returns_to_browser() {
    let mut app = app();
    app.view = View::Browser {
        editor: Editor::new(
            None,
            String::new(),
            Entry {
                source: "local-storage".into(),
                value: "previous".into(),
            },
        ),
        browser: Browser {
            stack: vec![providers::get("local-storage").unwrap().browser().unwrap()],
            choices: vec![],
            query: String::new(),
            selected: 0,
            loading: false,
        },
    };
    let (tx, _) = mpsc::unbounded_channel();
    let mut tasks = Tasks(vec![]);
    app.key(
        KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
        &tx,
        &mut tasks,
    );
    key(&mut app, KeyCode::Esc);
    assert!(
        matches!(&app.view, View::Browser { editor, .. } if editor.value.as_str() == "previous")
    );
    assert!(!app.config.path.exists());
}

#[test]
fn dashboard_new_row_is_selectable_and_not_an_entry() {
    let mut app = legacy_app();
    let count = app.config.entries.len();
    key(&mut app, KeyCode::End);
    for (width, height) in [(45, 12), (120, 30)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let text = screen_text(terminal.backend().buffer());
        assert!(text.contains("+ New entry"), "{width}×{height}: {text}");
    }
    key(&mut app, KeyCode::End);
    assert_eq!(app.selected, count);
    assert!(app.selected_name().is_none());
    for character in ['d', 'r', 'y'] {
        ctrl(&mut app, character);
        assert!(app.delete.is_none());
        assert!(app.loading.is_empty());
        assert!(!app.busy_copy);
    }
    key(&mut app, KeyCode::Enter);
    assert!(matches!(&app.view, View::Editor(e) if e.original.is_none()));
    key(&mut app, KeyCode::Esc);
    let (tx, _) = mpsc::unbounded_channel();
    app.key(
        KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
        &tx,
        &mut Tasks(vec![]),
    );
    assert!(matches!(&app.view, View::Editor(e) if e.original.is_none()));
    assert_eq!(app.config.entries.len(), count);
}

#[tokio::test]
async fn local_browser_new_row_stays_available_when_filter_has_no_matches() {
    let mut app = app();
    app.view = View::Browser {
        editor: Editor::new(
            None,
            String::new(),
            Entry {
                source: "local-storage".into(),
                value: String::new(),
            },
        ),
        browser: Browser {
            stack: vec![providers::get("local-storage").unwrap().browser().unwrap()],
            choices: vec![Choice {
                label: "existing".into(),
                detail: "fingerprint".into(),
                target: Target::Reference("existing".into()),
            }],
            query: String::new(),
            selected: 0,
            loading: false,
        },
    };
    key(&mut app, KeyCode::Down);
    assert!(matches!(&app.view, View::Browser { browser, .. } if browser.selected == 1));
    key(&mut app, KeyCode::Char('a'));
    assert!(
        matches!(&app.view, View::Browser { browser, .. } if browser.query == "a" && browser.selected == 0)
    );
    for (width, height) in [(45, 12), (120, 30)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert!(screen_text(terminal.backend().buffer()).contains("+ New value"));
    }
    let (tx, _) = mpsc::unbounded_channel();
    let mut tasks = Tasks(vec![]);
    app.key(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        &tx,
        &mut tasks,
    );
    assert!(matches!(app.view, View::Create(_)));
    key(&mut app, KeyCode::Esc);
    app.key(
        KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
        &tx,
        &mut tasks,
    );
    assert!(matches!(app.view, View::Create(_)));
    assert!(!app.config.path.exists());
}

#[test]
fn encrypted_draft_reveal_matches_manual_toggle_and_expiry() {
    let mut app = app();
    creator(&mut app);
    app.paste("REVEALED_DRAFT".into());
    assert!(!screen(&mut app).contains("REVEALED_DRAFT"));
    let (tx, _) = mpsc::unbounded_channel();
    let mut tasks = Tasks(vec![]);
    let toggle = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL);
    app.key(toggle, &tx, &mut tasks);
    assert!(screen(&mut app).contains("REVEALED_DRAFT"));
    assert!(matches!(&app.view, View::Create(c) if c.editor.show && c.editor.shown_at.is_some()));
    app.key(toggle, &tx, &mut tasks);
    assert!(!screen(&mut app).contains("REVEALED_DRAFT"));
    assert!(matches!(&app.view, View::Create(c) if !c.editor.show && c.editor.shown_at.is_none()));
    app.key(toggle, &tx, &mut tasks);
    if let View::Create(c) = &mut app.view {
        c.editor.shown_at = Some(Instant::now() - Duration::from_secs(31));
    }
    app.expire();
    assert!(!screen(&mut app).contains("REVEALED_DRAFT"));
    assert!(
        matches!(&app.view, View::Create(c) if c.secret.as_str() == "REVEALED_DRAFT" && !c.editor.show)
    );
    assert!(!app.config.path.exists());
    assert!(tasks.0.is_empty());
    assert!(!app.preview_busy);
}

struct TestLevel;
impl crate::browser::BrowserLevel for TestLevel {
    fn title(&self) -> String {
        "Test references".into()
    }
    fn list<'a>(
        &'a self,
        _dir: &'a std::path::Path,
    ) -> futures::future::BoxFuture<'a, Result<Vec<Choice>>> {
        Box::pin(async { Ok(test_choices()) })
    }
}
fn test_choices() -> Vec<Choice> {
    ["FIRST", "SECOND"]
        .into_iter()
        .map(|label| Choice {
            label: label.into(),
            detail: String::new(),
            target: Target::Reference(format!("printf {label}_SECRET")),
        })
        .collect()
}
fn reference_browser() -> App {
    let mut app = app();
    app.view = View::Browser {
        editor: Editor::new(
            None,
            "DRAFT".into(),
            Entry {
                source: "shell".into(),
                value: "previous".into(),
            },
        ),
        browser: Browser {
            stack: vec![crate::browser::Level::new(TestLevel)],
            choices: test_choices(),
            query: String::new(),
            selected: 0,
            loading: false,
        },
    };
    app
}
fn list_query(app: &App) -> &str {
    match &app.view {
        View::Main => &app.query,
        View::Browser { browser, .. } => &browser.query,
        _ => panic!("expected a list"),
    }
}

#[test]
fn lists_type_shortcut_characters_and_backspace_only_erases_filter() {
    for mut app in [legacy_app(), reference_browser()] {
        let (tx, _) = mpsc::unbounded_channel();
        let mut tasks = Tasks(vec![]);
        for character in "jkqca?eh /".chars() {
            assert!(!app.key(
                KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE),
                &tx,
                &mut tasks,
            ));
        }
        assert_eq!(list_query(&app), "jkqca?eh /");
        for character in ['\0', '\n', '\t', '\u{1b}'] {
            key(&mut app, KeyCode::Char(character));
        }
        assert_eq!(list_query(&app), "jkqca?eh /");
        assert!(tasks.0.is_empty());
        assert!(app.loading.is_empty() && !app.busy_copy);
        assert!(app.delete.is_none() && app.browser_delete.is_none());
        key(&mut app, KeyCode::Delete);
        assert_eq!(list_query(&app), "jkqca?eh /");
        key(&mut app, KeyCode::Backspace);
        assert_eq!(list_query(&app), "jkqca?eh ");
        let quit = app.key(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            &tx,
            &mut tasks,
        );
        assert_eq!(quit, matches!(app.view, View::Main));
        if quit {
            assert_eq!(list_query(&app), "jkqca?eh ");
            app.query.clear();
        }
        assert_eq!(list_query(&app), "");
        key(&mut app, KeyCode::Backspace);
        assert!(app.delete.is_none() && app.browser_delete.is_none());
    }
}

#[test]
fn lists_paste_filters_and_discards_nul_and_control_characters() {
    for mut app in [legacy_app(), reference_browser()] {
        key(&mut app, KeyCode::End);
        app.paste("s\0e\nc\tr\u{1b}é🔑".into());
        assert_eq!(list_query(&app), "secré🔑");
        assert!(!app.error);
        match &app.view {
            View::Main => assert_eq!(app.selected, 0),
            View::Browser { browser, .. } => assert_eq!(browser.selected, 0),
            _ => unreachable!(),
        }
        app.paste(" /".into());
        assert_eq!(list_query(&app), "secré🔑 /");
        key(&mut app, KeyCode::Backspace);
        assert_eq!(list_query(&app), "secré🔑 ");
        let (tx, _) = mpsc::unbounded_channel();
        let quit = app.key(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            &tx,
            &mut Tasks(vec![]),
        );
        assert_eq!(quit, matches!(app.view, View::Main));
        assert_eq!(list_query(&app), if quit { "secré🔑 " } else { "" });
        assert!(!app.config.path.exists());
    }
}

#[test]
fn enter_edits_or_selects_the_filtered_row_without_an_extra_search_mode() {
    let mut app = legacy_app();
    app.paste("secret_2".into());
    key(&mut app, KeyCode::Enter);
    assert!(matches!(&app.view, View::Editor(e) if e.original.as_deref() == Some("SECRET_2")));
    assert_eq!(app.config.entries.len(), 2);

    let mut app = reference_browser();
    app.paste("second".into());
    key(&mut app, KeyCode::Enter);
    assert!(matches!(&app.view, View::Editor(e) if e.value.as_str() == "printf SECOND_SECRET"));
    assert!(app.revealed.is_empty());
    assert!(!app.config.path.exists());
}

#[tokio::test]
async fn browser_esc_clears_filter_then_goes_up_then_returns_to_editor() {
    let mut app = reference_browser();
    if let View::Browser { browser, .. } = &mut app.view {
        browser.stack.push(crate::browser::Level::new(TestLevel));
        browser.query = "first".into();
    }
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut tasks = Tasks(vec![]);
    let escape = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    assert!(!app.key(escape, &tx, &mut tasks));
    assert!(matches!(&app.view, View::Browser { browser, .. }
        if browser.query.is_empty() && browser.stack.len() == 2 && !browser.loading));
    assert!(tasks.0.is_empty());
    assert!(!app.key(escape, &tx, &mut tasks));
    assert!(matches!(&app.view, View::Browser { browser, .. }
        if browser.stack.len() == 1 && browser.loading));
    app.message(rx.recv().await.unwrap());
    assert!(!app.key(escape, &tx, &mut tasks));
    assert!(matches!(&app.view, View::Editor(e) if e.value.as_str() == "previous"));
    assert!(!app.key(escape, &tx, &mut tasks));
    assert!(matches!(app.view, View::Main));
    assert!(app.key(escape, &tx, &mut tasks));
    assert!(matches!(app.view, View::Main));
}

#[test]
fn f1_help_returns_to_each_view_without_losing_draft_or_filter() {
    for state in 0..4 {
        let mut app = match state {
            2 => reference_browser(),
            _ => legacy_app(),
        };
        match state {
            0 => {
                app.query = "secret".into();
                app.selected = 1;
            }
            1 => key(&mut app, KeyCode::Enter),
            2 => {
                if let View::Browser { browser, .. } = &mut app.view {
                    browser.query = "second".into();
                }
            }
            3 => {
                creator(&mut app);
                app.paste("UNSAVED_SECRET".into());
            }
            _ => unreachable!(),
        }
        key(&mut app, KeyCode::F(1));
        assert!(matches!(app.view, View::Help));
        assert!(app.help_return.is_some());
        key(&mut app, KeyCode::F(1));
        key(&mut app, KeyCode::Esc);
        assert!(app.help_return.is_none());
        match state {
            0 => {
                assert!(matches!(app.view, View::Main));
                assert_eq!(app.query, "secret");
                assert_eq!(app.selected, 1);
            }
            1 => assert!(matches!(&app.view, View::Editor(e)
                if e.original.as_deref() == Some("SECRET_1"))),
            2 => assert!(matches!(&app.view, View::Browser { editor, browser }
                if editor.value.as_str() == "previous" && browser.query == "second")),
            3 => assert!(matches!(&app.view, View::Create(c)
                if c.secret.as_str() == "UNSAVED_SECRET")),
            _ => unreachable!(),
        }
        assert!(!app.config.path.exists());
    }
}

#[test]
fn dashboard_delete_is_inert_while_selected_entry_is_loading() {
    let mut app = legacy_app();
    app.loading.insert("SECRET_1".into());
    let generation = app.generation;
    let status = app.status.clone();
    let entries = app.config.entries.clone();
    let (tx, _) = mpsc::unbounded_channel();
    let mut tasks = Tasks(vec![]);
    assert!(!app.key(
        KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL),
        &tx,
        &mut tasks,
    ));
    assert!(app.loading.contains("SECRET_1"));
    assert_eq!(app.loading.len(), 1);
    assert_eq!(app.generation, generation);
    assert_eq!(app.status, status);
    assert!(app.config.entries == entries);
    assert!(app.delete.is_none() && app.browser_delete.is_none());
    assert!(!app.delete_busy && !app.delete_remove);
    assert!(!app.config.path.exists());
}

#[test]
fn dashboard_esc_quits_after_errors_with_or_without_a_filter() {
    for query in ["", "secret"] {
        for message in [
            "Manual secret is empty",
            "No clipboard helper found (install wl-copy, xclip or xsel)",
            "SSH agent refused signing",
        ] {
            let mut app = legacy_app();
            app.query = query.into();
            app.message(Message::Copy {
                generation: app.generation,
                result: Err(anyhow::anyhow!(message)),
            });
            let (tx, _) = mpsc::unbounded_channel();
            assert!(app.key(
                KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                &tx,
                &mut Tasks(vec![]),
            ));
            assert_eq!(app.query, query);
            assert_eq!(app.config.entries.len(), 2);
            assert!(!app.config.path.exists());
        }
    }
}

#[test]
fn nested_error_details_preserve_causes_without_rendering_secrets_or_controls() {
    let mut app = legacy_app();
    app.config.entries["SECRET_1"].value = "NEVER_RENDER_THIS_SECRET".into();
    app.message(Message::Reveal {
        generation: app.generation,
        name: "SECRET_1".into(),
        result: Err(anyhow::anyhow!("agent disconnected\u{1b}")
            .context("signing failed")
            .context("fetch failed")),
    });
    let text = screen(&mut app);
    for expected in [
        "Failure details",
        "fetch failed",
        "signing failed",
        "agent disconnected",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    assert!(!text.contains("NEVER_RENDER_THIS_SECRET"));
    assert!(!text.contains('\u{1b}'));
    key(&mut app, KeyCode::PageDown);
    assert_eq!(app.error_scroll, 1);
}

#[test]
fn dashboard_esc_quits_without_clearing_filter_or_changing_configuration() {
    for query in ["", "secret"] {
        let mut app = legacy_app();
        app.query = query.into();
        let (tx, _) = mpsc::unbounded_channel();
        assert!(app.key(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            &tx,
            &mut Tasks(vec![])
        ));
        assert_eq!(app.query, query);
        assert_eq!(app.config.entries.len(), 2);
        assert!(!app.config.path.exists());
        assert!(screen(&mut app).contains("Esc  quit"));
    }
}

#[test]
fn ctrl_c_quits_every_view_and_plain_letters_never_quit() {
    for state in 0..5 {
        let mut app = reference_browser();
        match state {
            0 => app.view = View::Main,
            1 => key(&mut app, KeyCode::Esc),
            2 => {}
            3 => creator(&mut app),
            4 => key(&mut app, KeyCode::F(1)),
            _ => unreachable!(),
        }
        let (tx, _) = mpsc::unbounded_channel();
        let mut tasks = Tasks(vec![]);
        for character in ['q', 'c'] {
            assert!(!app.key(
                KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE),
                &tx,
                &mut tasks,
            ));
        }
        assert!(app.key(
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            &tx,
            &mut tasks,
        ));
    }
}

#[tokio::test]
async fn ctrl_y_copies_selected_list_rows_and_plain_c_only_filters() {
    for mut app in [legacy_app(), reference_browser()] {
        let (tx, _) = mpsc::unbounded_channel();
        let mut tasks = Tasks(vec![]);
        key(&mut app, KeyCode::Char('c'));
        assert_eq!(list_query(&app), "c");
        assert!(!app.busy_copy);
        key(&mut app, KeyCode::Backspace);
        assert!(!app.key(
            KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL),
            &tx,
            &mut tasks,
        ));
        assert!(app.busy_copy);
        assert_eq!(tasks.0.len(), 1);
        assert!(list_query(&app).is_empty());
        // Dropping Tasks cancels before invoking a real clipboard helper.
    }
}

#[tokio::test]
async fn dashboard_ctrl_r_cancels_pending_reveal_and_ignores_late_results() {
    let mut app = legacy_app();
    app.paste("secret_2".into());
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut tasks = Tasks(vec![]);
    let toggle = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL);
    app.key(toggle, &tx, &mut tasks);
    assert!(app.loading.contains("SECRET_2"));
    let stale = rx.recv().await.unwrap();
    app.key(toggle, &tx, &mut tasks);
    assert!(app.loading.is_empty());
    app.message(stale);
    assert!(app.revealed.is_empty());
    assert_eq!(app.query, "secret_2");
    app.key(toggle, &tx, &mut tasks);
    app.message(rx.recv().await.unwrap());
    assert_eq!(app.revealed["SECRET_2"].0.as_str(), "value2");
    app.revealed.get_mut("SECRET_2").unwrap().1 = Instant::now() - Duration::from_secs(31);
    app.expire();
    assert!(app.revealed.is_empty());
    assert!(matches!(app.view, View::Main));
}

#[test]
fn browser_backspace_and_plain_delete_never_delete_a_record() {
    let (mut app, path) = browser_deletion_fixture();
    let before = std::fs::read(&path).unwrap();
    key(&mut app, KeyCode::Delete);
    assert!(app.browser_delete.is_none());
    assert_eq!(list_query(&app), "fix");
    key(&mut app, KeyCode::Backspace);
    assert!(app.browser_delete.is_none());
    assert_eq!(list_query(&app), "fi");
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn browser_reveal_and_copy_ignore_levels_new_rows_and_loading_lists() {
    let (mut app, _) = browser_deletion_fixture();
    let (tx, _) = mpsc::unbounded_channel();
    let mut tasks = Tasks(vec![]);
    for state in 0..4 {
        if let View::Browser { browser, .. } = &mut app.view {
            browser.query.clear();
            browser.selected = usize::from(state == 1);
            browser.loading = state == 2;
            browser.choices[0].target = if state == 0 {
                Target::Level(crate::browser::Level::new(TestLevel))
            } else {
                Target::Reference("fixture".into())
            };
            if state == 3 {
                browser.query = "no match".into();
            }
        }
        for character in ['r', 'y'] {
            app.key(
                KeyEvent::new(KeyCode::Char(character), KeyModifiers::CONTROL),
                &tx,
                &mut tasks,
            );
            assert!(tasks.0.is_empty());
            assert!(app.loading.is_empty());
            assert!(app.revealed.is_empty());
            assert!(!app.busy_copy);
        }
    }
}

#[tokio::test]
async fn browser_ctrl_r_fetches_hides_expires_and_cancels_stale_results() {
    let mut app = reference_browser();
    app.paste("second".into());
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut tasks = Tasks(vec![]);
    let toggle = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL);
    app.key(toggle, &tx, &mut tasks);
    assert_eq!(app.loading.len(), 1);
    let message = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    app.message(message);
    assert!(app.loading.is_empty());
    assert_eq!(app.revealed.len(), 1);
    assert_eq!(
        app.revealed.values().next().unwrap().0.as_str(),
        "SECOND_SECRET"
    );
    assert!(screen(&mut app).contains("SECOND_SECRET"));
    app.key(toggle, &tx, &mut tasks);
    assert!(app.revealed.is_empty());
    assert!(!screen(&mut app).contains("SECOND_SECRET"));

    app.key(toggle, &tx, &mut tasks);
    app.message(rx.recv().await.unwrap());
    for (_, at) in app.revealed.values_mut() {
        *at = Instant::now() - Duration::from_secs(31);
    }
    app.expire();
    assert!(app.revealed.is_empty());

    app.key(toggle, &tx, &mut tasks);
    let message = rx.recv().await.unwrap();
    assert_eq!(app.loading.len(), 1);
    app.key(toggle, &tx, &mut tasks);
    assert!(app.loading.is_empty());
    app.message(message);
    assert!(app.revealed.is_empty());
    assert_eq!(list_query(&app), "second");
    assert!(matches!(&app.view, View::Browser { editor, .. }
        if editor.value.as_str() == "previous" && editor.preview.is_none()));
}

#[tokio::test]
async fn reference_editor_ctrl_r_toggles_fetched_preview_and_ctrl_p_is_inert() {
    let mut app = app();
    app.view = View::Editor(Editor::new(
        None,
        "KEY".into(),
        Entry {
            source: "shell".into(),
            value: "printf FETCHED_PREVIEW".into(),
        },
    ));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut tasks = Tasks(vec![]);
    app.key(
        KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
        &tx,
        &mut tasks,
    );
    assert!(tasks.0.is_empty());
    assert!(!app.preview_busy);
    let toggle = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL);
    app.key(toggle, &tx, &mut tasks);
    assert!(app.preview_busy);
    app.message(rx.recv().await.unwrap());
    assert!(!app.preview_busy);
    assert!(matches!(&app.view, View::Editor(e)
        if !e.show && e.preview.as_deref().map(String::as_str) == Some("FETCHED_PREVIEW")
        && e.preview_at.is_some()));
    app.key(toggle, &tx, &mut tasks);
    assert!(!app.preview_busy);
    assert!(matches!(&app.view, View::Editor(e)
        if e.preview.is_none() && e.preview_at.is_none() && !e.show));

    app.key(toggle, &tx, &mut tasks);
    let stale = rx.recv().await.unwrap();
    app.key(toggle, &tx, &mut tasks);
    assert!(!app.preview_busy);
    app.message(stale);
    assert!(matches!(&app.view, View::Editor(e) if e.preview.is_none()));
    assert!(!app.config.path.exists());
}

#[test]
fn unicode_editing() {
    let mut e = Editor::new(
        None,
        "X".into(),
        Entry {
            source: "manual".into(),
            value: "é🔑".into(),
        },
    );
    e.focus(2);
    e.delete(true);
    assert_eq!(e.value.as_str(), "é");
    e.cursor = 0;
    e.change_text("λ");
    assert_eq!(e.value.as_str(), "λé");
}
