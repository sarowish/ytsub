use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::ops::RangeBounds;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

mod handlers;

pub use handlers::handle_event;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum InputChange {
    Append,
    Insert,
    Delete,
}

#[derive(Default)]
pub struct Input {
    text: String,
    prompt: String,
    idx: usize,
    cursor_position: u16,
}

impl Input {
    pub fn new(prompt: &str) -> Self {
        Self {
            prompt: prompt.to_owned(),
            ..Self::default()
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    pub fn set_text(&mut self, text: &str) {
        text.clone_into(&mut self.text);
        self.move_cursor_to_end_of_line();
    }

    pub fn take_text(&mut self) -> String {
        self.idx = 0;
        self.cursor_position = 0;
        std::mem::take(&mut self.text)
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.idx = 0;
        self.cursor_position = 0;
    }

    fn clear_range<R: RangeBounds<usize>>(&mut self, range: R) -> bool {
        self.text.drain(range).next().is_some()
    }

    fn insert_key(&mut self, ch: char) -> InputChange {
        let change = if self.idx == self.text.len() {
            self.text.push(ch);
            InputChange::Append
        } else {
            self.text.insert(self.idx, ch);
            InputChange::Insert
        };

        self.idx += ch.len_utf8();
        self.cursor_position += ch.width().unwrap() as u16;
        change
    }

    fn pop_key(&mut self) -> bool {
        if self.idx == 0 {
            return false;
        }

        let (idx, ch) = self.text[..self.idx]
            .grapheme_indices(true)
            .next_back()
            .unwrap();
        self.cursor_position -= ch.width() as u16;
        self.text.drain(idx..self.idx);
        self.idx = idx;
        true
    }

    fn move_cursor_left(&mut self) {
        if self.idx == 0 {
            return;
        }

        let (idx, ch) = self.text[..self.idx]
            .grapheme_indices(true)
            .next_back()
            .unwrap();
        self.idx = idx;
        self.cursor_position -= ch.width() as u16;
    }

    fn move_cursor_right(&mut self) {
        if self.idx == self.text.len() {
            return;
        }

        let (idx, ch) = self.text[self.idx..]
            .grapheme_indices(true)
            .next()
            .map(|(idx, ch)| (self.idx + idx + ch.len(), ch))
            .unwrap();
        self.idx = idx;
        self.cursor_position += ch.width() as u16;
    }

    fn move_cursor_one_word_left(&mut self) {
        let idx = self.text[..self.idx]
            .unicode_word_indices()
            .next_back()
            .map_or(0, |(idx, _)| idx);
        self.cursor_position -= self.text[idx..self.idx].width() as u16;
        self.idx = idx;
    }

    fn move_cursor_one_word_right(&mut self) {
        let old_idx = self.idx;
        self.idx = self.text[self.idx..]
            .unicode_word_indices()
            .nth(1)
            .map_or(self.text.len(), |(idx, _)| self.idx + idx);
        self.cursor_position += self.text[old_idx..self.idx].width() as u16;
    }

    fn move_cursor_to_beginning_of_line(&mut self) {
        self.idx = 0;
        self.cursor_position = 0;
    }

    fn move_cursor_to_end_of_line(&mut self) {
        self.idx = self.text.len();
        self.cursor_position = self.text.width() as u16;
    }

    fn delete_word_before_cursor(&mut self) -> bool {
        let old_idx = self.idx;
        self.move_cursor_one_word_left();
        self.clear_range(self.idx..old_idx)
    }

    fn clear_line(&mut self) -> bool {
        if self.text.is_empty() {
            return false;
        }

        self.clear();
        true
    }

    fn clear_to_right(&mut self) -> bool {
        self.clear_range(self.idx..)
    }

    fn update(&mut self, key: KeyEvent) -> Option<InputChange> {
        match (key.code, key.modifiers) {
            (KeyCode::Left, KeyModifiers::CONTROL) => self.move_cursor_one_word_left(),
            (KeyCode::Right, KeyModifiers::CONTROL) => self.move_cursor_one_word_right(),
            (KeyCode::Left, _) | (KeyCode::Char('b'), KeyModifiers::CONTROL) => {
                self.move_cursor_left();
            }
            (KeyCode::Right, _) | (KeyCode::Char('f'), KeyModifiers::CONTROL) => {
                self.move_cursor_right();
            }
            (KeyCode::Char('a'), KeyModifiers::CONTROL) => {
                self.move_cursor_to_beginning_of_line();
            }
            (KeyCode::Char('e'), KeyModifiers::CONTROL) => self.move_cursor_to_end_of_line(),
            (KeyCode::Char('w'), KeyModifiers::CONTROL) => {
                return self
                    .delete_word_before_cursor()
                    .then_some(InputChange::Delete);
            }
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                return self.clear_line().then_some(InputChange::Delete);
            }
            (KeyCode::Char('k'), KeyModifiers::CONTROL) => {
                return self.clear_to_right().then_some(InputChange::Delete);
            }
            (KeyCode::Backspace, _) | (KeyCode::Char('h'), KeyModifiers::CONTROL) => {
                return self.pop_key().then_some(InputChange::Delete);
            }
            (KeyCode::Char(ch), _) => return Some(self.insert_key(ch)),
            _ => {}
        }

        None
    }

    pub fn cursor_position(&self) -> u16 {
        self.prompt.width() as u16 + self.cursor_position
    }
}

#[derive(Clone)]
pub enum InputMode {
    Normal,
    Subscribe,
    Search,
    Confirmation,
    Import,
    Tag,
    TagCreation,
    TagRenaming,
    ChannelSelection,
    FormatSelection,
}
