use super::app::{App, View};
use crate::{browser::Target, config::MappingKind, providers};
use ratatui::{prelude::*, widgets::*};

pub(super) const ACCENT: Color = Color::Rgb(110, 210, 195);
pub(super) const MUTED: Color = Color::Rgb(135, 148, 166);
pub(super) const BG: Color = Color::Rgb(19, 25, 35);
pub(super) const PANEL: Color = Color::Rgb(28, 37, 50);
pub(super) const MASK: &str = "••••••••";

pub(super) fn block(title: impl Into<String>) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(MUTED))
        .title(Line::from(title.into()).style(Style::default().fg(ACCENT).bold()))
        .padding(Padding::horizontal(1))
}
pub(super) fn shortcut_lines(actions: &[(&str, &str)], width: usize) -> Vec<Line<'static>> {
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
pub(super) fn clean(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
fn help_lines(width: usize) -> Vec<Line<'static>> {
    let sections: &[(&str, &[(&str, &str)])] = &[
        (
            "MANAGE ENTRIES AND FILES",
            &[
                (
                    "Tab",
                    "Switch Entries / Files; keep each filter and selection",
                ),
                ("Up/Down", "Select a mapping"),
                ("Enter", "Edit mapping, or create on the + New row"),
                ("Ctrl+N", "Add an entry or file mapping"),
                ("Type", "Filter by environment name or destination"),
                ("Backspace", "Erase filter"),
                ("Ctrl+R", "Reveal or hide resolved secret (30 seconds)"),
                ("Ctrl+Y", "Copy secret to clipboard"),
                ("Ctrl+D", "Confirm mapping deletion"),
                ("Esc", "Quit from the dashboard, even while filtered"),
            ],
        ),
        (
            "EDIT A MAPPING",
            &[
                ("Tab / Shift+Tab", "Move between fields"),
                ("Left/Right", "Choose source or move text cursor"),
                (
                    "Ctrl+S",
                    "Save mapping only; pull files with psst --files-pull separately",
                ),
                ("", "File deletion keeps destination and provider contents."),
                ("Esc", "Discard changes"),
                (
                    "Ctrl+R",
                    "Reveal or hide resolved value; manual shows draft only",
                ),
                (
                    "",
                    "Resolving shell values executes the command on request.",
                ),
                ("Ctrl+Y", "Copy secret to clipboard"),
                ("Ctrl+B", "Browse provider references when supported"),
                ("", "Create encrypted values from the provider browser"),
            ],
        ),
        (
            "CREATE OR UPDATE ENCRYPTED VALUES",
            &[
                ("Tab / Shift+Tab", "Move between fields and SSH keys"),
                ("Up/Down", "Select an SSH key in the key field"),
                ("Enter", "Choose key, or insert newline in value field"),
                ("Ctrl+S", "Save encrypted value only; link separately"),
                ("Ctrl+R", "Reveal or hide draft value only (30 seconds)"),
                ("Ctrl+Y", "Copy draft value to clipboard"),
                ("Enter / Esc", "Confirm or decline ciphertext replacement"),
                ("Esc", "Cancel draft; completed writes are not undone"),
            ],
        ),
        (
            "BROWSE REFERENCES",
            &[
                ("Type", "Filter references"),
                ("Ctrl+N", "Create encrypted value when supported"),
                ("Ctrl+D", "Confirm encrypted-value removal; keep mappings"),
                ("Up/Down", "Select a reference"),
                ("Enter", "Choose reference or open a level"),
                ("Backspace", "Erase filter only"),
                ("Ctrl+R", "Reveal or hide selected secret (30 seconds)"),
                ("Ctrl+Y", "Copy selected secret to clipboard"),
                (
                    "Esc",
                    "Clear filter, then go up a level, then return to editor",
                ),
            ],
        ),
        (
            "CONFIRMATIONS AND GLOBAL CONTROLS",
            &[
                (
                    "Tab",
                    "Toggle optional ciphertext removal for entry deletion only",
                ),
                ("Enter / Esc", "Confirm or cancel deletion"),
                ("F1", "Open help from any view"),
                ("Ctrl+C", "Quit from any view"),
            ],
        ),
        (
            "COMMAND LINE AND SAFETY",
            &[
                (
                    "psst COMMAND [ARGS...]",
                    "Run command with resolved secrets",
                ),
                ("psst --copy NAME", "Copy one secret"),
                (
                    "psst --write",
                    "Export environment entries only to private .env",
                ),
                (
                    "psst --files-pull [--yes]",
                    "Pull all managed files: provider → local",
                ),
                (
                    "",
                    "One batch replacement prompt precedes all destination writes.",
                ),
                (
                    "",
                    "Refusal cancels; --yes approves replacements, not unsafe paths.",
                ),
                (
                    "",
                    "written / unchanged / failed; errors exit unsuccessfully.",
                ),
                (
                    "",
                    "Identical files need 0600; wrong modes fail without repair.",
                ),
                (
                    "",
                    "Independent writes have no rollback; Ctrl+C keeps completed writes.",
                ),
                ("", "Destinations and staging are plaintext, not a sandbox."),
                (
                    "",
                    "Crashes may leave system-temp psst-files-*; inspect/remove manually.",
                ),
                (
                    "",
                    "Trusted shell providers can have side effects before consent.",
                ),
                (
                    "",
                    "See README.md for filesystem and local-storage release limits.",
                ),
                ("", "Manual values and .env files are plaintext."),
                ("", "The clipboard is not automatically cleared."),
                ("", "Draft values stay masked until explicitly revealed."),
                ("", "Ctrl+C quits from any view."),
            ],
        ),
    ];
    let mut lines = Vec::new();
    for (title, actions) in sections {
        if !lines.is_empty() {
            lines.push(Line::default());
        }
        lines.push(Line::styled(*title, Style::default().fg(ACCENT).bold()));
        let mut notes = false;
        for (key, description) in *actions {
            if key.is_empty() && !notes {
                lines.push(Line::default());
                notes = true;
            }
            let mut line = Line::default();
            if !key.is_empty() {
                line.spans.push(Span::styled(
                    format!(" {key} "),
                    Style::default().fg(ACCENT).bg(PANEL),
                ));
            }
            // Wrap before rendering so scrolling uses the actual visible rows.
            for word in description.split_whitespace() {
                let prefix = if line.spans.is_empty() { "" } else { " " };
                if line.width() + prefix.len() + word.len() > width && !line.spans.is_empty() {
                    lines.push(line);
                    line = Line::from("  ");
                }
                let prefix = if line.spans.is_empty() { "" } else { " " };
                line.spans.push(Span::raw(format!("{prefix}{word}")));
            }
            lines.push(line);
        }
    }
    lines
}

pub(super) fn draw(frame: &mut Frame, app: &mut App) {
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
    let actions: &[(&str, &str)] = if app.error_prompt() {
        if matches!(app.view, View::Main) && app.delete.is_none() {
            &[("Esc", "quit")]
        } else {
            &[("Esc", "back")]
        }
    } else if app.browser_delete.is_some() {
        &[("Enter", "delete"), ("Esc", "cancel")]
    } else if let Some(name) = &app.delete {
        if app.config.entries[name].kind == MappingKind::Env
            && providers::get(&app.config.entries[name].source)
                .is_some_and(|p| p.author().is_some())
        {
            &[
                ("Enter", "delete"),
                ("Esc", "cancel"),
                ("Tab", "toggle ciphertext"),
            ]
        } else {
            &[("Enter", "delete"), ("Esc", "cancel")]
        }
    } else if matches!(&app.view, View::Create(c) if c.pending.is_some()) {
        &[("Enter", "replace"), ("Esc", "cancel")]
    } else {
        match &app.view {
            View::Main => &[
                ("Tab", "Entries/Files"),
                ("Type", "filter"),
                ("↑↓", "select"),
                ("Enter", "edit"),
                ("Ctrl+N", "new"),
                ("Ctrl+D", "delete"),
                ("Ctrl+R", "reveal/hide"),
                ("Ctrl+Y", "copy"),
                ("Esc", "quit"),
                ("F1", "help"),
            ],
            View::Create(_) => &[
                ("Tab", "fields"),
                ("Ctrl+S", "save encrypted"),
                ("Ctrl+R", "reveal/hide draft"),
                ("Ctrl+Y", "copy draft"),
                ("Enter", "newline / choose key"),
                ("Esc", "cancel"),
                ("F1", "help"),
                ("Ctrl+C", "quit"),
            ],
            View::Editor(_) => &[
                ("Tab", "fields"),
                ("Ctrl+S", "save"),
                ("Esc", "cancel"),
                ("Ctrl+B", "browse"),
                ("Ctrl+R", "reveal/hide"),
                ("Ctrl+Y", "copy"),
                ("F1", "help"),
                ("Ctrl+C", "quit"),
            ],
            View::Browser { editor, .. }
                if providers::get(&editor.source).is_some_and(|p| p.author().is_some()) =>
            {
                &[
                    ("Type", "filter"),
                    ("↑↓", "select"),
                    ("Enter", "select"),
                    ("Ctrl+N", "new value"),
                    ("Ctrl+D", "delete value"),
                    ("Ctrl+R", "reveal/hide"),
                    ("Ctrl+Y", "copy"),
                    ("Esc", "clear/up/back"),
                    ("F1", "help"),
                    ("Ctrl+C", "quit"),
                ]
            }
            View::Browser { .. } => &[
                ("Type", "filter"),
                ("↑↓", "select"),
                ("Enter", "choose"),
                ("Ctrl+R", "reveal/hide"),
                ("Ctrl+Y", "copy"),
                ("Esc", "clear/up/back"),
                ("F1", "help"),
                ("Ctrl+C", "quit"),
            ],
            View::Help => &[("↑↓", "scroll"), ("Esc", "back"), ("Ctrl+C", "quit")],
        }
    };
    let hints = shortcut_lines(actions, usize::from(area.width.saturating_sub(2)));
    let hint_height = hints.len() as u16;
    let help = matches!(app.view, View::Help);
    let compact_creator = matches!(app.view, View::Create(_)) && area.height < 24;
    let compact_editor = matches!(app.view, View::Editor(_)) && area.height < 24;
    let compact_list = matches!(app.view, View::Main | View::Browser { .. }) && area.height < 20;
    let compact = compact_creator || compact_editor || compact_list;
    let chunks = Layout::vertical([
        Constraint::Length(if compact || help { 1 } else { 3 }),
        Constraint::Min(if compact_creator { 4 } else { 3 }),
        Constraint::Length(if help {
            0
        } else if compact {
            1
        } else {
            2
        }),
        Constraint::Length(hint_height),
    ])
    .margin(if compact || help { 0 } else { 1 })
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
            let file = app.kind == MappingKind::File;
            let label = if file { "Files" } else { "Entries" };
            let names = app.names();
            app.selected = app.selected.min(names.len());
            app.table.select(Some(app.selected));
            let rows = names
                .iter()
                .map(|name| {
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
                })
                .chain(std::iter::once(Row::new(vec![
                    Cell::from(if file { "+ New file" } else { "+ New entry" })
                        .style(Style::default().fg(ACCENT).bold()),
                    Cell::from(""),
                    Cell::from(""),
                    Cell::from(""),
                ])));
            let title = if app.query.is_empty() {
                format!(" {label} · {} ", names.len())
            } else {
                format!(
                    " {label} · {} · Filter: {} ",
                    names.len(),
                    clean(&app.query)
                )
            };
            let widths = if area.width >= 100 {
                vec![
                    Constraint::Percentage(23),
                    Constraint::Percentage(14),
                    Constraint::Percentage(38),
                    Constraint::Percentage(25),
                ]
            } else {
                vec![
                    Constraint::Length(12),
                    Constraint::Percentage(18),
                    Constraint::Percentage(27),
                    Constraint::Min(0),
                ]
            };
            let table = Table::new(rows, widths)
                .header(if compact_list && chunks[1].height < 5 {
                    // Preserve a selectable row when wrapped hints leave
                    // too little room for both column headings and data.
                    Row::default().height(0)
                } else {
                    Row::new(if file {
                        ["DESTINATION", "SOURCE", "REFERENCE", "CONTENTS"]
                    } else {
                        ["NAME", "SOURCE", "REFERENCE", "SECRET"]
                    })
                    .style(Style::default().fg(MUTED))
                    .bottom_margin(u16::from(!compact_list))
                })
                .column_spacing(1)
                .block(block("").title_bottom(Line::from(title).fg(ACCENT)))
                .row_highlight_style(Style::default().bg(PANEL).fg(ACCENT))
                .highlight_symbol("› ");
            let content = Layout::vertical([
                Constraint::Length(if compact_list { 1 } else { 2 }),
                Constraint::Min(1),
            ])
            .split(chunks[1]);
            frame.render_widget(
                Paragraph::new(if names.is_empty() && app.query.is_empty() {
                    if file {
                        "Type to filter: · No files yet".into()
                    } else {
                        "Type to filter: · No secrets yet".into()
                    }
                } else {
                    format!("Type to filter: {}", clean(&app.query))
                })
                .style(Style::default().fg(ACCENT)),
                content[0],
            );
            frame.render_stateful_widget(table, content[1], &mut app.table);
            // Overlay actual Tabs on the table's top border, not beside branding.
            frame.render_widget(
                Tabs::new([" Entries ", " Files "])
                    .select(usize::from(file))
                    .padding("", "")
                    .divider(" · ")
                    .style(Style::default().fg(MUTED).bg(BG))
                    .highlight_style(Style::default().fg(BG).bg(ACCENT).bold()),
                Rect::new(
                    content[1].x + 2,
                    content[1].y,
                    content[1].width.saturating_sub(4).min(23),
                    u16::from(content[1].height > 0),
                ),
            );
        }
        View::Create(c) => {
            let inner = block(" Create encrypted value ");
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
                    Constraint::Length(if c.return_browser.is_some() { 0 } else { 3 }),
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Min(4),
                ])
                .split(body)
                .to_vec()
            };
            for (i, label) in [
                c.editor.name_label(),
                "Local storage ID",
                if c.editor.show {
                    "Exact UTF-8 value · revealed"
                } else {
                    "Exact UTF-8 value · masked"
                },
            ]
            .iter()
            .enumerate()
            {
                if (i == 0 && c.return_browser.is_some()) || (focused_only && c.field != i) {
                    continue;
                }
                let text = match i {
                    0 => clean(&c.editor.name),
                    1 => clean(&c.editor.value),
                    _ if c.editor.show => clean(&c.secret),
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
            let inner = block(match (e.kind, e.original.is_some()) {
                (MappingKind::File, true) => " Edit file mapping ",
                (MappingKind::File, false) => " Add file mapping ",
                (_, true) => " Edit secret ",
                (_, false) => " Add secret ",
            });
            let body = if compact_editor {
                chunks[1]
            } else {
                let body = inner.inner(chunks[1]);
                frame.render_widget(inner, chunks[1]);
                body
            };
            let fields = if compact_editor {
                let compact_fields =
                    Layout::vertical([Constraint::Length(3), Constraint::Min(3)]).split(body);
                let mut fields = vec![Rect::default(); 4];
                fields[e.field] = compact_fields[0];
                fields[3] = compact_fields[1];
                fields
            } else {
                Layout::vertical([
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Min(1),
                ])
                .split(body)
                .to_vec()
            };
            let provider = providers::get(&e.source);
            let ref_label = provider
                .map(|provider| provider.reference_label())
                .unwrap_or("Reference (unknown provider)");
            for (i, label) in [e.name_label(), "Source · ← / → to choose", ref_label]
                .into_iter()
                .enumerate()
            {
                if compact_editor && e.field != i {
                    continue;
                }
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
            let draft_only = providers::get(&e.source).is_some_and(|p| p.sensitive_reference());
            let preview = if draft_only {
                if e.show {
                    clean(&e.value)
                } else {
                    "Masked. Ctrl+R reveals draft value only.".into()
                }
            } else {
                e.preview.as_ref().map(|s| clean(s)).unwrap_or_else(|| {
                    "Hidden. Ctrl+R resolves on demand; may execute shell commands.".into()
                })
            };
            frame.render_widget(
                Paragraph::new(preview)
                    .style(
                        Style::default().fg(if e.preview.is_some() || (draft_only && e.show) {
                            ACCENT
                        } else {
                            MUTED
                        }),
                    )
                    .block(block(" Revealed value · auto-hides after 30s "))
                    .wrap(Wrap { trim: false }),
                fields[3],
            );
        }
        View::Browser { browser, editor } => {
            let can_create =
                providers::get(&editor.source).is_some_and(|provider| provider.author().is_some());
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
            browser.selected = browser.selected.min(if can_create {
                choices.len()
            } else {
                choices.len().saturating_sub(1)
            });
            let preview = if browser.loading {
                None
            } else {
                choices.get(browser.selected).and_then(|choice| {
                    let Target::Reference(reference) = &choice.target else {
                        return None;
                    };
                    if app.loading.contains(reference) {
                        Some("Fetching…".to_owned())
                    } else {
                        app.revealed.get(reference).and_then(|(value, at)| {
                            (at.elapsed() < std::time::Duration::from_secs(30))
                                .then(|| clean(value))
                        })
                    }
                })
            };
            let mut items = choices
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
            let filter_height = if compact_list { 1 } else { 2 };
            // Keep the preview short and reserve space for the list on small screens.
            let preview_height = if preview.is_some() {
                chunks[1].height.saturating_sub(filter_height + 3).min(6)
            } else {
                0
            };
            let preview_height = if preview_height >= 3 {
                preview_height
            } else {
                0
            };
            let content = Layout::vertical([
                Constraint::Length(filter_height),
                Constraint::Min(3),
                Constraint::Length(preview_height),
            ])
            .split(chunks[1]);
            frame.render_widget(
                Paragraph::new(format!("Type to filter: {}", clean(&browser.query)))
                    .style(Style::default().fg(ACCENT)),
                content[0],
            );
            if can_create && !browser.loading {
                items.push(ListItem::new(Line::from(Span::styled(
                    "+ New value",
                    Style::default().fg(ACCENT).bold(),
                ))));
            }
            if browser.loading || (choices.is_empty() && !can_create) {
                frame.render_widget(
                    Paragraph::new(if browser.loading {
                        "Loading…"
                    } else {
                        "No matches. Esc clears filter, goes up, then returns to editor."
                    })
                    .block(block(title))
                    .wrap(Wrap { trim: false }),
                    content[1],
                );
            } else {
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
            if let Some(preview) = preview
                && preview_height > 0
            {
                frame.render_widget(
                    Paragraph::new(preview)
                        .style(Style::default().fg(ACCENT))
                        .block(block(" Revealed value · auto-hides after 30s "))
                        .wrap(Wrap { trim: false }),
                    content[2],
                );
            }
        }
        View::Help => {
            let panel = block(" Help ");
            let body = panel.inner(chunks[1]);
            let lines = help_lines(usize::from(body.width));
            let max_scroll = lines.len().saturating_sub(usize::from(body.height)) as u16;
            app.help_scroll = app.help_scroll.min(max_scroll);
            frame.render_widget(panel, chunks[1]);
            frame.render_widget(Paragraph::new(lines).scroll((app.help_scroll, 0)), body);
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
            "Replace existing ciphertext?\nUpdates affect ALL references to this\n(ID,key) record.\nLoaded config only: {count}\npossible references.\nEnter replaces · Esc cancels"
        );
        frame.render_widget(
            Paragraph::new(warning)
                .block(block(" Confirm replacement "))
                .wrap(Wrap { trim: false }),
            popup,
        );
    }
    if let Some(deletion) = &app.browser_delete {
        let popup = centered(area, 78, 11);
        frame.render_widget(Clear, popup);
        let count = if let View::Browser { editor, .. } = &app.view {
            let author = providers::get(&editor.source).and_then(|p| p.author());
            app.config
                .entries
                .values()
                .filter(|entry| {
                    entry.source == editor.source
                        && matches!(&deletion.choice.target, Target::Reference(reference)
                        if author.is_some_and(|a| a.may_share(reference, &entry.value)))
                })
                .count()
        } else {
            0
        };
        let panel = block(" Delete encrypted value? ");
        let body = panel.inner(popup);
        frame.render_widget(panel, popup);
        // Keep metadata on two clipped lines so long or wide IDs cannot push
        // destructive-action warnings or consent controls out of the popup.
        let parts = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(4),
            Constraint::Length(1),
        ])
        .split(body);
        frame.render_widget(
            Paragraph::new(format!(
                "{}\n{}",
                clean(&deletion.choice.label),
                clean(&deletion.choice.detail),
            )),
            parts[0],
        );
        frame.render_widget(Paragraph::new(format!(
            "Mappings kept; references will fail.\nPossible references: {count}\nLoaded configuration only.\nOther configurations may also use it."
        )), parts[1]);
        frame.render_widget(Paragraph::new("Enter deletes · Esc cancels"), parts[2]);
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
        let author = (entry.kind == MappingKind::Env)
            .then(|| providers::get(&entry.source).and_then(|p| p.author()))
            .flatten();
        let options = if entry.kind == MappingKind::File {
            "\nDestination and provider contents kept.".to_owned()
        } else if let Some(author) = author {
            let shared = app.config.entries.iter().any(|(other, value)| {
                other != name
                    && value.source == entry.source
                    && author.may_share(&entry.value, &value.value)
            });
            format!(
                "\n[{}] Remove ciphertext\n{}{}",
                if app.delete_remove { "x" } else { " " },
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
        let panel = block("")
            .title(
                Line::from(" Confirm deletion ").style(Style::default().fg(Color::LightRed).bold()),
            )
            .border_style(Style::default().fg(Color::LightRed));
        let body = panel.inner(popup);
        frame.render_widget(panel, popup);
        let mut hints = shortcut_lines(actions, body.width.into());
        for line in &mut hints {
            for key in line.spans.iter_mut().step_by(2) {
                key.style = key.style.fg(Color::LightRed);
            }
        }
        let parts = Layout::vertical([Constraint::Min(1), Constraint::Length(hints.len() as u16)])
            .split(body);
        frame.render_widget(
            Paragraph::new(format!(
                "Delete {shown_name}?\n\nRemoves reference from this config.{options}"
            )),
            parts[0],
        );
        frame.render_widget(Paragraph::new(hints), parts[1]);
    }
    if app.error_prompt() {
        let popup = centered(area, 45, 7);
        frame.render_widget(Clear, popup);
        let panel = block("")
            .title(Line::from(" Error ").style(Style::default().fg(Color::LightRed).bold()))
            .border_style(Style::default().fg(Color::LightRed));
        let body = panel.inner(popup);
        frame.render_widget(panel, popup);
        let parts = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(body);
        frame.render_widget(
            Paragraph::new(app.status.as_str())
                .style(Style::default().fg(Color::LightRed))
                .wrap(Wrap { trim: false }),
            parts[0],
        );
        let mut hints = shortcut_lines(actions, body.width.into());
        for line in &mut hints {
            for key in line.spans.iter_mut().step_by(2) {
                key.style = key.style.fg(Color::LightRed);
            }
        }
        frame.render_widget(Paragraph::new(hints), parts[1]);
        return;
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
pub(super) fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        browser::Choice, config::Config, config::Entry, ui::app::Browser, ui::editor::Editor,
    };
    use ratatui::backend::TestBackend;
    use std::time::{Duration, Instant};
    use zeroize::Zeroizing;

    fn screen(app: &mut App) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .chunks(120)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect())
            .collect()
    }

    #[test]
    fn browser_preview_is_selected_short_lived_and_height_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::new(Config::load(dir.path().join("psst.json")).unwrap());
        app.view = View::Browser {
            editor: Editor::new(
                None,
                "DRAFT".into(),
                Entry {
                    kind: crate::config::MappingKind::Env,
                    source: "local-storage".into(),
                    value: String::new(),
                },
            ),
            browser: Browser {
                stack: vec![providers::get("local-storage").unwrap().browser().unwrap()],
                choices: ["selected", "other"]
                    .into_iter()
                    .map(|reference| Choice {
                        label: reference.into(),
                        detail: String::new(),
                        target: Target::Reference(reference.into()),
                    })
                    .collect(),
                query: String::new(),
                selected: 0,
                loading: false,
            },
        };
        app.revealed.insert(
            "selected".into(),
            (
                Zeroizing::new("VISIBLE_SECRET ".repeat(1000)),
                Instant::now(),
            ),
        );
        app.revealed.insert(
            "other".into(),
            (Zeroizing::new("UNSELECTED_SECRET".into()), Instant::now()),
        );
        let rows = screen(&mut app);
        let visible_rows = rows
            .iter()
            .filter(|row| row.contains("VISIBLE_SECRET"))
            .count();
        assert!((1..=4).contains(&visible_rows));
        assert!(!rows.join("\n").contains("UNSELECTED_SECRET"));

        app.loading.insert("selected".into());
        let text = screen(&mut app).join("\n");
        assert!(text.contains("Fetching…"));
        assert!(!text.contains("VISIBLE_SECRET"));
        app.loading.clear();
        app.revealed.get_mut("selected").unwrap().1 = Instant::now() - Duration::from_secs(31);
        let text = screen(&mut app).join("\n");
        assert!(!text.contains("VISIBLE_SECRET"));
        assert!(!text.contains("Revealed value"));

        if let View::Browser { browser, .. } = &mut app.view {
            browser.selected = 2;
        }
        assert!(!screen(&mut app).join("\n").contains("UNSELECTED_SECRET"));
    }

    #[test]
    fn dashboard_filter_is_always_visible_and_help_uses_unified_controls() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::new(Config::load(dir.path().join("psst.json")).unwrap());
        let dashboard = screen(&mut app).join("\n");
        assert!(dashboard.contains("Type to filter:"));
        assert!(dashboard.contains("Type  filter"));
        app.query = "QUERY".into();
        assert!(
            screen(&mut app)
                .join("\n")
                .contains("Type to filter: QUERY")
        );
        let help = help_lines(120)
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        for key in [
            "Ctrl+D", "Ctrl+Y", "Ctrl+R", "Ctrl+B", "Ctrl+S", "F1", "Ctrl+C",
        ] {
            assert!(help.contains(key));
        }
        for old in ["Ctrl+P", "Space", "q / Ctrl+C", "Erase filter; go up"] {
            assert!(!help.contains(old));
        }
    }
}
