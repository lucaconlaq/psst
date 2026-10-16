use super::editor::{Creator, Editor};
use crate::{
    browser::{self, Choice, Level, Target},
    config::{Config, Entry, MappingKind},
    operations, providers,
};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::widgets::TableState;
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};
use tokio::{sync::mpsc, task::JoinHandle};
use zeroize::Zeroizing;

pub(super) struct Tasks(pub(super) Vec<(u64, JoinHandle<()>)>);
impl Drop for Tasks {
    fn drop(&mut self) {
        for (_, t) in &self.0 {
            t.abort();
        }
    }
}
impl Tasks {
    pub(super) fn push(&mut self, generation: u64, task: JoinHandle<()>) {
        self.0.retain(|(_, t)| !t.is_finished());
        self.0.push((generation, task));
    }
    pub(super) fn cancel_stale(&mut self, generation: u64) {
        self.0.retain(|(g, t)| {
            if *g != generation {
                t.abort();
                false
            } else {
                true
            }
        });
    }
}

pub(super) struct Browser {
    pub(super) stack: Vec<Level>,
    pub(super) choices: Vec<Choice>,
    pub(super) query: String,
    pub(super) selected: usize,
    pub(super) loading: bool,
}
pub(super) struct BrowserDeletion {
    pub(super) choice: Choice,
    pub(super) pending: Box<dyn providers::PreparedDelete>,
}
pub(super) enum View {
    Main,
    Create(Creator),
    Editor(Editor),
    Browser { editor: Editor, browser: Browser },
    Help,
}
pub(super) enum Message {
    Deletion {
        generation: u64,
        name: String,
        result: Result<Box<dyn providers::PreparedDelete>>,
    },
    Identities {
        generation: u64,
        result: Result<Vec<providers::Identity>>,
    },
    Created {
        generation: u64,
        name: String,
        original: Option<String>,
        source: String,
        result: Result<String>,
    },
    Reveal {
        generation: u64,
        name: String,
        result: Result<Zeroizing<String>>,
    },
    Copy {
        generation: u64,
        result: Result<()>,
    },
    Preview {
        generation: u64,
        result: Result<Zeroizing<String>>,
    },
    Browse {
        generation: u64,
        result: Result<Vec<Choice>>,
    },
}
pub(super) struct App {
    pub(super) config: Config,
    pub(super) kind: MappingKind,
    // Park navigation only; configuration always contains both kinds.
    other_dashboard: (String, usize, TableState),
    pub(super) view: View,
    pub(super) selected: usize,
    pub(super) table: TableState,
    pub(super) query: String,
    pub(super) preview_busy: bool,
    pub(super) help_return: Option<Box<View>>,
    pub(super) delete: Option<String>,
    pub(super) browser_delete: Option<BrowserDeletion>,
    pub(super) delete_remove: bool,
    pub(super) delete_busy: bool,
    pub(super) revealed: HashMap<String, (Zeroizing<String>, Instant)>,
    pub(super) loading: HashSet<String>,
    pub(super) status: String,
    pub(super) error: bool,
    pub(super) error_panel: bool,
    pub(super) error_scroll: u16,
    pub(super) help_scroll: u16,
    pub(super) generation: u64,
    pub(super) busy_copy: bool,
}
impl App {
    pub(super) fn new(config: Config) -> Self {
        Self {
            config,
            kind: MappingKind::Env,
            other_dashboard: (String::new(), 0, TableState::default()),
            view: View::Main,
            selected: 0,
            table: TableState::default(),
            query: String::new(),
            preview_busy: false,
            help_return: None,
            delete: None,
            browser_delete: None,
            delete_remove: false,
            delete_busy: false,
            revealed: HashMap::new(),
            loading: HashSet::new(),
            status: "Secrets stay masked until you ask. Fetches run in parallel.".into(),
            error: false,
            error_panel: false,
            error_scroll: 0,
            help_scroll: 0,
            generation: 0,
            busy_copy: false,
        }
    }
    pub(super) fn names(&self) -> Vec<String> {
        let query = self.query.to_lowercase();
        self.config
            .entries
            .iter()
            .filter(|(name, entry)| entry.kind == self.kind && name.to_lowercase().contains(&query))
            .map(|(name, _)| name.clone())
            .collect()
    }
    pub(super) fn selected_name(&self) -> Option<String> {
        self.names().get(self.selected).cloned()
    }
    fn status(&mut self, message: impl Into<String>, error: bool) {
        self.status = message.into();
        self.error = error;
        self.error_panel = error;
        self.error_scroll = 0;
    }
    pub(super) fn error_prompt(&self) -> bool {
        self.error_panel
            && matches!(
                self.status.as_str(),
                "Manual secret is empty"
                    | "No clipboard helper found (install wl-copy, xclip or xsel)"
            )
    }
    fn invalidate(&mut self) {
        self.generation += 1;
        self.loading.clear();
        self.busy_copy = false;
        self.preview_busy = false;
        self.revealed.clear();
    }
    fn main(&mut self) {
        self.invalidate();
        self.view = View::Main;
        self.selected = self.selected.min(self.names().len());
    }
    fn browse(&mut self, tx: &mpsc::UnboundedSender<Message>, tasks: &mut Tasks) {
        let View::Browser { browser, .. } = &mut self.view else {
            return;
        };
        browser.loading = true;
        browser.choices.clear();
        browser.query.clear();
        browser.selected = 0;
        let level = browser.stack.last().unwrap().clone();
        self.invalidate();
        let generation = self.generation;
        let dir = self.config.dir().to_owned();
        let tx = tx.clone();
        tasks.push(
            generation,
            tokio::spawn(async move {
                let result = browser::list(&level, &dir).await;
                let _ = tx.send(Message::Browse { generation, result });
            }),
        );
    }
    fn preview(&mut self, tx: &mpsc::UnboundedSender<Message>, tasks: &mut Tasks) {
        let View::Editor(e) = &mut self.view else {
            return;
        };
        if e.preview.is_some() || self.preview_busy {
            e.preview = None;
            e.preview_at = None;
            self.invalidate();
            self.status("Value hidden.", false);
            return;
        }
        let entry = e.entry();
        self.invalidate();
        self.preview_busy = true;
        let generation = self.generation;
        let dir = self.config.dir().to_owned();
        let tx = tx.clone();
        self.status("Resolving value to reveal… Ctrl+R cancels.", false);
        tasks.push(
            generation,
            tokio::spawn(async move {
                let result = providers::fetch(&entry, &dir).await;
                let _ = tx.send(Message::Preview { generation, result });
            }),
        );
    }
    fn reveal_entry(
        &mut self,
        name: String,
        entry: Entry,
        tx: &mpsc::UnboundedSender<Message>,
        tasks: &mut Tasks,
    ) {
        if self.revealed.remove(&name).is_some() {
            self.status("Value hidden.", false);
            return;
        }
        if self.loading.contains(&name) {
            self.invalidate();
            self.status("Reveal cancelled.", false);
            return;
        }
        self.loading.insert(name.clone());
        let dir = self.config.dir().to_owned();
        let generation = self.generation;
        let tx = tx.clone();
        self.status("Resolving value to reveal… Ctrl+R cancels.", false);
        tasks.push(
            generation,
            tokio::spawn(async move {
                let result = providers::fetch(&entry, &dir).await;
                let _ = tx.send(Message::Reveal {
                    generation,
                    name,
                    result,
                });
            }),
        );
    }
    fn copy_entry(
        &mut self,
        entry: Entry,
        cached: Option<Zeroizing<String>>,
        tx: &mpsc::UnboundedSender<Message>,
        tasks: &mut Tasks,
    ) {
        if self.busy_copy {
            return;
        }
        self.busy_copy = true;
        self.status("Copying…", false);
        let dir = self.config.dir().to_owned();
        let generation = self.generation;
        let tx = tx.clone();
        tasks.push(
            generation,
            tokio::spawn(async move {
                let result = async {
                    let value = match cached {
                        Some(value) => value,
                        None => providers::fetch(&entry, &dir).await?,
                    };
                    operations::copy(&value).await
                }
                .await;
                let _ = tx.send(Message::Copy { generation, result });
            }),
        );
    }
    pub(super) fn save(&mut self) {
        let View::Editor(e) = &self.view else {
            return;
        };
        let mut entries = self.config.entries.clone();
        let valid = match e.kind {
            MappingKind::Env => crate::config::valid_name(&e.name),
            MappingKind::File => crate::config::valid_destination(&e.name),
        };
        if !valid {
            self.status(
                if e.kind == MappingKind::File {
                    "Use a nonempty literal destination without control characters."
                } else {
                    "Use an environment name: letters, digits and _, starting with a letter or _."
                },
                true,
            );
            return;
        }
        if e.original.as_deref() != Some(&e.name) && entries.contains_key(&e.name) {
            self.status("That name already exists. Choose another name.", true);
            return;
        }
        if let Some(original) = &e.original
            && original != &e.name
        {
            entries.shift_remove(original);
        }
        entries.insert(e.name.clone(), e.entry());
        match self.config.save(entries) {
            Ok(()) => {
                self.main();
                self.status(
                    if self.kind == MappingKind::File {
                        "File mapping saved. Run psst --write-files separately to write destinations."
                    } else {
                        "Saved securely."
                    },
                    false,
                );
            }
            Err(e) => self.status(format!("{e:#}"), true),
        }
    }
    pub(super) fn create(&mut self, tx: &mpsc::UnboundedSender<Message>, tasks: &mut Tasks) {
        let editor = match &self.view {
            View::Editor(editor) | View::Browser { editor, .. } => editor,
            _ => return,
        };
        let Some(author) = providers::get(&editor.source).and_then(|p| p.author()) else {
            self.status("This provider does not support encrypted creation.", true);
            return;
        };
        self.create_with_identities(author.identities(), tx, tasks);
    }
    pub(super) fn create_with_identities(
        &mut self,
        identities: impl std::future::Future<Output = Result<Vec<providers::Identity>>> + Send + 'static,
        tx: &mpsc::UnboundedSender<Message>,
        tasks: &mut Tasks,
    ) {
        let view = std::mem::replace(&mut self.view, View::Main);
        let (mut editor, return_browser) = match view {
            View::Editor(editor) => (editor, None),
            View::Browser { editor, browser } => (editor, Some(browser)),
            _ => return,
        };
        let store_only = return_browser.is_some();
        let return_value =
            store_only.then(|| std::mem::replace(&mut editor.value, Zeroizing::new(String::new())));
        editor.preview = None;
        editor.show = false;
        editor.shown_at = None;
        self.invalidate();
        self.view = View::Create(Creator {
            cursor: if store_only {
                0
            } else {
                editor.name.chars().count()
            },
            editor,
            secret: Zeroizing::new(String::new()),
            field: usize::from(store_only),
            keys: vec![],
            selected: 0,
            chosen: None,
            busy: true,
            pending: None,
            return_browser,
            return_value,
        });
        self.status("Listing eligible keys… Esc cancels.", false);
        let generation = self.generation;
        let tx = tx.clone();
        tasks.push(
            generation,
            tokio::spawn(async move {
                let result = identities.await;
                let _ = tx.send(Message::Identities { generation, result });
            }),
        );
    }
    fn prepare_creation(&mut self, tx: &mpsc::UnboundedSender<Message>, tasks: &mut Tasks) {
        let View::Create(c) = &mut self.view else {
            return;
        };
        if c.busy || c.pending.is_some() {
            return;
        }
        if c.return_browser.is_none() {
            let valid = match c.editor.kind {
                MappingKind::Env => crate::config::valid_name(&c.editor.name),
                MappingKind::File => crate::config::valid_destination(&c.editor.name),
            };
            if !valid {
                let message = match c.editor.kind {
                    MappingKind::Env => "Invalid environment name.",
                    MappingKind::File => "Invalid destination.",
                };
                self.status(message, true);
                return;
            }
        }
        if c.return_browser.is_none()
            && c.editor.original.as_deref() != Some(&c.editor.name)
            && self.config.entries.contains_key(&c.editor.name)
        {
            self.status("That name already exists. Choose another name.", true);
            return;
        }
        let Some(key) = c.chosen.and_then(|index| c.keys.get(index)) else {
            self.status(
                "Choose an SSH fingerprint explicitly (Tab to keys, Enter).",
                true,
            );
            return;
        };
        let Some(author) = providers::get(&c.editor.source).and_then(|p| p.author()) else {
            return;
        };
        match author.prepare(self.config.dir(), &c.editor.value, key) {
            Ok(pending) => {
                let replacing = pending.replacing();
                c.pending = Some(pending);
                if replacing {
                    self.status("Replace ciphertext? Updates affect ALL references to this record. Enter confirms; Esc retains draft.", false);
                } else {
                    self.save_creation(tx, tasks);
                }
            }
            Err(error) => self.status(format!("{error:#}"), true),
        }
    }
    pub(super) fn save_creation(&mut self, tx: &mpsc::UnboundedSender<Message>, tasks: &mut Tasks) {
        let View::Create(c) = &mut self.view else {
            return;
        };
        let Some(pending) = c.pending.take() else {
            return;
        };
        c.busy = true;
        c.editor.show = false;
        c.editor.shown_at = None;
        let value = c.secret.clone();
        let name = c.editor.name.clone();
        let original = c.editor.original.clone();
        let source = c.editor.source.clone();
        let generation = self.generation;
        let tx = tx.clone();
        self.status(
            "Saving ciphertext… Esc cancels unfinished work, not completed writes.",
            false,
        );
        tasks.push(
            generation,
            tokio::spawn(async move {
                let result = pending.save(value).await;
                // No await between replacement completion and reporting it.
                let _ = tx.send(Message::Created {
                    generation,
                    name,
                    original,
                    source,
                    result,
                });
            }),
        );
    }
    pub(super) fn delete_mapping(
        &mut self,
        name: &str,
        removal: Option<Result<Box<dyn providers::PreparedDelete>>>,
        config_boundary: impl FnMut(crate::config::WriteBoundary) -> Result<()>,
        mut storage_boundary: impl FnMut(crate::config::WriteBoundary) -> Result<()>,
    ) {
        let file = self.config.entries[name].kind == MappingKind::File;
        let encrypted = !file
            && providers::get(&self.config.entries[name].source)
                .is_some_and(|p| p.author().is_some());
        let removal = if file { None } else { removal };
        let mut entries = self.config.entries.clone();
        entries.shift_remove(name);
        let result = self.config.save_checked(entries, config_boundary);
        let changed = result.as_ref().err().is_some_and(|error| {
            error
                .downcast_ref::<crate::config::PersistenceError>()
                .is_some_and(|e| e.changed)
        });
        self.delete = None;
        self.delete_busy = false;
        self.delete_remove = false;
        if let Err(error) = result {
            if changed {
                self.main();
                self.status(
                    format!(
                        "Mapping removed; config durability uncertain; ciphertext kept: {error:#}"
                    ),
                    true,
                );
            } else {
                self.status(
                    format!("Configuration unchanged; ciphertext kept: {error:#}"),
                    true,
                );
            }
            // Even after replacement, keep ciphertext on uncertain config durability.
            return;
        }
        self.main();
        match removal {
            None => self.status(if file { format!("Deleted {name} mapping; destination and provider contents kept.") } else if encrypted { format!("Deleted {name}; ciphertext kept.") } else { format!("Deleted {name}.") }, false),
            Some(Err(error)) => self.status(format!("Mapping removed; ciphertext kept: {error:#}"), true),
            Some(Ok(mut selected)) => match selected.remove(&mut storage_boundary) {
                Ok(()) => self.status("Mapping and selected ciphertext removed. Other loaded references to it will fail safely.", false),
                Err(error) => {
                    let changed = error.downcast_ref::<crate::config::PersistenceError>().is_some_and(|e| e.changed);
                    self.status(format!("Mapping removed; ciphertext {}: {error:#}", if changed { "removed but storage durability uncertain" } else { "kept" }), true);
                }
            },
        }
    }
    pub(super) fn delete_browser_record(
        &mut self,
        mut boundary: impl FnMut(crate::config::WriteBoundary) -> Result<()>,
    ) {
        let Some(mut deletion) = self.browser_delete.take() else {
            return;
        };
        let result = deletion.pending.remove(&mut boundary);
        let changed = result.as_ref().err().is_some_and(|error| {
            error
                .downcast_ref::<crate::config::PersistenceError>()
                .is_some_and(|e| e.changed)
        });
        if result.is_ok() || changed {
            self.invalidate();
            if let View::Browser { editor, browser } = &mut self.view {
                editor.preview = None;
                editor.preview_at = None;
                browser.choices.retain(|choice| {
                    !matches!((&choice.target, &deletion.choice.target),
                        (Target::Reference(left), Target::Reference(right)) if left == right)
                });
                let count = browser
                    .choices
                    .iter()
                    .filter(|choice| {
                        choice
                            .label
                            .to_lowercase()
                            .contains(&browser.query.to_lowercase())
                    })
                    .count();
                browser.selected = browser.selected.min(count);
            }
        }
        match result {
            Ok(()) => self.status("Encrypted value removed; configuration unchanged. References to it will fail safely.", false),
            Err(error) => self.status(format!(
                "Ciphertext {}; configuration unchanged: {error:#}",
                if changed { "removed but storage durability uncertain" } else { "kept" }
            ), true),
        }
    }
    pub(super) fn message(&mut self, msg: Message) {
        self.message_checked(msg, |_| Ok(()));
    }
    pub(super) fn message_checked(
        &mut self,
        msg: Message,
        boundary: impl FnMut(crate::config::WriteBoundary) -> Result<()>,
    ) {
        match msg {
            Message::Deletion {
                generation,
                name,
                result,
            } if generation == self.generation
                && self.delete_busy
                && self.delete.as_deref() == Some(&name) =>
            {
                self.delete_mapping(&name, Some(result), boundary, |_| Ok(()));
            }
            Message::Identities { generation, result } if generation == self.generation => {
                if let View::Create(c) = &mut self.view {
                    c.busy = false;
                    match result {
                        Ok(keys) if !keys.is_empty() => {
                            c.chosen = (keys.len() == 1).then_some(0);
                            c.keys = keys;
                            self.status("Masked exact text · Tab fields · Enter inserts newline in value · Ctrl+S saves.", false);
                        }
                        Ok(_) => {
                            self.status("No eligible Ed25519 SSH agent keys. Esc cancels.", true);
                        }
                        Err(error) => self.status(format!("{error:#}"), true),
                    }
                }
            }
            Message::Created {
                generation,
                name,
                original,
                source,
                result,
            } => {
                let current = generation == self.generation && matches!(self.view, View::Create(_));
                if current && let View::Create(c) = &mut self.view {
                    c.busy = false;
                }
                match result {
                    Ok(reference) if current => {
                        if let View::Create(c) = &mut self.view
                            && let Some(mut browser) = c.return_browser.take()
                        {
                            let view = std::mem::replace(&mut self.view, View::Main);
                            if let View::Create(mut c) = view {
                                let fingerprint = c.chosen
                                    .and_then(|i| c.keys.get(i))
                                    .map(|key| key.fingerprint.clone())
                                    .unwrap_or_default();
                                let reference = if !reference.contains('#') && !fingerprint.is_empty() {
                                    format!("{reference}#{fingerprint}")
                                } else {
                                    reference
                                };
                                browser.choices.retain(|choice| {
                                    !matches!(&choice.target,
                                        Target::Reference(value) if value == &reference)
                                });
                                browser.choices.push(Choice {
                                    label: c.editor.value.split('#')
                                        .next().unwrap_or_default().to_owned(),
                                    detail: fingerprint,
                                    target: Target::Reference(reference),
                                });
                                browser.query.clear();
                                browser.selected = browser.choices.len() - 1;
                                self.invalidate();
                                if let Some(value) = c.return_value.take() {
                                    c.editor.value = value;
                                }
                                self.view = View::Browser { editor: c.editor, browser };
                                self.status(
                                    "Value stored; configuration unchanged. Enter links the reference.",
                                    false,
                                );
                            }
                            return;
                        }
                        let mut entries = self.config.entries.clone();
                        if let Some(original) = original && original != name { entries.shift_remove(&original); }
                        entries.insert(name, Entry { kind: self.kind, source, value: reference });
                        match self.config.save_checked(entries, boundary) {
                            Ok(()) => { self.main(); self.status("Saved encrypted value and configuration.", false); }
                            Err(error) => {
                                let changed = error.downcast_ref::<crate::config::PersistenceError>().is_some_and(|e| e.changed);
                                self.status(if changed {
                                    format!("Ciphertext and configuration replaced; config durability uncertain: {error:#}")
                                } else {
                                    format!("Ciphertext saved; configuration unchanged; existing references may now resolve the updated value; unused ciphertext may remain: {error:#}")
                                }, true);
                            }
                        }
                    }
                    Ok(_) => self.status("Cancelled draft: ciphertext was saved; configuration unchanged. Completed writes were not undone.", true),
                    Err(error) => {
                        let changed = error.downcast_ref::<crate::config::PersistenceError>().is_some_and(|e| e.changed);
                        self.status(if changed {
                            format!("Ciphertext replaced but durability uncertain; configuration unchanged; existing references may now resolve the updated value: {error:#}")
                        } else { format!("Ciphertext unchanged; configuration unchanged: {error:#}") }, true);
                    }
                }
            }
            Message::Reveal {
                generation,
                name,
                result,
            } if generation == self.generation => {
                self.loading.remove(&name);
                match result {
                    Ok(v) => {
                        self.revealed.insert(name, (v, Instant::now()));
                        self.status("Revealed for 30 seconds. Ctrl+R hides it now.", false);
                    }
                    Err(e) => self.status(format!("{e:#}"), true),
                }
            }
            Message::Copy { generation, result } if generation == self.generation => {
                self.busy_copy = false;
                match result {
                    Ok(()) => self.status(
                        "Copied. Your desktop owns the clipboard; psst does not clear it.",
                        false,
                    ),
                    Err(e) => self.status(format!("{e:#}"), true),
                }
            }
            Message::Preview { generation, result } if generation == self.generation => {
                self.preview_busy = false;
                match result {
                    Ok(value) => {
                        if let View::Editor(e) = &mut self.view {
                            e.preview = Some(value);
                            e.preview_at = Some(Instant::now());
                        }
                        self.status(
                            "Value revealed (auto-hides after 30 seconds). Ctrl+R hides it.",
                            false,
                        );
                    }
                    Err(e) => self.status(format!("{e:#}"), true),
                }
            }
            Message::Browse { generation, result } if generation == self.generation => {
                if let View::Browser { browser, .. } = &mut self.view {
                    browser.loading = false;
                    match result {
                        Ok(choices) => {
                            browser.choices = choices;
                            self.status("Type filters · Enter selects · Esc goes back", false);
                        }
                        Err(e) => self.status(format!("{e:#}"), true),
                    }
                }
            }
            _ => {}
        }
    }
    pub(super) fn key(
        &mut self,
        key: KeyEvent,
        tx: &mpsc::UnboundedSender<Message>,
        tasks: &mut Tasks,
    ) -> bool {
        if key.kind == KeyEventKind::Release {
            return false;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return true;
        }
        if key.code == KeyCode::Esc && matches!(self.view, View::Main) && self.delete.is_none() {
            return true;
        }
        if self.error_prompt() && key.code != KeyCode::Esc {
            return false;
        }
        if self.error_panel {
            match key.code {
                KeyCode::PageDown => {
                    self.error_scroll = self.error_scroll.saturating_add(1);
                    return false;
                }
                KeyCode::PageUp => {
                    self.error_scroll = self.error_scroll.saturating_sub(1);
                    return false;
                }
                _ => {
                    self.error_panel = false;
                    // Dismissing an asynchronous failure must not also consent
                    // to a destructive dialog that was hidden underneath it.
                    if self.delete.is_some()
                        || self.browser_delete.is_some()
                        || matches!(&self.view, View::Create(c) if c.pending.is_some())
                    {
                        return false;
                    }
                }
            }
        }
        if matches!(self.view, View::Help) {
            match key.code {
                KeyCode::Up => {
                    self.help_scroll = self.help_scroll.saturating_sub(1);
                }
                KeyCode::Down => {
                    self.help_scroll = self.help_scroll.saturating_add(1);
                }
                KeyCode::Esc => {
                    self.view = self
                        .help_return
                        .take()
                        .map(|view| *view)
                        .unwrap_or(View::Main);
                    self.status("Returned from help.", false);
                }
                _ => {}
            }
            return false;
        }
        if key.code == KeyCode::F(1) && self.delete.is_none() && self.browser_delete.is_none() {
            if self.preview_busy
                || self.busy_copy
                || !self.loading.is_empty()
                || matches!(&self.view, View::Create(c) if c.busy || c.pending.is_some())
                || matches!(&self.view, View::Browser { browser, .. } if browser.loading)
            {
                self.status(
                    "Finish or cancel the current operation before opening help.",
                    false,
                );
                return false;
            }
            let previous = std::mem::replace(&mut self.view, View::Help);
            self.help_return = Some(Box::new(previous));
            self.help_scroll = 0;
            self.status("F1 help · Esc returns to your previous screen.", false);
            return false;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && (self.delete.is_some()
                || self.browser_delete.is_some()
                || matches!(&self.view, View::Create(c) if c.pending.is_some()))
        {
            return false;
        }
        if let Some(name) = self.delete.clone() {
            let entry = &self.config.entries[&name];
            let author = (entry.kind == MappingKind::Env)
                .then(|| providers::get(&entry.source).and_then(|p| p.author()))
                .flatten();
            match key.code {
                KeyCode::Tab if author.is_some() && !self.delete_busy => {
                    self.delete_remove = !self.delete_remove;
                }
                KeyCode::Char('y') | KeyCode::Enter if !self.delete_busy => {
                    if self.delete_remove
                        && let Some(author) = author
                    {
                        let work = author.prepare_deletion(self.config.dir(), &entry.value);
                        self.invalidate();
                        self.delete_busy = true;
                        let generation = self.generation;
                        let tx = tx.clone();
                        self.status(
                            "Selecting ciphertext identity… Esc cancels before persistence.",
                            false,
                        );
                        tasks.push(
                            generation,
                            tokio::spawn(async move {
                                let result = work.await;
                                let _ = tx.send(Message::Deletion {
                                    generation,
                                    name,
                                    result,
                                });
                            }),
                        );
                    } else {
                        self.delete_mapping(&name, None, |_| Ok(()), |_| Ok(()));
                    }
                }
                KeyCode::Char('n') | KeyCode::Esc => {
                    self.delete = None;
                    self.delete_remove = false;
                    self.delete_busy = false;
                    self.invalidate();
                    self.status(
                        "Deletion cancelled; configuration and ciphertext unchanged.",
                        false,
                    );
                }
                _ => {}
            }
            return false;
        }
        if self.browser_delete.is_some() {
            match key.code {
                KeyCode::Enter | KeyCode::Char('y') => self.delete_browser_record(|_| Ok(())),
                KeyCode::Esc | KeyCode::Char('n') => {
                    self.browser_delete = None;
                    self.status(
                        "Deletion cancelled; configuration and ciphertext unchanged.",
                        false,
                    );
                }
                _ => {}
            }
            return false;
        }
        if matches!(self.view, View::Browser { .. }) {
            if key.code == KeyCode::Char('d') && key.modifiers.contains(KeyModifiers::CONTROL) {
                if let View::Browser { editor, browser } = &self.view
                    && !browser.loading
                    && let Some(author) = providers::get(&editor.source).and_then(|p| p.author())
                    && let Some(choice) = browser
                        .choices
                        .iter()
                        .filter(|choice| {
                            choice
                                .label
                                .to_lowercase()
                                .contains(&browser.query.to_lowercase())
                        })
                        .nth(browser.selected)
                        .cloned()
                    && let Target::Reference(reference) = &choice.target
                {
                    match author.prepare_browser_deletion(self.config.dir(), reference) {
                        Ok(pending) => {
                            self.invalidate();
                            self.browser_delete = Some(BrowserDeletion { choice, pending });
                            self.status(
                                "Delete encrypted value? Configuration mappings will be kept.",
                                false,
                            );
                        }
                        Err(error) => self.status(format!("{error:#}"), true),
                    }
                }
                return false;
            }
            if key.code == KeyCode::Esc {
                if let View::Browser { browser, .. } = &mut self.view {
                    if !browser.query.is_empty() {
                        browser.query.clear();
                        browser.selected = 0;
                        return false;
                    }
                    if browser.stack.len() > 1 {
                        browser.stack.pop();
                        self.browse(tx, tasks);
                        return false;
                    }
                }
                let view = std::mem::replace(&mut self.view, View::Main);
                self.invalidate();
                if let View::Browser { editor, .. } = view {
                    self.view = View::Editor(editor);
                }
                return false;
            }
            if key.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(key.code, KeyCode::Char('r' | 'y'))
            {
                let selected = if let View::Browser { editor, browser } = &self.view {
                    (!browser.loading)
                        .then(|| {
                            browser
                                .choices
                                .iter()
                                .filter(|choice| {
                                    choice
                                        .label
                                        .to_lowercase()
                                        .contains(&browser.query.to_lowercase())
                                })
                                .nth(browser.selected)
                        })
                        .flatten()
                        .and_then(|choice| {
                            if let Target::Reference(reference) = &choice.target {
                                Some((
                                    reference.clone(),
                                    Entry {
                                        kind: crate::config::MappingKind::Env,
                                        source: editor.source.clone(),
                                        value: reference.clone(),
                                    },
                                ))
                            } else {
                                None
                            }
                        })
                } else {
                    None
                };
                if let Some((reference, entry)) = selected {
                    if key.code == KeyCode::Char('r') {
                        self.reveal_entry(reference, entry, tx, tasks);
                    } else {
                        let cached = self
                            .revealed
                            .get(&reference)
                            .map(|(value, _)| value.clone());
                        self.copy_entry(entry, cached, tx, tasks);
                    }
                }
                return false;
            }
            if key.code == KeyCode::Char('n')
                && key.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(&self.view, View::Browser { editor, browser } if !browser.loading && providers::get(&editor.source).is_some_and(|p| p.author().is_some()))
            {
                self.create(tx, tasks);
                return false;
            }
            let can_create = matches!(&self.view, View::Browser { editor, .. }
                if providers::get(&editor.source).is_some_and(|p| p.author().is_some()));
            let View::Browser { browser, .. } = &mut self.view else {
                unreachable!()
            };
            let filtered = browser
                .choices
                .iter()
                .filter(|c| {
                    c.label
                        .to_lowercase()
                        .contains(&browser.query.to_lowercase())
                })
                .collect::<Vec<_>>();
            match key.code {
                KeyCode::Up => browser.selected = browser.selected.saturating_sub(1),
                KeyCode::Down => {
                    let last = if can_create {
                        filtered.len()
                    } else {
                        filtered.len().saturating_sub(1)
                    };
                    browser.selected = (browser.selected + 1).min(last)
                }
                KeyCode::Enter if !browser.loading => {
                    if can_create && browser.selected == filtered.len() {
                        self.create(tx, tasks);
                        return false;
                    }
                    let target = filtered.get(browser.selected).map(|c| c.target.clone());
                    match target {
                        Some(Target::Level(level)) => {
                            browser.stack.push(level);
                            self.browse(tx, tasks);
                        }
                        Some(Target::Reference(reference)) => {
                            let view = std::mem::replace(&mut self.view, View::Main);
                            if let View::Browser { mut editor, .. } = view {
                                editor.value = Zeroizing::new(reference);
                                editor.preview = None;
                                editor.focus(2);
                                self.invalidate();
                                self.view = View::Editor(editor);
                                self.status(
                                    "Reference selected. Ctrl+S saves; Esc discards.",
                                    false,
                                );
                            }
                        }
                        _ => {}
                    }
                }
                KeyCode::Backspace => {
                    browser.query.pop();
                    browser.selected = 0;
                }
                KeyCode::Char(c)
                    if !key.modifiers.contains(KeyModifiers::CONTROL) && !c.is_control() =>
                {
                    browser.query.push(c);
                    browser.selected = 0;
                }
                _ => {}
            }
            return false;
        }
        if matches!(self.view, View::Create(_)) {
            if let View::Create(c) = &mut self.view
                && c.pending.is_some()
            {
                match key.code {
                    KeyCode::Enter | KeyCode::Char('y') => self.save_creation(tx, tasks),
                    KeyCode::Esc | KeyCode::Char('n') => {
                        c.pending = None;
                        self.status("Replacement cancelled; draft retained.", false);
                    }
                    _ => {}
                }
                return false;
            }
            if key.code == KeyCode::Esc {
                let view = std::mem::replace(&mut self.view, View::Main);
                self.invalidate();
                if let View::Create(mut c) = view
                    && let Some(browser) = c.return_browser.take()
                {
                    if let Some(value) = c.return_value.take() {
                        c.editor.value = value;
                    }
                    c.editor.show = false;
                    c.editor.shown_at = None;
                    self.view = View::Browser {
                        editor: c.editor,
                        browser,
                    };
                }
                self.status("Draft cancelled. Completed writes are not undone; pending completion will be reported.", false);
                return false;
            }
            let View::Create(c) = &mut self.view else {
                return false;
            };
            if c.busy {
                return false;
            }
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                if key.code == KeyCode::Char('s') {
                    self.prepare_creation(tx, tasks);
                } else if key.code == KeyCode::Char('r') {
                    c.editor.toggle_reveal();
                } else if key.code == KeyCode::Char('y') {
                    let entry = c.editor.entry();
                    let value = c.secret.clone();
                    self.copy_entry(entry, Some(value), tx, tasks);
                }
                return false;
            }
            match key.code {
                KeyCode::Tab => c.focus(if c.return_browser.is_some() && c.field == 3 {
                    1
                } else {
                    (c.field + 1) % 4
                }),
                KeyCode::BackTab => c.focus(if c.return_browser.is_some() && c.field == 1 {
                    3
                } else {
                    (c.field + 3) % 4
                }),
                KeyCode::Up if c.field == 3 => c.selected = c.selected.saturating_sub(1),
                KeyCode::Down if c.field == 3 => {
                    c.selected = (c.selected + 1).min(c.keys.len().saturating_sub(1))
                }
                KeyCode::Enter if c.field == 3 && !c.keys.is_empty() => c.chosen = Some(c.selected),
                KeyCode::Enter if c.field == 2 => {
                    let limit = providers::get(&c.editor.source)
                        .and_then(|p| p.author())
                        .map(|a| a.value_limit())
                        .unwrap_or(0);
                    if c.secret.len() < limit {
                        c.insert("\n");
                    } else {
                        self.status("Value exceeds byte limit; draft unchanged.", true);
                    }
                }
                KeyCode::Left => c.cursor = c.cursor.saturating_sub(1),
                KeyCode::Right => c.cursor = (c.cursor + 1).min(c.text().chars().count()),
                KeyCode::Home => c.cursor = 0,
                KeyCode::End => c.cursor = c.text().chars().count(),
                KeyCode::Backspace => c.delete(true),
                KeyCode::Delete => c.delete(false),
                KeyCode::Char('\0') => self.status("NUL rejected; draft unchanged.", true),
                KeyCode::Char(ch) => {
                    let limit = providers::get(&c.editor.source)
                        .and_then(|p| p.author())
                        .map(|a| a.value_limit())
                        .unwrap_or(0);
                    if c.field != 2 || c.secret.len() + ch.len_utf8() <= limit {
                        c.insert(&Zeroizing::new(ch.to_string()));
                    } else {
                        self.status("Value exceeds byte limit; draft unchanged.", true);
                    }
                }
                _ => {}
            }
            return false;
        }
        if matches!(self.view, View::Editor(_)) {
            if key.code == KeyCode::Esc {
                self.main();
                self.status("Changes discarded.", false);
                return false;
            }
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('s') => self.save(),
                    KeyCode::Char('r') => {
                        if matches!(&self.view, View::Editor(e)
                            if providers::get(&e.source).is_some_and(|p| p.sensitive_reference()))
                        {
                            if let View::Editor(e) = &mut self.view {
                                e.toggle_reveal();
                            }
                        } else {
                            self.preview(tx, tasks);
                        }
                    }
                    KeyCode::Char('y') => {
                        if let View::Editor(e) = &self.view {
                            let entry = e.entry();
                            let cached = if providers::get(&e.source)
                                .is_some_and(|p| p.sensitive_reference())
                            {
                                Some(e.value.clone())
                            } else {
                                e.preview.clone()
                            };
                            self.copy_entry(entry, cached, tx, tasks);
                        }
                    }
                    KeyCode::Char('b') => {
                        let View::Editor(e) = &self.view else {
                            return false;
                        };
                        let level =
                            providers::get(&e.source).and_then(|provider| provider.browser());
                        if let Some(level) = level {
                            let view = std::mem::replace(&mut self.view, View::Main);
                            if let View::Editor(editor) = view {
                                self.view = View::Browser {
                                    editor,
                                    browser: Browser {
                                        stack: vec![level],
                                        choices: vec![],
                                        query: String::new(),
                                        selected: 0,
                                        loading: true,
                                    },
                                };
                                self.browse(tx, tasks);
                            }
                        } else {
                            self.status(
                                "This provider uses a text reference; no browser needed.",
                                false,
                            );
                        }
                    }
                    _ => {}
                }
                return false;
            }
            let View::Editor(e) = &mut self.view else {
                return false;
            };
            match key.code {
                KeyCode::Tab | KeyCode::Down | KeyCode::Enter => e.focus((e.field + 1) % 3),
                KeyCode::BackTab | KeyCode::Up => e.focus((e.field + 2) % 3),
                KeyCode::Left | KeyCode::Right if e.field == 1 => {
                    let sources = providers::sources();
                    let current = sources.iter().position(|s| *s == e.source).unwrap_or(0);
                    let next = if key.code == KeyCode::Right {
                        (current + 1) % sources.len()
                    } else {
                        (current + sources.len() - 1) % sources.len()
                    };
                    e.source = sources[next].into();
                    e.preview = None;
                    e.preview_at = None;
                    self.invalidate();
                }
                KeyCode::Left => e.cursor = e.cursor.saturating_sub(1),
                KeyCode::Right => e.cursor = (e.cursor + 1).min(e.len()),
                KeyCode::Home => e.cursor = 0,
                KeyCode::End => e.cursor = e.len(),
                KeyCode::Backspace => {
                    e.delete(true);
                    self.invalidate();
                }
                KeyCode::Delete => {
                    e.delete(false);
                    self.invalidate();
                }
                KeyCode::Char(c) => {
                    e.change_text(&c.to_string());
                    self.invalidate();
                }
                _ => {}
            }
            return false;
        }
        if key.code == KeyCode::Char('n') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.add_entry();
            return false;
        }
        match key.code {
            KeyCode::Esc => return true,
            KeyCode::Tab => {
                self.invalidate();
                self.kind = match self.kind {
                    MappingKind::Env => MappingKind::File,
                    MappingKind::File => MappingKind::Env,
                };
                std::mem::swap(&mut self.query, &mut self.other_dashboard.0);
                std::mem::swap(&mut self.selected, &mut self.other_dashboard.1);
                std::mem::swap(&mut self.table, &mut self.other_dashboard.2);
                tasks.cancel_stale(self.generation);
                self.status(
                    "Contents stay masked; reveal or copy resolves on request.",
                    false,
                );
            }
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => self.selected = (self.selected + 1).min(self.names().len()),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = self.names().len(),
            KeyCode::PageDown => self.selected = (self.selected + 10).min(self.names().len()),
            KeyCode::PageUp => self.selected = self.selected.saturating_sub(10),
            KeyCode::Enter => {
                if key.code == KeyCode::Enter && self.selected == self.names().len() {
                    self.add_entry();
                    return false;
                }
                if let Some(name) = self.selected_name() {
                    let entry = self.config.entries[&name].clone();
                    self.invalidate();
                    self.view = View::Editor(Editor::new(Some(name.clone()), name, entry));
                    self.status(
                        "Ctrl+R reveals on demand; shell references execute only when requested.",
                        false,
                    );
                }
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.selected = 0;
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(name) = self.selected_name()
                    && !self.loading.contains(&name)
                {
                    self.invalidate();
                    self.delete = Some(name);
                    self.delete_remove = false;
                    self.delete_busy = false;
                }
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(name) = self.selected_name() {
                    let entry = self.config.entries[&name].clone();
                    self.reveal_entry(name, entry, tx, tasks);
                }
            }
            KeyCode::Char('y') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(name) = self.selected_name() {
                    let cached = self.revealed.get(&name).map(|(value, _)| value.clone());
                    let entry = self.config.entries[&name].clone();
                    self.copy_entry(entry, cached, tx, tasks);
                }
            }
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL) && !c.is_control() =>
            {
                self.query.push(c);
                self.selected = 0;
            }
            _ => {}
        }
        false
    }
    fn add_entry(&mut self) {
        let next = self
            .config
            .entries
            .keys()
            .filter_map(|k| k.strip_prefix("SECRET_")?.parse::<u64>().ok())
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.invalidate();
        self.view = View::Editor(Editor::new(
            None,
            if self.kind == MappingKind::File {
                String::new()
            } else {
                format!("SECRET_{next}")
            },
            Entry {
                kind: self.kind,
                source: "manual".into(),
                value: String::new(),
            },
        ));
        self.status(
            if self.kind == MappingKind::File {
                "New file mapping · Ctrl+S saves configuration only, not destination contents."
            } else {
                "New entry · nothing is saved until Ctrl+S."
            },
            false,
        );
    }
    pub(super) fn paste(&mut self, text: String) {
        let text = Zeroizing::new(text);
        if self.error_prompt() {
            return;
        }
        if let View::Create(c) = &mut self.view {
            if c.busy || c.pending.is_some() {
                return;
            }
            let limit = providers::get(&c.editor.source)
                .and_then(|p| p.author())
                .map(|a| a.value_limit())
                .unwrap_or(0);
            if text.contains('\0') {
                self.status("NUL rejected; draft unchanged.", true);
            } else if c.field == 2 && c.secret.len() + text.len() > limit {
                self.status("Value exceeds byte limit; draft unchanged.", true);
            } else {
                c.insert(&text);
            }
        } else if let View::Editor(e) = &mut self.view {
            e.change_text(&text.replace('\0', ""));
            self.invalidate();
        } else if let View::Browser { browser, .. } = &mut self.view {
            if !browser.loading && self.browser_delete.is_none() {
                browser
                    .query
                    .extend(text.chars().filter(|c| !c.is_control()));
                browser.selected = 0;
            }
        } else if matches!(self.view, View::Main) && self.delete.is_none() {
            self.query.extend(text.chars().filter(|c| !c.is_control()));
            self.selected = 0;
        }
    }
    pub(super) fn expire(&mut self) {
        self.revealed
            .retain(|_, (_, time)| time.elapsed() < Duration::from_secs(30));
        for view in std::iter::once(&mut self.view).chain(self.help_return.as_deref_mut()) {
            match view {
                View::Editor(editor) | View::Browser { editor, .. } => editor.expire(),
                View::Create(c) => c.editor.expire(),
                _ => {}
            }
        }
    }
}
