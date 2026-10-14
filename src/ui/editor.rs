use crate::{config::Entry, providers};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

pub(super) struct Editor {
    pub(super) original: Option<String>,
    pub(super) name: String,
    pub(super) source: String,
    pub(super) value: Zeroizing<String>,
    pub(super) field: usize,
    pub(super) cursor: usize,
    pub(super) show: bool,
    pub(super) shown_at: Option<Instant>,
    pub(super) preview: Option<Zeroizing<String>>,
    pub(super) preview_at: Option<Instant>,
}
impl Editor {
    pub(super) fn new(original: Option<String>, name: String, entry: Entry) -> Self {
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
    pub(super) fn toggle_reveal(&mut self) {
        self.show = !self.show;
        self.shown_at = self.show.then(Instant::now);
    }
    pub(super) fn expire(&mut self) {
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
    pub(super) fn entry(&self) -> Entry {
        Entry {
            source: self.source.clone(),
            value: self.value.to_string(),
        }
    }
    pub(super) fn len(&self) -> usize {
        match self.field {
            0 => self.name.chars().count(),
            2 => self.value.chars().count(),
            _ => 0,
        }
    }
    pub(super) fn focus(&mut self, field: usize) {
        self.field = field;
        self.cursor = self.len();
    }
    pub(super) fn change_text(&mut self, input: &str) {
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
    pub(super) fn delete(&mut self, back: bool) {
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
pub(super) struct Creator {
    pub(super) editor: Editor,
    pub(super) secret: Zeroizing<String>,
    pub(super) field: usize,
    pub(super) cursor: usize,
    pub(super) keys: Vec<providers::Identity>,
    pub(super) selected: usize,
    pub(super) chosen: Option<usize>,
    pub(super) busy: bool,
    pub(super) pending: Option<Box<dyn providers::PreparedSave>>,
    pub(super) return_browser: Option<super::app::Browser>,
    pub(super) return_value: Option<Zeroizing<String>>,
}
impl Creator {
    pub(super) fn text(&self) -> &str {
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
    pub(super) fn focus(&mut self, field: usize) {
        self.field = field;
        self.cursor = self.text().chars().count();
    }
    pub(super) fn insert(&mut self, text: &str) {
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
    pub(super) fn delete(&mut self, back: bool) {
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
