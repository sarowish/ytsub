use crate::KEY_BINDINGS;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::ops::{Deref, DerefMut};

const DESCRIPTIONS_LEN: usize = 25;
const DESCRIPTIONS: [&str; DESCRIPTIONS_LEN] = [
    "Switch to subscriptions mode",
    "Switch to latest videos mode",
    "Go one line downward",
    "Go one line upward",
    "Switch to channels block",
    "Switch to videos block",
    "Jump to the first line",
    "Jump to the last line",
    "Jump to the channel of the selected video from latest videos mode",
    "Hide/unhide watched videos",
    "Subscribe",
    "Unsubscribe",
    "Delete the selected video from database",
    "Search Forward",
    "Search backward",
    "Repeat last search",
    "Repeat last search in the opposite direction",
    "Refresh videos of the selected channel",
    "Refresh videos of every channel",
    "Refresh videos of channels which their latest refresh was a failure",
    "Open channel or video in browser",
    "Play video in video player",
    "Mark/unmark video as watched",
    "Toggle help window",
    "Quit application",
];

pub struct HelpWindowState {
    pub show: bool,
    pub scroll: u16,
    pub max_scroll: u16,
}

impl HelpWindowState {
    pub fn new() -> Self {
        Self {
            show: false,
            scroll: 0,
            max_scroll: 0,
        }
    }

    pub fn toggle(&mut self) {
        self.show = !self.show;
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        self.scroll = std::cmp::min(self.scroll + 1, self.max_scroll);
    }

    pub fn scroll_top(&mut self) {
        self.scroll = 0;
    }

    pub fn scroll_bottom(&mut self) {
        self.scroll = self.max_scroll;
    }
}

const HELP_ENTRY: (String, &str) = (String::new(), "");

pub struct HelpWindow<'a>([(String, &'a str); DESCRIPTIONS_LEN]);

impl<'a> HelpWindow<'a> {
    pub fn new() -> Self {
        let mut help = HelpWindow([HELP_ENTRY; DESCRIPTIONS_LEN]);

        for (key, command) in KEY_BINDINGS.iter() {
            let idx = *command as usize;

            if !help[idx].0.is_empty() {
                help[idx].0.push_str(", ");
            }
            help[idx].0.push_str(&key_event_to_string(key));
        }

        for (idx, (key, desc)) in help.iter_mut().enumerate() {
            *key = format!("{:10}  ", key);
            *desc = DESCRIPTIONS[idx];
        }

        help
    }
}

impl<'a> Deref for HelpWindow<'a> {
    type Target = [(String, &'a str); DESCRIPTIONS_LEN];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> DerefMut for HelpWindow<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

fn key_event_to_string(key_event: &KeyEvent) -> String {
    let char;
    let key_code = match key_event.code {
        KeyCode::Backspace => "backspace",
        KeyCode::Enter => "enter",
        KeyCode::Left => "left",
        KeyCode::Right => "right",
        KeyCode::Up => "up",
        KeyCode::Down => "down",
        KeyCode::Home => "home",
        KeyCode::End => "end",
        KeyCode::PageUp => "pageup",
        KeyCode::PageDown => "pagedown",
        KeyCode::Tab => "tab",
        KeyCode::BackTab => "backtab",
        KeyCode::Delete => "delete",
        KeyCode::Insert => "insert",
        KeyCode::Char(c) if c == ' ' => "space",
        KeyCode::Char(c) => {
            char = c.to_string();
            &char
        }
        KeyCode::Esc => "esc",
        _ => "",
    };

    let mut modifiers = Vec::with_capacity(3);

    if key_event.modifiers.intersects(KeyModifiers::CONTROL) {
        modifiers.push("ctrl");
    }

    if key_event.modifiers.intersects(KeyModifiers::SHIFT) {
        modifiers.push("shift");
    }

    if key_event.modifiers.intersects(KeyModifiers::ALT) {
        modifiers.push("alt");
    }

    let mut key = modifiers.join("-");

    if !key.is_empty() {
        key.push('-');
    }
    key.push_str(key_code);

    key
}
