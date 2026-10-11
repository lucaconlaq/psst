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
enum View {
    Main,
    Editor(Editor),
    Browser { editor: Editor, browser: Browser },
    Help,
}
enum Message {
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
    revealed: HashMap<String, (Zeroizing<String>, Instant)>,
    loading: HashSet<String>,
    status: String,
    error: bool,
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
            revealed: HashMap::new(),
            loading: HashSet::new(),
            status: "Secrets stay masked until you ask. Fetches run in parallel.".into(),
            error: false,
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
    fn message(&mut self, msg: Message) {
        match msg {
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
            match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    let mut entries = self.config.entries.clone();
                    entries.shift_remove(&name);
                    match self.config.save(entries) {
                        Ok(()) => {
                            self.delete = None;
                            self.main();
                            self.status(format!("Deleted {name}."), false);
                        }
                        Err(e) => {
                            self.delete = None;
                            self.status(e.to_string(), true);
                        }
                    }
                }
                KeyCode::Char('n') | KeyCode::Esc => self.delete = None,
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
        if matches!(self.view, View::Editor(_)) {
            if key.code == KeyCode::Esc {
                self.main();
                self.status("Changes discarded.", false);
                return false;
            }
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('s') => self.save(),
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
            KeyCode::Delete | KeyCode::Char('d') => self.delete = self.selected_name(),
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
        if let View::Editor(e) = &mut self.view {
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
            View::Editor(_) => &[
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
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(2),
        Constraint::Length(hint_height),
    ])
    .margin(1)
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
            frame.render_widget(Paragraph::new("YOUR SECRETS, YOUR TOOLS\n\n↑ / ↓  Select     Enter  Edit     a  Add     /  Search\nSpace  Reveal/hide (30 seconds)     c  Copy     Delete  Remove\nPgUp / PgDn  Scroll     q / Ctrl+C  Quit\n\nEDITOR\nTab / Shift+Tab  Fields     ← / →  Choose source or move cursor\nCtrl+S  Save     Esc  Discard     Ctrl+R  Show/hide literal\nCtrl+B  Browse op/Vault     Ctrl+P  Fetch preview (opt-in)\n\nBROWSER\nType to filter     Enter  Select     Backspace  Parent\nEsc  Cancel browser, preserving your draft\n\nUsage: psst COMMAND [ARGS...] injects secrets in parallel.\n--copy NAME copies one; --write exports a private .env.\nManual secrets and .env files are plaintext.\nClipboard is not automatically cleared.\n\nEsc or Enter returns.").block(block(" Help ")).wrap(Wrap { trim: false }), chunks[1]);
        }
    }
    frame.render_widget(
        Paragraph::new(clean(&app.status))
            .style(Style::default().fg(if app.error { Color::LightRed } else { MUTED }))
            .wrap(Wrap { trim: false }),
        chunks[2],
    );
    frame.render_widget(Paragraph::new(hints), chunks[3]);
    if let Some(name) = &app.delete {
        let popup = centered(area, 60, 7);
        frame.render_widget(Clear, popup);
        frame.render_widget(Paragraph::new(format!("Delete {name}?\n\nThis removes the reference from this config.\nEnter / y delete · Esc / n cancel")).block(block(" Confirm deletion ").border_style(Style::default().fg(Color::LightRed))).wrap(Wrap { trim: false }), popup);
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
        let t = tempfile::tempdir().unwrap().keep();
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
