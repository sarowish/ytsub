use crate::KEY_BINDINGS;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::ops::{Deref, DerefMut};

const DESCRIPTIONS_LEN: usize = 30;
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
    "Switch API",
    "Refresh videos of the selected channel",
    "Refresh videos of every channel",
    "Refresh videos of channels which their latest refresh was a failure",
    "Open channel or video Invidious page in browser",
    "Open channel or video Youtube page in browser",
    "Play video in video player using stream formats",
    "Play video in mpv using yt-dlp",
    "Toggle format selection window",
    "Mark/unmark video as watched",
    "Toggle help window",
    "Toggle tag selection window",
    "Quit application",
];

const IMPORT_DESCRIPTIONS_LEN: usize = 4;
const IMPORT_DESCRIPTIONS: [&str; IMPORT_DESCRIPTIONS_LEN] = [
    " - Import, ",
    " - Toggle, ",
    " - Select all, ",
    " - Deselect all",
];

const TAG_DESCRIPTIONS_LEN: usize = 8;
const TAG_DESCRIPTIONS: [&str; TAG_DESCRIPTIONS_LEN] = [
    " - Create tag, ",
    " - Delete tag, ",
    " - Rename tag, ",
    " - Pick channels, ",
    " - Toggle, ",
    " - Select all, ",
    " - Deselect all, ",
    " - Abort",
];

const CHANNEL_SELECTION_DESCRIPTIONS_LEN: usize = 5;
const CHANNEL_SELECTION_DESCRIPTIONS: [&str; CHANNEL_SELECTION_DESCRIPTIONS_LEN] = [
    " - Confirm, ",
    " - Abort, ",
    " - Toggle, ",
    " - Select all, ",
    " - Deselect all",
];

const FORMAT_SELECTION_DESCRIPTIONS_LEN: usize = 6;
const FORMAT_SELECTION_DESCRIPTIONS: [&str; FORMAT_SELECTION_DESCRIPTIONS_LEN] = [
    " - Previous tab, ",
    " - Next tab, ",
    " - Switch format, ",
    " - Select, ",
    " - Play video, ",
    " - Abort",
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

pub struct Help<'a> {
    pub general: [(String, &'a str); DESCRIPTIONS_LEN],
    pub import: [(String, &'a str); IMPORT_DESCRIPTIONS_LEN],
    pub tag: [(String, &'a str); TAG_DESCRIPTIONS_LEN],
    pub channel_selection: [(String, &'a str); CHANNEL_SELECTION_DESCRIPTIONS_LEN],
    pub format_selection: [(String, &'a str); FORMAT_SELECTION_DESCRIPTIONS_LEN],
}

impl Default for Help<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl Help<'_> {
    pub fn new() -> Self {
        let mut help = Self {
            general: [HELP_ENTRY; DESCRIPTIONS_LEN],
            import: [HELP_ENTRY; IMPORT_DESCRIPTIONS_LEN],
            tag: [HELP_ENTRY; TAG_DESCRIPTIONS_LEN],
            channel_selection: [HELP_ENTRY; CHANNEL_SELECTION_DESCRIPTIONS_LEN],
            format_selection: [HELP_ENTRY; FORMAT_SELECTION_DESCRIPTIONS_LEN],
        };

        macro_rules! generate_entries {
            ($entries: expr, $bindings: expr, $descriptions: ident) => {
                for (key, command) in &$bindings {
                    let idx = *command as usize;

                    if !$entries[idx].0.is_empty() {
                        $entries[idx].0.push_str(", ");
                    }
                    $entries[idx].0.push_str(&key_event_to_string(key));
                }

                for (idx, (_, desc)) in $entries.iter_mut().enumerate() {
                    *desc = $descriptions[idx];
                }
            };
        }

        generate_entries!(help.general, KEY_BINDINGS.general, DESCRIPTIONS);
        generate_entries!(help.import, KEY_BINDINGS.import, IMPORT_DESCRIPTIONS);
        generate_entries!(help.tag, KEY_BINDINGS.tag, TAG_DESCRIPTIONS);
        generate_entries!(
            help.channel_selection,
            KEY_BINDINGS.channel_selection,
            CHANNEL_SELECTION_DESCRIPTIONS
        );
        generate_entries!(
            help.format_selection,
            KEY_BINDINGS.format_selection,
            FORMAT_SELECTION_DESCRIPTIONS
        );

        for (keys, _) in &mut help.general {
            *keys = format!("{keys:10}  ");
        }

        help
    }
}

impl<'a> Deref for Help<'a> {
    type Target = [(String, &'a str); DESCRIPTIONS_LEN];

    fn deref(&self) -> &Self::Target {
        &self.general
    }
}

impl DerefMut for Help<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.general
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
        KeyCode::Char(' ') => "space",
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
