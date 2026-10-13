use crate::{
    browser::{self, Choice, Level, Target},
    config::{Config, Entry},
    operations, providers,
};
use anyhow::{Result, bail};
use crossterm::{
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers,
    },
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use ratatui::{prelude::*, widgets::*};
use std::{
    collections::{HashMap, HashSet},
    io::{self, IsTerminal},
    time::{Duration, Instant},
};
use tokio::{sync::mpsc, task::JoinHandle};
use zeroize::Zeroizing;

const ACCENT: Color = Color::Rgb(110, 210, 195);
const MUTED: Color = Color::Rgb(135, 148, 166);
const BG: Color = Color::Rgb(19, 25, 35);
const PANEL: Color = Color::Rgb(28, 37, 50);
const MASK: &str = "••••••••";

struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            DisableBracketedPaste,
            LeaveAlternateScreen,
            crossterm::cursor::Show
        );
    }
}
struct Tasks(Vec<(u64, JoinHandle<()>)>);
impl Drop for Tasks {
    fn drop(&mut self) {
        for (_, t) in &self.0 {
            t.abort();
        }
    }
}
impl Tasks {
    fn push(&mut self, generation: u64, task: JoinHandle<()>) {
        self.0.retain(|(_, t)| !t.is_finished());
        self.0.push((generation, task));
    }
    fn cancel_stale(&mut self, generation: u64) {
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

struct Editor {
    original: Option<String>,
    name: String,
    source: String,
    value: Zeroizing<String>,
    field: usize,
    cursor: usize,
    show: bool,
    shown_at: Option<Instant>,
    preview: Option<Zeroizing<String>>,
    preview_at: Option<Instant>,
}
impl Editor {
    fn new(original: Option<String>, name: String, entry: Entry) -> Self {
        let cursor = name.chars().count();
        Self {
            original,
            name,
            source: entry.source,
            value: Zeroizing::new(entry.value),
            field: 0,
            cursor,
            show: false,
            shown_at: None,
            preview: None,
            preview_at: None,
        }
    }
    fn expire(&mut self) {
        if self
            .shown_at
            .is_some_and(|t| t.elapsed() >= Duration::from_secs(30))
        {
            self.show = false;
            self.shown_at = None;
        }
        if self
            .preview_at
            .is_some_and(|t| t.elapsed() >= Duration::from_secs(30))
        {
            self.preview = None;
            self.preview_at = None;
        }
    }
    fn entry(&self) -> Entry {
        Entry {
            source: self.source.clone(),
            value: self.value.to_string(),
        }
    }
    fn len(&self) -> usize {
        match self.field {
            0 => self.name.chars().count(),
            2 => self.value.chars().count(),
            _ => 0,
        }
    }
    fn focus(&mut self, field: usize) {
        self.field = field;
        self.cursor = self.len();
    }
    fn change_text(&mut self, input: &str) {
        let s: &mut String = match self.field {
            0 => &mut self.name,
            2 => &mut self.value,
            _ => return,
        };
        let pos = byte_at(s, self.cursor);
        s.insert_str(pos, input);
        self.cursor += input.chars().count();
        self.preview = None;
    }
    fn delete(&mut self, back: bool) {
        let s: &mut String = match self.field {
            0 => &mut self.name,
            2 => &mut self.value,
            _ => return,
        };
        let start = if back {
            self.cursor.saturating_sub(1)
        } else {
            self.cursor
        };
        if back && self.cursor == 0 {
            return;
        }
        let end = start + 1;
        if start < s.chars().count() {
            s.replace_range(byte_at(s, start)..byte_at(s, end), "");
            self.cursor = start;
            self.preview = None;
        }
    }
}
fn byte_at(s: &str, char_index: usize) -> usize {
    s.char_indices()
        .nth(char_index)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}
struct Browser {
    stack: Vec<Level>,
    choices: Vec<Choice>,
    query: String,
    selected: usize,
    loading: bool,
}
struct Creator {
    editor: Editor,
    secret: Zeroizing<String>,
    field: usize,
    cursor: usize,
    keys: Vec<providers::Identity>,
    selected: usize,
    chosen: Option<usize>,
    busy: bool,
    pending: Option<Box<dyn providers::PreparedSave>>,
}
impl Creator {
    fn text(&self) -> &str {
        match self.field {
            0 => &self.editor.name,
            1 => &self.editor.value,
            2 => &self.secret,
            _ => "",
        }
    }
    fn text_mut(&mut self) -> Option<&mut String> {
        match self.field {
            0 => Some(&mut self.editor.name),
            1 => Some(&mut self.editor.value),
            2 => Some(&mut self.secret),
            _ => None,
        }
    }
    fn focus(&mut self, field: usize) {
        self.field = field;
        self.cursor = self.text().chars().count();
    }
    fn insert(&mut self, text: &str) {
        let cursor = self.cursor;
        if let Some(value) = self.text_mut() {
            // Allocate a zeroizing replacement so discarded secret allocations
            // are wiped rather than freed by String growth.
            let mut next = Zeroizing::new(String::with_capacity(value.len() + text.len()));
            let pos = byte_at(value, cursor);
            next.push_str(&value[..pos]);
            next.push_str(text);
            next.push_str(&value[pos..]);
            zeroize::Zeroize::zeroize(value);
            std::mem::swap(value, &mut next);
            self.cursor += text.chars().count();
        }
    }
    fn delete(&mut self, back: bool) {
        let cursor = self.cursor;
        if back && cursor == 0 {
            return;
        }
        let start = if back { cursor - 1 } else { cursor };
        if let Some(value) = self.text_mut()
            && start < value.chars().count()
        {
            let mut next = Zeroizing::new(String::with_capacity(value.len()));
            next.push_str(&value[..byte_at(value, start)]);
            next.push_str(&value[byte_at(value, start + 1)..]);
            zeroize::Zeroize::zeroize(value);
            std::mem::swap(value, &mut next);
            self.cursor = start;
        }
    }
}

enum View {
    Main,
    Create(Creator),
    Editor(Editor),
    Browser { editor: Editor, browser: Browser },
    Help,
}
enum Message {
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
        reused: bool,
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
struct App {
    config: Config,
    view: View,
    selected: usize,
    table: TableState,
    query: String,
    searching: bool,
    delete: Option<String>,
    delete_remove: bool,
    delete_busy: bool,
    revealed: HashMap<String, (Zeroizing<String>, Instant)>,
    loading: HashSet<String>,
    status: String,
    error: bool,
    error_panel: bool,
    error_scroll: u16,
    generation: u64,
    busy_copy: bool,
}
impl App {
    fn new(config: Config) -> Self {
        Self {
            config,
            view: View::Main,
            selected: 0,
            table: TableState::default(),
            query: String::new(),
            searching: false,
            delete: None,
            delete_remove: false,
            delete_busy: false,
            revealed: HashMap::new(),
            loading: HashSet::new(),
            status: "Secrets stay masked until you ask. Fetches run in parallel.".into(),
            error: false,
            error_panel: false,
            error_scroll: 0,
            generation: 0,
            busy_copy: false,
        }
    }
    fn names(&self) -> Vec<String> {
        let query = self.query.to_lowercase();
        self.config
            .entries
            .keys()
            .filter(|n| n.to_lowercase().contains(&query))
            .cloned()
            .collect()
    }
    fn selected_name(&self) -> Option<String> {
        self.names().get(self.selected).cloned()
    }
    fn status(&mut self, message: impl Into<String>, error: bool) {
        self.status = message.into();
        self.error = error;
        self.error_panel = error;
        self.error_scroll = 0;
    }
    fn invalidate(&mut self) {
        self.generation += 1;
        self.loading.clear();
        self.busy_copy = false;
        self.revealed.clear();
    }
    fn main(&mut self) {
        self.invalidate();
        self.view = View::Main;
        self.selected = self.selected.min(self.names().len().saturating_sub(1));
    }
    fn browse(&mut self, tx: &mpsc::UnboundedSender<Message>, tasks: &mut Tasks) {
        let View::Browser { browser, .. } = &mut self.view else {
            return;
        };
        browser.loading = true;
        browser.choices.clear();
        browser.query.clear();
        browser.selected = 0;
        self.generation += 1;
        let generation = self.generation;
        let level = browser.stack.last().unwrap().clone();
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
        e.preview = None;
        self.generation += 1;
        let generation = self.generation;
        let entry = e.entry();
        let dir = self.config.dir().to_owned();
        let tx = tx.clone();
        self.status("Fetching preview…", false);
        tasks.push(
            generation,
            tokio::spawn(async move {
                let result = providers::fetch(&entry, &dir).await;
                let _ = tx.send(Message::Preview { generation, result });
            }),
        );
    }
    fn save(&mut self) {
        let View::Editor(e) = &self.view else {
            return;
        };
        let mut entries = self.config.entries.clone();
        if !crate::config::valid_name(&e.name) {
            self.status(
                "Use an environment name: letters, digits and _, starting with a letter or _.",
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
                self.status("Saved securely.", false);
            }
            Err(e) => self.status(e.to_string(), true),
        }
    }
    fn create(&mut self, tx: &mpsc::UnboundedSender<Message>, tasks: &mut Tasks) {
        let View::Editor(editor) = &self.view else {
            return;
        };
        let Some(author) = providers::get(&editor.source).and_then(|p| p.author()) else {
            self.status("This provider does not support encrypted creation.", true);
            return;
        };
        let view = std::mem::replace(&mut self.view, View::Main);
        let View::Editor(mut editor) = view else {
            return;
        };
        editor.preview = None;
        editor.show = false;
        self.invalidate();
        self.view = View::Create(Creator {
            cursor: editor.name.chars().count(),
            editor,
            secret: Zeroizing::new(String::new()),
            field: 0,
            keys: vec![],
            selected: 0,
            chosen: None,
            busy: true,
            pending: None,
        });
        self.status("Listing eligible keys… Esc cancels.", false);
        let generation = self.generation;
        let tx = tx.clone();
        tasks.push(
            generation,
            tokio::spawn(async move {
                let result = author.identities().await;
                let _ = tx.send(Message::Identities { generation, result });
            }),
        );
    }
    fn prepare_creation(
        &mut self,
        tx: &mpsc::UnboundedSender<Message>,
        tasks: &mut Tasks,
        reuse: bool,
    ) {
        let View::Create(c) = &mut self.view else {
            return;
        };
        if c.busy || c.pending.is_some() {
            return;
        }
        if !crate::config::valid_name(&c.editor.name) {
            self.status("Invalid environment name.", true);
            return;
        }
        if c.editor.original.as_deref() != Some(&c.editor.name)
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
                if reuse && !replacing {
                    self.status(
                        "No existing record for selected ID and key; nothing reused.",
                        true,
                    );
                    return;
                }
                c.pending = Some(pending);
                if reuse {
                    self.save_record(tx, tasks, true);
                } else if replacing {
                    self.status("Replace ciphertext? Updates affect ALL references to this record. y confirms; n cancels.", false);
                } else {
                    self.save_creation(tx, tasks);
                }
            }
            Err(error) => self.status(error.to_string(), true),
        }
    }
    fn save_creation(&mut self, tx: &mpsc::UnboundedSender<Message>, tasks: &mut Tasks) {
        self.save_record(tx, tasks, false);
    }
    fn save_record(
        &mut self,
        tx: &mpsc::UnboundedSender<Message>,
        tasks: &mut Tasks,
        reused: bool,
    ) {
        let View::Create(c) = &mut self.view else {
            return;
        };
        let Some(pending) = c.pending.take() else {
            return;
        };
        c.busy = true;
        let value = c.secret.clone();
        let name = c.editor.name.clone();
        let original = c.editor.original.clone();
        let source = c.editor.source.clone();
        let generation = self.generation;
        let tx = tx.clone();
        self.status(
            if reused {
                "Verifying existing ciphertext… Esc cancels unfinished work."
            } else {
                "Saving ciphertext… Esc cancels unfinished work, not completed writes."
            },
            false,
        );
        tasks.push(
            generation,
            tokio::spawn(async move {
                let result = if reused {
                    pending.reuse().await
                } else {
                    pending.save(value).await
                };
                // No await between replacement completion and reporting it.
                let _ = tx.send(Message::Created {
                    reused,
                    generation,
                    name,
                    original,
                    source,
                    result,
                });
            }),
        );
    }
    fn delete_mapping(
        &mut self,
        name: &str,
        removal: Option<Result<Box<dyn providers::PreparedDelete>>>,
        config_boundary: impl FnMut(crate::config::WriteBoundary) -> Result<()>,
        mut storage_boundary: impl FnMut(crate::config::WriteBoundary) -> Result<()>,
    ) {
        let encrypted =
            providers::get(&self.config.entries[name].source).is_some_and(|p| p.author().is_some());
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
                        "Mapping removed; config durability uncertain; ciphertext kept: {error}"
                    ),
                    true,
                );
            } else {
                self.status(
                    format!("Configuration unchanged; ciphertext kept: {error}"),
                    true,
                );
            }
            // Even after replacement, keep ciphertext on uncertain config durability.
            return;
        }
        self.main();
        match removal {
            None => self.status(if encrypted { format!("Deleted {name}; ciphertext kept.") } else { format!("Deleted {name}.") }, false),
            Some(Err(error)) => self.status(format!("Mapping removed; ciphertext kept: {error}"), true),
            Some(Ok(mut selected)) => match selected.remove(&mut storage_boundary) {
                Ok(()) => self.status("Mapping and selected ciphertext removed. Other loaded references to it will fail safely.", false),
                Err(error) => {
                    let changed = error.downcast_ref::<crate::config::PersistenceError>().is_some_and(|e| e.changed);
                    self.status(format!("Mapping removed; ciphertext {}: {error}", if changed { "removed but storage durability uncertain" } else { "kept" }), true);
                }
            },
        }
    }
    fn message(&mut self, msg: Message) {
        self.message_checked(msg, |_| Ok(()));
    }
    fn message_checked(
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
                        Err(error) => self.status(error.to_string(), true),
                    }
                }
            }
            Message::Created {
                reused,
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
                        let mut entries = self.config.entries.clone();
                        if let Some(original) = original && original != name { entries.shift_remove(&original); }
                        entries.insert(name, Entry { source, value: reference });
                        match self.config.save_checked(entries, boundary) {
                            Ok(()) => { self.main(); self.status(if reused { "Reused ciphertext; saved configuration." } else { "Saved encrypted value and configuration." }, false); }
                            Err(error) => {
                                let changed = error.downcast_ref::<crate::config::PersistenceError>().is_some_and(|e| e.changed);
                                self.status(if reused {
                                    format!("Ciphertext unchanged; configuration {}; {error}", if changed { "replaced but durability uncertain" } else { "unchanged" })
                                } else if changed {
                                    format!("Ciphertext and configuration replaced; config durability uncertain: {error}")
                                } else {
                                    format!("Ciphertext saved; configuration unchanged; existing references may now resolve the updated value; unused ciphertext may remain: {error}")
                                }, true);
                            }
                        }
                    }
                    Ok(_) => self.status(if reused { "Cancelled draft: ciphertext unchanged; configuration unchanged." } else { "Cancelled draft: ciphertext was saved; configuration unchanged. Completed writes were not undone." }, true),
                    Err(error) => {
                        let changed = error.downcast_ref::<crate::config::PersistenceError>().is_some_and(|e| e.changed);
                        self.status(if changed {
                            format!("Ciphertext replaced but durability uncertain; configuration unchanged; existing references may now resolve the updated value: {error}")
                        } else { format!("Ciphertext unchanged; configuration unchanged: {error}") }, true);
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
                        self.status("Revealed for 30 seconds. Space hides it now.", false);
                    }
                    Err(e) => self.status(e.to_string(), true),
                }
            }
            Message::Copy { generation, result } if generation == self.generation => {
                self.busy_copy = false;
                match result {
                    Ok(()) => self.status(
                        "Copied. Your desktop owns the clipboard; psst does not clear it.",
                        false,
                    ),
                    Err(e) => self.status(e.to_string(), true),
                }
            }
            Message::Preview { generation, result } if generation == self.generation => {
                match result {
                    Ok(value) => {
                        if let View::Editor(e) = &mut self.view {
                            e.preview = Some(value);
                            e.preview_at = Some(Instant::now());
                        }
                        self.status("Preview fetched (auto-hides after 30 seconds).", false);
                    }
                    Err(e) => self.status(e.to_string(), true),
                }
            }
            Message::Browse { generation, result } if generation == self.generation => {
                if let View::Browser { browser, .. } = &mut self.view {
                    browser.loading = false;
                    match result {
                        Ok(choices) => {
                            browser.choices = choices;
                            self.status("Enter selects · Backspace goes up · Esc cancels", false);
                        }
                        Err(e) => self.status(e.to_string(), true),
                    }
                }
            }
            _ => {}
        }
    }
    fn key(
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
                _ => self.error_panel = false,
            }
        }
        if matches!(self.view, View::Help) {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Char('?' | 'h') | KeyCode::Enter
            ) {
                self.main();
            }
            return false;
        }
        if let Some(name) = self.delete.clone() {
            let entry = &self.config.entries[&name];
            let author = providers::get(&entry.source).and_then(|p| p.author());
            match key.code {
                KeyCode::Char('r') if author.is_some() && !self.delete_busy => {
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
        if matches!(self.view, View::Browser { .. }) {
            if key.code == KeyCode::Esc {
                let view = std::mem::replace(&mut self.view, View::Main);
                if let View::Browser { editor, .. } = view {
                    self.generation += 1;
                    self.view = View::Editor(editor);
                }
                return false;
            }
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
                    browser.selected = (browser.selected + 1).min(filtered.len().saturating_sub(1))
                }
                KeyCode::Enter if !browser.loading => {
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
                                self.generation += 1;
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
                KeyCode::Backspace if browser.query.is_empty() && browser.stack.len() > 1 => {
                    browser.stack.pop();
                    self.browse(tx, tasks);
                }
                KeyCode::Backspace => {
                    browser.query.pop();
                    browser.selected = 0;
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    browser.query.push(c);
                    browser.selected = 0;
                }
                _ => {}
            }
            return false;
        }
        if matches!(self.view, View::Create(_)) {
            if key.code == KeyCode::Esc {
                self.main();
                self.status("Draft cancelled. Completed writes are not undone; pending completion will be reported.", false);
                return false;
            }
            let View::Create(c) = &mut self.view else {
                return false;
            };
            if c.busy {
                return false;
            }
            if c.pending.is_some() {
                match key.code {
                    KeyCode::Char('y') => self.save_creation(tx, tasks),
                    KeyCode::Char('n') => {
                        c.pending = None;
                        self.status("Replacement cancelled; draft retained.", false);
                    }
                    _ => {}
                }
                return false;
            }
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                if key.code == KeyCode::Char('s') {
                    self.prepare_creation(tx, tasks, false);
                } else if key.code == KeyCode::Char('u') {
                    self.prepare_creation(tx, tasks, true);
                }
                return false;
            }
            match key.code {
                KeyCode::Tab => c.focus((c.field + 1) % 4),
                KeyCode::BackTab => c.focus((c.field + 3) % 4),
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
                    KeyCode::Char('n') => self.create(tx, tasks),
                    KeyCode::Char('p') => self.preview(tx, tasks),
                    KeyCode::Char('r') => {
                        if let View::Editor(e) = &mut self.view {
                            e.show = !e.show;
                            e.shown_at = e.show.then(Instant::now);
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
                    self.generation += 1;
                }
                KeyCode::Left => e.cursor = e.cursor.saturating_sub(1),
                KeyCode::Right => e.cursor = (e.cursor + 1).min(e.len()),
                KeyCode::Home => e.cursor = 0,
                KeyCode::End => e.cursor = e.len(),
                KeyCode::Backspace => {
                    e.delete(true);
                    self.generation += 1;
                }
                KeyCode::Delete => {
                    e.delete(false);
                    self.generation += 1;
                }
                KeyCode::Char(c) => {
                    e.change_text(&c.to_string());
                    self.generation += 1;
                }
                _ => {}
            }
            return false;
        }
        if self.searching {
            match key.code {
                KeyCode::Esc => {
                    self.query.clear();
                    self.searching = false;
                }
                KeyCode::Enter => self.searching = false,
                KeyCode::Backspace => {
                    self.query.pop();
                    self.selected = 0;
                }
                KeyCode::Char(c) => {
                    self.query.push(c);
                    self.selected = 0;
                }
                _ => {}
            }
            return false;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Char('?' | 'h') => {
                self.invalidate();
                self.view = View::Help;
            }
            KeyCode::Char('/') => self.searching = true,
            KeyCode::Up | KeyCode::Char('k') => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(self.names().len().saturating_sub(1))
            }
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = self.names().len().saturating_sub(1),
            KeyCode::PageDown => {
                self.selected = (self.selected + 10).min(self.names().len().saturating_sub(1))
            }
            KeyCode::PageUp => self.selected = self.selected.saturating_sub(10),
            KeyCode::Char('a') => {
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
                    format!("SECRET_{next}"),
                    Entry {
                        source: "manual".into(),
                        value: String::new(),
                    },
                ));
                self.status("New secret · nothing is saved until Ctrl+S.", false);
            }
            KeyCode::Enter | KeyCode::Char('e') => {
                if let Some(name) = self.selected_name() {
                    let entry = self.config.entries[&name].clone();
                    self.invalidate();
                    self.view = View::Editor(Editor::new(Some(name.clone()), name, entry));
                    self.status(
                        "Preview is opt-in (Ctrl+P); shell references execute only when requested.",
                        false,
                    );
                }
            }
            KeyCode::Delete | KeyCode::Char('d') => {
                self.delete = self.selected_name();
                self.delete_remove = false;
                self.delete_busy = false;
            }
            KeyCode::Char(' ') => {
                if let Some(name) = self.selected_name()
                    && self.revealed.remove(&name).is_none()
                    && !self.loading.contains(&name)
                {
                    self.loading.insert(name.clone());
                    let entry = self.config.entries[&name].clone();
                    let dir = self.config.dir().to_owned();
                    let generation = self.generation;
                    let tx = tx.clone();
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
            }
            KeyCode::Char('c') if !self.busy_copy => {
                if let Some(name) = self.selected_name() {
                    self.busy_copy = true;
                    self.status("Copying…", false);
                    let cached = self.revealed.get(&name).map(|(v, _)| v.clone());
                    let entry = self.config.entries[&name].clone();
                    let dir = self.config.dir().to_owned();
                    let generation = self.generation;
                    let tx = tx.clone();
                    tasks.push(
                        generation,
                        tokio::spawn(async move {
                            let result = async {
                                let value = match cached {
                                    Some(v) => v,
                                    None => providers::fetch(&entry, &dir).await?,
                                };
                                operations::copy(&value).await
                            }
                            .await;
                            let _ = tx.send(Message::Copy { generation, result });
                        }),
                    );
                }
            }
            _ => {}
        }
        false
    }
    fn paste(&mut self, text: String) {
        let text = Zeroizing::new(text);
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
            self.generation += 1;
        } else if self.searching {
            self.query.push_str(&text);
            self.selected = 0;
        }
    }
    fn expire(&mut self) {
        self.revealed
            .retain(|_, (_, time)| time.elapsed() < Duration::from_secs(30));
        match &mut self.view {
            View::Editor(editor) | View::Browser { editor, .. } => editor.expire(),
            _ => {}
        }
    }
}

fn block(title: impl Into<String>) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(MUTED))
        .title(Line::from(title.into()).style(Style::default().fg(ACCENT).bold()))
        .padding(Padding::horizontal(1))
}
fn shortcut_lines(actions: &[(&str, &str)], width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut line = Line::default();
    for (key, label) in actions {
        let key = Span::styled(format!(" {key} "), Style::default().fg(ACCENT).bg(PANEL));
        let label = Span::styled(format!(" {label}  "), Style::default().fg(Color::White));
        let action_width = key.width() + label.width();
        if !line.spans.is_empty() && line.width() + action_width > width {
            lines.push(line);
            line = Line::default();
        }
        line.spans.extend([key, label]);
    }
    if !line.spans.is_empty() {
        lines.push(line);
    }
    lines
}
fn clean(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    frame.render_widget(
        Block::new().style(Style::default().bg(BG).fg(Color::White)),
        area,
    );
    if area.width < 45 || area.height < 12 {
        frame.render_widget(
            Paragraph::new("psst 🤫\n\nPlease resize to at least 45 × 12.\nCtrl+C quits safely.")
                .style(Style::default().fg(ACCENT)),
            area,
        );
        return;
    }
    let actions: &[(&str, &str)] = if app.delete.is_some() {
        &[
            ("Enter / y", "delete"),
            ("Esc / n", "cancel"),
            ("Ctrl+C", "quit"),
        ]
    } else {
        match &app.view {
            View::Main if app.searching => {
                &[("Type", "search"), ("Enter", "apply"), ("Esc", "clear")]
            }
            View::Main => &[
                ("↑↓", "select"),
                ("Enter", "edit"),
                ("a", "add"),
                ("Space", "reveal"),
                ("c", "copy"),
                ("/", "search"),
                ("?", "help"),
                ("q", "quit"),
            ],
            View::Create(_) => &[
                ("Tab", "fields"),
                ("Ctrl+S", "save encrypted"),
                ("Ctrl+U", "reuse existing"),
                ("Enter", "newline / choose key"),
                ("Esc", "cancel"),
            ],
            View::Editor(_) => &[
                ("Ctrl+N", "create encrypted"),
                ("Tab", "fields"),
                ("Ctrl+S", "save"),
                ("Esc", "cancel"),
                ("Ctrl+B", "browse"),
                ("Ctrl+P", "preview"),
                ("Ctrl+R", "reveal"),
            ],
            View::Browser { .. } => &[
                ("Type", "filter"),
                ("↑↓", "select"),
                ("Enter", "choose"),
                ("Backspace", "up"),
                ("Esc", "cancel"),
            ],
            View::Help => &[("Esc / Enter", "return"), ("Ctrl+C", "quit")],
        }
    };
    let hints = shortcut_lines(actions, usize::from(area.width.saturating_sub(2)));
    let hint_height = hints.len() as u16;
    let compact_creator = matches!(app.view, View::Create(_)) && area.height < 24;
    let chunks = Layout::vertical([
        Constraint::Length(if compact_creator { 1 } else { 3 }),
        Constraint::Min(if compact_creator { 4 } else { 3 }),
        Constraint::Length(if compact_creator { 1 } else { 2 }),
        Constraint::Length(hint_height),
    ])
    .margin(if compact_creator { 0 } else { 1 })
    .split(area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(" psst 🤫 ", Style::default().fg(ACCENT).bold()),
                Span::styled(" / secrets, quietly", Style::default().fg(MUTED)),
            ]),
            Line::from(clean(&app.config.path.display().to_string()))
                .style(Style::default().fg(MUTED)),
        ]),
        chunks[0],
    );
    match &mut app.view {
        View::Main => {
            let query = app.query.to_lowercase();
            let names = app
                .config
                .entries
                .keys()
                .filter(|n| n.to_lowercase().contains(&query))
                .cloned()
                .collect::<Vec<_>>();
            app.selected = app.selected.min(names.len().saturating_sub(1));
            app.table.select(if names.is_empty() {
                None
            } else {
                Some(app.selected)
            });
            let rows = names.iter().map(|name| {
                let entry = &app.config.entries[name];
                let reference = if providers::get(&entry.source)
                    .is_none_or(|provider| provider.sensitive_reference())
                {
                    MASK.into()
                } else {
                    clean(&entry.value)
                };
                let secret = if app.loading.contains(name) {
                    "Fetching…".into()
                } else {
                    app.revealed
                        .get(name)
                        .map(|(v, _)| clean(v))
                        .unwrap_or(MASK.into())
                };
                Row::new(vec![
                    Cell::from(name.clone()).style(Style::default().bold()),
                    Cell::from(clean(&entry.source)).style(Style::default().fg(MUTED)),
                    Cell::from(reference).style(Style::default().fg(MUTED)),
                    Cell::from(secret).style(Style::default().fg(ACCENT)),
                ])
                .height(1)
            });
            let title = if app.query.is_empty() {
                format!(" Secrets · {} ", names.len())
            } else {
                format!(" Search: {} · {} matches ", clean(&app.query), names.len())
            };
            if names.is_empty() {
                frame.render_widget(Paragraph::new(if app.query.is_empty() { "\n  No secrets yet.\n\n  Press a to add your first secret.\n  References stay local; provider CLIs handle authentication." } else { "\n  No matching secrets. Press / to change the search." }).block(block(title)).style(Style::default().fg(MUTED)).wrap(Wrap { trim: false }), chunks[1]);
            } else {
                let widths = if area.width >= 100 {
                    vec![
                        Constraint::Percentage(23),
                        Constraint::Percentage(14),
                        Constraint::Percentage(38),
                        Constraint::Percentage(25),
                    ]
                } else {
                    vec![
                        Constraint::Percentage(25),
                        Constraint::Percentage(18),
                        Constraint::Percentage(27),
                        Constraint::Percentage(30),
                    ]
                };
                let table = Table::new(rows, widths)
                    .header(
                        Row::new(["NAME", "SOURCE", "REFERENCE", "SECRET"])
                            .style(Style::default().fg(MUTED))
                            .bottom_margin(1),
                    )
                    .column_spacing(1)
                    .block(block(title))
                    .row_highlight_style(Style::default().bg(PANEL).fg(ACCENT))
                    .highlight_symbol("› ");
                frame.render_stateful_widget(table, chunks[1], &mut app.table);
            }
        }
        View::Create(c) => {
            let inner = block(" Create encrypted value · always masked ");
            let focused_only = chunks[1].height < 15;
            let body = if focused_only {
                chunks[1]
            } else {
                frame.render_widget(inner.clone(), chunks[1]);
                inner.inner(chunks[1])
            };
            let fields = if focused_only {
                let mut fields = vec![Rect::default(); 4];
                fields[c.field] = body;
                fields
            } else {
                Layout::vertical([
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Min(4),
                ])
                .split(body)
                .to_vec()
            };
            for (i, label) in [
                "Environment name",
                "Exact secret ID",
                "Exact UTF-8 value · masked",
            ]
            .iter()
            .enumerate()
            {
                if focused_only && c.field != i {
                    continue;
                }
                let text = match i {
                    0 => clean(&c.editor.name),
                    1 => clean(&c.editor.value),
                    _ => MASK.to_owned(),
                };
                frame.render_widget(
                    Paragraph::new(text).block(block(*label).border_style(
                        Style::default().fg(if c.field == i { ACCENT } else { MUTED }),
                    )),
                    fields[i],
                );
            }
            if !focused_only || c.field == 3 {
                let fingerprint_width = usize::from(fields[3].width.saturating_sub(4)).max(1);
                let items: Vec<_> = c
                    .keys
                    .iter()
                    .enumerate()
                    .map(|(i, key)| {
                        let text = format!(
                            "{} {}",
                            if c.chosen == Some(i) { "✓" } else { " " },
                            key.fingerprint
                        );
                        let chars = text.chars().collect::<Vec<_>>();
                        ListItem::new(
                            chars
                                .chunks(fingerprint_width)
                                .map(|part| Line::from(part.iter().collect::<String>()))
                                .collect::<Vec<_>>(),
                        )
                    })
                    .collect();
                let mut state =
                    ListState::default().with_selected((!c.keys.is_empty()).then_some(c.selected));
                frame.render_stateful_widget(
                    List::new(items)
                        .block(
                            block(format!(
                                " SSH key {}/{} · ↑↓ Enter chooses ",
                                c.selected + usize::from(!c.keys.is_empty()),
                                c.keys.len()
                            ))
                            .border_style(
                                Style::default().fg(if c.field == 3 { ACCENT } else { MUTED }),
                            ),
                        )
                        .highlight_style(Style::default().bg(PANEL)),
                    fields[3],
                    &mut state,
                );
            }
        }
        View::Editor(e) => {
            let inner = block(if e.original.is_some() {
                " Edit secret "
            } else {
                " Add secret "
            });
            let body = inner.inner(chunks[1]);
            frame.render_widget(inner, chunks[1]);
            let fields = Layout::vertical([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(1),
            ])
            .split(body);
            let provider = providers::get(&e.source);
            let ref_label = provider
                .map(|provider| provider.reference_label())
                .unwrap_or("Reference (unknown provider)");
            for (i, label) in ["Environment name", "Source · ← / → to choose", ref_label]
                .into_iter()
                .enumerate()
            {
                let text = match i {
                    0 => e.name.clone(),
                    1 => clean(&e.source),
                    _ if provider.is_some_and(|provider| provider.sensitive_reference())
                        && !e.show =>
                    {
                        "•".repeat(e.value.chars().count().min(200))
                    }
                    _ => clean(&e.value),
                };
                let width = fields[i].width.saturating_sub(4) as usize;
                let start = if e.field == i {
                    e.cursor.saturating_sub(width.saturating_sub(1))
                } else {
                    0
                };
                let shown: String = text.chars().skip(start).take(width).collect();
                let field_block = block(label).border_style(Style::default().fg(if e.field == i {
                    ACCENT
                } else {
                    MUTED
                }));
                frame.render_widget(Paragraph::new(shown).block(field_block), fields[i]);
                if e.field == i && i != 1 && width > 0 {
                    frame.set_cursor_position((
                        fields[i].x + 2 + e.cursor.saturating_sub(start).min(width - 1) as u16,
                        fields[i].y + 1,
                    ));
                }
            }
            let preview = e.preview.as_ref().map(|s| clean(s)).unwrap_or_else(|| {
                "Not fetched. Ctrl+P previews on demand; may execute shell commands.".into()
            });
            frame.render_widget(
                Paragraph::new(preview)
                    .style(Style::default().fg(if e.preview.is_some() { ACCENT } else { MUTED }))
                    .block(block(" Preview · auto-hides after 30s "))
                    .wrap(Wrap { trim: false }),
                fields[3],
            );
        }
        View::Browser { browser, .. } => {
            let title = browser.stack.last().unwrap().title();
            let choices = browser
                .choices
                .iter()
                .filter(|c| {
                    c.label
                        .to_lowercase()
                        .contains(&browser.query.to_lowercase())
                })
                .collect::<Vec<_>>();
            let items = choices
                .iter()
                .map(|c| {
                    ListItem::new(Line::from(vec![
                        Span::raw(clean(&c.label)),
                        Span::styled(
                            format!("  {}", clean(&c.detail)),
                            Style::default().fg(MUTED),
                        ),
                    ]))
                })
                .collect::<Vec<_>>();
            let content =
                Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).split(chunks[1]);
            frame.render_widget(
                Paragraph::new(format!("Type to filter: {}", clean(&browser.query)))
                    .style(Style::default().fg(ACCENT)),
                content[0],
            );
            if browser.loading || choices.is_empty() {
                frame.render_widget(
                    Paragraph::new(if browser.loading {
                        "Loading…"
                    } else {
                        "No matches. Backspace goes up; Esc returns to the editor."
                    })
                    .block(block(title))
                    .wrap(Wrap { trim: false }),
                    content[1],
                );
            } else {
                browser.selected = browser.selected.min(choices.len().saturating_sub(1));
                let mut state = ListState::default().with_selected(Some(browser.selected));
                frame.render_stateful_widget(
                    List::new(items)
                        .block(block(title))
                        .highlight_style(Style::default().bg(PANEL).fg(ACCENT))
                        .highlight_symbol("› "),
                    content[1],
                    &mut state,
                );
            }
        }
        View::Help => {
            frame.render_widget(Paragraph::new("YOUR SECRETS, YOUR TOOLS\n\n↑ / ↓  Select     Enter  Edit     a  Add     /  Search\nSpace  Reveal/hide (30 seconds)     c  Copy     Delete  Remove\nPgUp / PgDn  Scroll     q / Ctrl+C  Quit\n\nEDITOR\nTab / Shift+Tab  Fields     ← / →  Choose source or move cursor\nCtrl+S  Save     Esc  Discard     Ctrl+R  Show/hide literal\nCtrl+B  Browse op/Vault · Ctrl+P  Preview (opt-in) · Ctrl+N  Encrypted creation/update · Ctrl+U in masked editor reuses ciphertext\n\nBROWSER\nType to filter     Enter  Select     Backspace  Parent\nEsc  Cancel browser, preserving your draft\n\nUsage: psst COMMAND [ARGS...] injects secrets in parallel.\n--copy NAME copies one; --write exports a private .env.\nManual secrets and .env files are plaintext.\nClipboard is not automatically cleared.\n\nEsc or Enter returns.").block(block(" Help ")).wrap(Wrap { trim: false }), chunks[1]);
        }
    }
    frame.render_widget(
        Paragraph::new(clean(&app.status))
            .style(Style::default().fg(if app.error { Color::LightRed } else { MUTED }))
            .wrap(Wrap { trim: false }),
        chunks[2],
    );
    frame.render_widget(Paragraph::new(hints), chunks[3]);
    if let View::Create(c) = &app.view
        && c.pending.is_some()
    {
        let popup = centered(area, area.width.saturating_sub(2).min(85), 10);
        frame.render_widget(Clear, popup);
        let count = app
            .config
            .entries
            .iter()
            .filter(|(_, e)| {
                e.source == c.editor.source
                    && c.pending
                        .as_ref()
                        .is_some_and(|p| p.matches_reference(&e.value))
            })
            .count();
        let warning = format!(
            "Replace existing ciphertext?\nUpdates affect ALL references to this\n(ID,key) record.\nLoaded config only: {count}\npossible references.\ny replaces · n retains draft\nEsc cancels"
        );
        frame.render_widget(
            Paragraph::new(warning)
                .block(block(" Confirm replacement "))
                .wrap(Wrap { trim: false }),
            popup,
        );
    }
    if let Some(name) = &app.delete {
        let popup = centered(area, 78, 11);
        frame.render_widget(Clear, popup);
        // Reserve one clipped line for the name, not space needed for consent.
        let name_limit = usize::from(popup.width.saturating_sub(13));
        let mut shown_name: String = clean(name).chars().take(name_limit).collect();
        if name.chars().count() > name_limit {
            shown_name.push('…');
        }
        let entry = &app.config.entries[name];
        let author = providers::get(&entry.source).and_then(|p| p.author());
        let options = if let Some(author) = author {
            let shared = app.config.entries.iter().any(|(other, value)| {
                other != name
                    && value.source == entry.source
                    && author.may_share(&entry.value, &value.value)
            });
            format!(
                "\nCiphertext: {}\nr toggles ciphertext removal\n{}{}",
                if app.delete_remove {
                    "REMOVE selected record"
                } else {
                    "KEEP (default)"
                },
                if shared {
                    "Known shared references in loaded config\nmay be affected.\n"
                } else {
                    ""
                },
                if app.delete_busy {
                    "Selecting identity… Esc cancels.\n"
                } else {
                    ""
                }
            )
        } else {
            String::new()
        };
        frame.render_widget(
            Paragraph::new(format!(
                "Delete {shown_name}?\nRemoves reference from this config.{options}\nEnter / y delete · Esc / n cancel"
            ))
            .block(block(" Confirm deletion ").border_style(Style::default().fg(Color::LightRed))),
            popup,
        );
    }
    if app.error_panel {
        let popup = centered(
            area,
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );
        frame.render_widget(Clear, popup);
        let panel = block(" Failure details ").border_style(Style::default().fg(Color::LightRed));
        let body = panel.inner(popup);
        frame.render_widget(panel, popup);
        let parts = Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).split(body);
        let details = Paragraph::new(clean(&app.status))
            .style(Style::default().fg(Color::LightRed))
            .wrap(Wrap { trim: false });
        // Clamp scrolling using the renderer's wrapping, without unstable APIs.
        let mut rendered = Buffer::empty(parts[0]);
        loop {
            rendered.reset();
            details
                .clone()
                .scroll((app.error_scroll, 0))
                .render(parts[0], &mut rendered);
            if app.error_scroll == 0
                || rendered
                    .content
                    .iter()
                    .any(|cell| !cell.symbol().trim().is_empty())
            {
                break;
            }
            app.error_scroll -= 1;
        }
        frame.render_widget(details.scroll((app.error_scroll, 0)), parts[0]);
        frame.render_widget(
            Paragraph::new("PgUp/PgDn scroll details\nOther keys close and act normally"),
            parts[1],
        );
    }
}
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

/// Run the interactive manager on terminal stdin/stdout, restoring state on exit.
/// Fetches and browsers run independently; previews are explicit and expire.
pub async fn run(config: Config) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("The manager needs a terminal. Use --help, --write, --copy NAME or a command.");
    }
    terminal::enable_raw_mode()?;
    let _guard = TerminalGuard;
    execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let old_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|info| {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
        eprintln!("psst panicked: {info}");
    }));
    let result = loop_run(&mut terminal, App::new(config)).await;
    std::panic::set_hook(old_hook);
    result
}
async fn loop_run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut app: App,
) -> Result<()> {
    let mut events = event::EventStream::new();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut tasks = Tasks(Vec::new());
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    #[cfg(unix)]
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    #[cfg(not(unix))]
    let mut term = ();
    loop {
        tasks.cancel_stale(app.generation);
        app.expire();
        terminal.draw(|f| draw(f, &mut app))?;
        tokio::select! {
            event = events.next() => match event {
                Some(Ok(Event::Key(key))) => if app.key(key, &tx, &mut tasks) { break; },
                Some(Ok(Event::Paste(text))) => app.paste(text),
                Some(Err(e)) => return Err(e.into()),
                None => break,
                _ => {}
            },
            Some(msg) = rx.recv() => app.message(msg),
            _ = tick.tick() => app.expire(),
            _ = tokio::signal::ctrl_c() => break,
            _ = termination(&mut term) => break,
        }
    }
    Ok(())
}
#[cfg(unix)]
async fn termination(term: &mut tokio::signal::unix::Signal) {
    term.recv().await;
}

#[cfg(not(unix))]
async fn termination(_: &mut ()) {
    std::future::pending::<()>().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    fn app() -> App {
        let t = tempfile::tempdir().unwrap().keep().canonicalize().unwrap();
        App::new(Config::load(t.join(".psst.json")).unwrap())
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
        let bytes = include_bytes!("../tests/fixtures/local-storage-v1.json");
        let document: KeyFile = serde_json::from_slice(bytes).unwrap();
        let key =
            PublicKey::from_blob(&BASE64.decode(document.public_key.as_bytes()).unwrap()).unwrap();
        let dir = app.config.dir().join(".psst/local-storage");
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
                let config_changed = config_failure.is_none()
                    || config_failure == Some(WriteBoundary::DirectorySync);
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
                        app.config.path.with_extension("psst-lock")
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
        key(&mut app, KeyCode::Char('d'));
        assert!(screen(&mut app).contains("KEEP (default)"));
        assert!(screen(&mut app).contains("Known shared references"));
        key(&mut app, KeyCode::Char('r'));
        assert!(app.delete_remove);
        key(&mut app, KeyCode::Esc);
        assert!(app.config.entries.contains_key("FIRST"));
        app.message(Message::Deletion {
            generation: app.generation - 1,
            name: "FIRST".into(),
            result: Ok(selected),
        });
        assert_eq!(std::fs::read(&path).unwrap(), before);
        key(&mut app, KeyCode::Char('d'));
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
        key(&mut app, KeyCode::Char('a'));
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
                            "REMOVE selected record"
                        } else {
                            "KEEP (default)"
                        },
                        "r toggles ciphertext removal",
                        "Known shared references in loaded config",
                        "may be affected.",
                        "Enter / y delete · Esc / n cancel",
                    ] {
                        assert!(
                            text.contains(expected),
                            "{width}×{height}: missing {expected}: {text}"
                        );
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
            KeyCode::Enter,
            KeyCode::Delete,
            KeyCode::Char(' '),
            KeyCode::Char('c'),
        ] {
            key(&mut app, code);
        }
        assert_eq!(app.selected, 0);
        assert!(matches!(app.view, View::Main));
        key(&mut app, KeyCode::Char('a'));
        assert!(matches!(app.view, View::Editor(_)));
        key(&mut app, KeyCode::Esc);
        assert!(app.config.entries.is_empty());
        assert!(!app.config.path.exists());
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
                assert!(text.contains("API_KEY"));
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
    fn legacy_delete_d_y_and_cancel_d_n() {
        let mut app = legacy_app();
        key(&mut app, KeyCode::Char('d'));
        assert_eq!(app.delete.as_deref(), Some("SECRET_1"));
        assert!(app.config.entries.contains_key("SECRET_1"));
        assert!(screen(&mut app).contains("Delete SECRET_1?"));
        key(&mut app, KeyCode::Char('n'));
        assert!(app.delete.is_none());
        assert!(app.config.entries.contains_key("SECRET_1"));
        key(&mut app, KeyCode::Char('d'));
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
            assert!(
                matches!(&app.view, View::Editor(e) if e.original.as_deref() == Some(expected))
            );
            key(&mut app, KeyCode::Esc);
        }
    }
    #[tokio::test]
    async fn legacy_reveal_loading_and_hide() {
        let mut app = legacy_app();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut tasks = Tasks(vec![]);
        assert!(!screen(&mut app).contains("value1"));
        // Space deliberately replaces the old p shortcut and artificial 100 ms delay.
        app.key(
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
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
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
            &tx,
            &mut tasks,
        );
        assert!(app.revealed.is_empty());
        assert!(!screen(&mut app).contains("value1"));
    }
    #[test]
    fn legacy_add_a_opens_a_real_unsaved_draft() {
        let mut app = legacy_app();
        key(&mut app, KeyCode::Char('a'));
        assert!(
            matches!(&app.view, View::Editor(e) if e.original.is_none() && e.name == "SECRET_3")
        );
        assert_eq!(app.config.entries.len(), 2);
        assert!(!app.config.path.exists());
    }
    #[test]
    fn legacy_help_h_shows_usage_and_returns() {
        let mut app = legacy_app();
        key(&mut app, KeyCode::Char('h'));
        assert!(matches!(app.view, View::Help));
        assert!(screen(&mut app).contains("Usage"));
        key(&mut app, KeyCode::Char('h'));
        assert!(matches!(app.view, View::Main));
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
    fn literal_reveal_toggle_sets_and_clears_expiry() {
        let mut app = legacy_app();
        key(&mut app, KeyCode::Enter);
        let (tx, _) = mpsc::unbounded_channel();
        for expected in [true, false] {
            app.key(
                KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
                &tx,
                &mut Tasks(vec![]),
            );
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
                app.searching = false;
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
                app.searching = view == 1;
                if view == 5 {
                    app.delete = Some("SECRET_1".into());
                }
                let mut terminal = Terminal::new(TestBackend::new(width, 30)).unwrap();
                terminal.draw(|frame| draw(frame, &mut app)).unwrap();
                let buffer = terminal.backend().buffer();
                let row = (25..29).find(|y| buffer[(1, *y)].bg == PANEL).unwrap();
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
        });
    }
    #[test]
    fn encrypted_editor_preserves_exact_text_and_never_reveals() {
        let mut app = app();
        creator(&mut app);
        for text in ["", " \n🔑\t", "PASTED_SECRET"] {
            app.paste(text.into());
        }
        key(&mut app, KeyCode::Enter);
        let expected = " \n🔑\tPASTED_SECRET\n";
        assert!(matches!(&app.view, View::Create(c) if c.secret.as_str() == expected));
        let (tx, _) = mpsc::unbounded_channel();
        for code in ['r', 'p', 'b'] {
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
                    text.contains(
                        ["Environment name", "Exact secret ID", "Exact UTF-8 value"][field]
                    ),
                    "{width}×{height}: {text}"
                );
                assert!(!text.contains("NEVER_RENDER_THIS"));
                assert!(text.contains("Ctrl+S") && text.contains("Ctrl+U") && text.contains("Esc"));
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
        fn reuse(self: Box<Self>) -> futures::future::BoxFuture<'static, Result<String>> {
            Box::pin(async { Ok("id".into()) })
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
    #[tokio::test]
    async fn reuse_config_failures_and_stale_results_do_not_claim_ciphertext_writes() {
        use crate::config::WriteBoundary;
        for failure in [
            WriteBoundary::Write,
            WriteBoundary::FileSync,
            WriteBoundary::Replace,
            WriteBoundary::DirectorySync,
        ] {
            let mut app = app();
            creator(&mut app);
            let storage = app.config.dir().join("ciphertext");
            std::fs::write(&storage, b"original ciphertext").unwrap();
            if let View::Create(c) = &mut app.view {
                c.pending = Some(Box::new(TestSave {
                    path: storage.clone(),
                    failure: None,
                    replacing: true,
                }));
            }
            let (tx, mut rx) = mpsc::unbounded_channel();
            let mut tasks = Tasks(vec![]);
            app.save_record(&tx, &mut tasks, true);
            let message = tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .unwrap()
                .unwrap();
            app.message_checked(message, |boundary| {
                anyhow::ensure!(boundary != failure, "Injected config failure");
                Ok(())
            });
            assert_eq!(std::fs::read(&storage).unwrap(), b"original ciphertext");
            assert!(app.error && app.status.contains("Ciphertext unchanged"));
            assert_narrow_failure_readable(&mut app);
            assert_eq!(
                app.config.path.exists(),
                failure == WriteBoundary::DirectorySync
            );
            key(&mut app, KeyCode::Esc);
            creator(&mut app);
            app.paste("NEW_DRAFT".into());
            app.message(Message::Created {
                reused: true,
                generation: app.generation - 1,
                name: "OLD".into(),
                original: None,
                source: "local-storage".into(),
                result: Ok("id".into()),
            });
            assert!(app.status.contains("Cancelled draft: ciphertext unchanged"));
            assert!(!app.config.entries.contains_key("OLD"));
            assert!(matches!(&app.view, View::Create(c) if c.secret.as_str() == "NEW_DRAFT"));
        }
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
                        anyhow::ensure!(
                            stage != 1 || boundary != failure,
                            "Injected config failure"
                        );
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
                    key(&mut app, KeyCode::Char('a'));
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
    fn replacement_requires_y_and_can_be_declined_without_writes() {
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
                "y replaces",
                "n retains draft",
                "Esc cancels",
            ] {
                assert!(text.contains(expected), "missing {expected}: {text}");
            }
        }
        key(&mut app, KeyCode::Enter);
        assert!(!storage.exists());
        key(&mut app, KeyCode::Char('n'));
        assert!(matches!(&app.view, View::Create(c) if c.pending.is_none()));
        assert!(!storage.exists() && !app.config.path.exists());
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
}
