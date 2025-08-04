use crate::commands::{
    ChannelSelectionCommand, Command, FormatSelectionCommand, HelpCommand, ImportCommand,
    TagCommand,
};
use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::Deserialize;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

#[derive(Deserialize)]
pub struct UserKeyBindings {
    #[serde(flatten)]
    general: Option<HashMap<String, String>>,
    import: Option<HashMap<String, String>>,
    tag: Option<HashMap<String, String>>,
    channel_selection: Option<HashMap<String, String>>,
    format_selection: Option<HashMap<String, String>>,
}

fn parse_binding(binding: &str) -> Result<KeyEvent> {
    let mut tokens = binding.rsplit('-');

    let code = if let Some(token) = tokens.next() {
        match token {
            "backspace" => KeyCode::Backspace,
            "space" => KeyCode::Char(' '),
            "enter" => KeyCode::Enter,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" => KeyCode::PageUp,
            "pagedown" => KeyCode::PageDown,
            "tab" => KeyCode::Tab,
            "backtab" => KeyCode::BackTab,
            "del" | "delete" => KeyCode::Delete,
            "insert" => KeyCode::Insert,
            "esc" | "escape" => KeyCode::Esc,
            token if token.len() == 1 => KeyCode::Char(token.chars().next().unwrap()),
            _ => anyhow::bail!("\"{}\" is not a valid key", token),
        }
    } else {
        anyhow::bail!("\"{}\" is not a valid binding", binding)
    };

    let mut modifiers = KeyModifiers::NONE;

    for token in tokens {
        match token {
            "ctrl" => modifiers.insert(KeyModifiers::CONTROL),
            "shift" => modifiers.insert(KeyModifiers::SHIFT),
            "alt" => modifiers.insert(KeyModifiers::ALT),
            _ => anyhow::bail!("\"{}\" is not a valid modifier", token),
        }
    }

    Ok(KeyEvent::new(code, modifiers))
}

#[derive(PartialEq, Eq, Debug)]
pub struct KeyBindings {
    pub general: HashMap<KeyEvent, Command>,
    pub help: HashMap<KeyEvent, HelpCommand>,
    pub import: HashMap<KeyEvent, ImportCommand>,
    pub tag: HashMap<KeyEvent, TagCommand>,
    pub channel_selection: HashMap<KeyEvent, ChannelSelectionCommand>,
    pub format_selection: HashMap<KeyEvent, FormatSelectionCommand>,
}

impl Default for KeyBindings {
    #[rustfmt::skip]
    fn default() -> Self {
        let mut general = HashMap::new();
        let mut help = HashMap::new();
        let mut import = HashMap::new();
        let mut tag = HashMap::new();
        let mut channel_selection = HashMap::new();
        let mut format_selection = HashMap::new();

        macro_rules! insert_binding {
            ($map: expr, $key: expr, $command: expr) => {
                $map.insert(parse_binding($key).unwrap(), $command);
            };
        }

        insert_binding!(general, "1", Command::SetModeSubs);
        insert_binding!(general, "2", Command::SetModeLatestVideos);
        insert_binding!(general, "j", Command::OnDown);
        insert_binding!(general, "down", Command::OnDown);
        insert_binding!(general, "k", Command::OnUp);
        insert_binding!(general, "up", Command::OnUp);
        insert_binding!(general, "h", Command::OnLeft);
        insert_binding!(general, "left", Command::OnLeft);
        insert_binding!(general, "l", Command::OnRight);
        insert_binding!(general, "right", Command::OnRight);
        insert_binding!(general, "g", Command::SelectFirst);
        insert_binding!(general, "G", Command::SelectLast);
        insert_binding!(general, "L", Command::NextTab);
        insert_binding!(general, "H", Command::PreviousTab);
        insert_binding!(general, "c", Command::JumpToChannel);
        insert_binding!(general, "t", Command::ToggleHide);
        insert_binding!(general, "i", Command::Subscribe);
        insert_binding!(general, "d", Command::Unsubscribe);
        insert_binding!(general, "D", Command::DeleteVideo);
        insert_binding!(general, "/", Command::SearchForward);
        insert_binding!(general, "?", Command::SearchBackward);
        insert_binding!(general, "n", Command::RepeatLastSearch);
        insert_binding!(general, "N", Command::RepeatLastSearchOpposite);
        insert_binding!(general, "s", Command::SwitchApi);
        insert_binding!(general, "r", Command::RefreshChannel);
        insert_binding!(general, "R", Command::RefreshChannels);
        insert_binding!(general, "F", Command::RefreshFailedChannels);
        insert_binding!(general, "J", Command::LoadMoreVideos);
        insert_binding!(general, "o", Command::OpenInYoutube);
        insert_binding!(general, "O", Command::OpenInInvidious);
        insert_binding!(general, "p", Command::PlayFromFormats);
        insert_binding!(general, "P", Command::PlayUsingYtdlp);
        insert_binding!(general, "f", Command::SelectFormats);
        insert_binding!(general, "m", Command::ToggleWatched);
        insert_binding!(general, "ctrl-h", Command::ToggleHelp);
        insert_binding!(general, "T", Command::ToggleTag);
        insert_binding!(general, "q", Command::Quit);
        insert_binding!(general, "ctrl-c", Command::Quit);

        insert_binding!(tag, "space", TagCommand::ToggleSelection);
        insert_binding!(tag, "a", TagCommand::SelectAll);
        insert_binding!(tag, "z", TagCommand::DeselectAll);
        insert_binding!(tag, "s", TagCommand::SelectChannels);
        insert_binding!(tag, "i", TagCommand::CreateTag);
        insert_binding!(tag, "d", TagCommand::DeleteTag);
        insert_binding!(tag, "r", TagCommand::RenameTag);
        insert_binding!(tag, "escape", TagCommand::Abort);

        insert_binding!(import, "space", ImportCommand::ToggleSelection);
        insert_binding!(import, "a", ImportCommand::SelectAll);
        insert_binding!(import, "z", ImportCommand::DeselectAll);
        insert_binding!(import, "enter", ImportCommand::Import);

        insert_binding!(channel_selection, "enter", ChannelSelectionCommand::Confirm);
        insert_binding!(channel_selection, "escape", ChannelSelectionCommand::Abort);
        insert_binding!(channel_selection, "space", ChannelSelectionCommand::ToggleSelection);
        insert_binding!(channel_selection, "a", ChannelSelectionCommand::SelectAll);
        insert_binding!(channel_selection, "z", ChannelSelectionCommand::DeselectAll);

        insert_binding!(format_selection, "l", FormatSelectionCommand::NextTab);
        insert_binding!(format_selection, "right", FormatSelectionCommand::NextTab);
        insert_binding!(format_selection, "h", FormatSelectionCommand::PreviousTab);
        insert_binding!(format_selection, "left", FormatSelectionCommand::PreviousTab);
        insert_binding!(format_selection, "s", FormatSelectionCommand::SwitchFormatType);
        insert_binding!(format_selection, "space", FormatSelectionCommand::Select);
        insert_binding!(format_selection, "enter", FormatSelectionCommand::PlayVideo);
        insert_binding!(format_selection, "escape", FormatSelectionCommand::Abort);

        insert_binding!(help, "ctrl-y", HelpCommand::ScrollUp);
        insert_binding!(help, "ctrl-e", HelpCommand::ScrollDown);
        insert_binding!(help, "g", HelpCommand::GoToTop);
        insert_binding!(help, "G", HelpCommand::GoToBottom);
        insert_binding!(help, "esc", HelpCommand::Abort);

        Self {
            general,
            help,
            import,
            tag,
            channel_selection,
            format_selection
        }
    }
}

fn set_bindings<'a, T, E>(
    key_bindings: &mut HashMap<KeyEvent, T>,
    user_key_bindings: &'a HashMap<String, String>,
) -> Result<(), anyhow::Error>
where
    T: TryFrom<&'a str, Error = E>,
    E: Into<anyhow::Error>,
{
    for (bindings, command) in user_key_bindings {
        for binding in bindings.split_whitespace() {
            let binding = parse_binding(binding)
                .with_context(|| format!("Error: failed to parse binding \"{binding}\""))?;
            if command.is_empty() {
                key_bindings.remove(&binding);
            } else {
                key_bindings.insert(
                    binding,
                    T::try_from(command.as_str())
                        .map_err(|e| anyhow::anyhow!(e))
                        .with_context(|| format!("Error: failed to parse command \"{command}\""))?,
                );
            }
        }
    }

    Ok(())
}

impl TryFrom<UserKeyBindings> for KeyBindings {
    type Error = anyhow::Error;

    fn try_from(user_key_bindings: UserKeyBindings) -> Result<Self, Self::Error> {
        let mut key_bindings = KeyBindings::default();

        if let Some(bindings) = user_key_bindings.general {
            set_bindings(&mut key_bindings, &bindings)?;
        }

        if let Some(bindings) = user_key_bindings.import {
            set_bindings(&mut key_bindings.import, &bindings)?;
        }

        if let Some(bindings) = user_key_bindings.tag {
            set_bindings(&mut key_bindings.tag, &bindings)?;
        }

        if let Some(bindings) = user_key_bindings.channel_selection {
            set_bindings(&mut key_bindings.channel_selection, &bindings)?;
        }

        if let Some(bindings) = user_key_bindings.format_selection {
            set_bindings(&mut key_bindings.format_selection, &bindings)?;
        }

        Ok(key_bindings)
    }
}

impl Deref for KeyBindings {
    type Target = HashMap<KeyEvent, Command>;

    fn deref(&self) -> &Self::Target {
        &self.general
    }
}

impl DerefMut for KeyBindings {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.general
    }
}

#[cfg(test)]
mod tests {
    use super::{KeyBindings, UserKeyBindings, parse_binding};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn valid_bindings() {
        assert_eq!(
            parse_binding("s").unwrap(),
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)
        );
        assert_eq!(
            parse_binding("up").unwrap(),
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)
        );
        assert_eq!(
            parse_binding("ctrl-s").unwrap(),
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)
        );
        assert_eq!(
            parse_binding("shift-ctrl-s").unwrap(),
            KeyEvent::new(
                KeyCode::Char('s'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            )
        );
        assert_eq!(
            parse_binding("shift-alt-left").unwrap(),
            KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT | KeyModifiers::ALT)
        );
    }

    #[test]
    #[should_panic]
    fn no_key_code() {
        parse_binding("ctrl-shift").unwrap();
    }

    #[test]
    #[should_panic]
    fn multiple_key_codes() {
        parse_binding("esc-s").unwrap();
    }

    #[test]
    #[should_panic]
    fn invalid_token() {
        parse_binding("rust").unwrap();
    }

    #[test]
    fn configure_bindings() {
        use crate::commands::Command;
        use std::collections::HashMap;

        let mut user_key_bindings = UserKeyBindings {
            general: Some(HashMap::new()),
            import: None,
            tag: None,
            channel_selection: None,
            format_selection: None,
        };

        let general_bindings = user_key_bindings.general.as_mut().unwrap();

        general_bindings.insert("l right".to_string(), "on_left".to_string());

        general_bindings.insert("esc".to_string(), "quit".to_string());

        let key_bindings = KeyBindings::try_from(user_key_bindings).unwrap();

        assert_eq!(
            *key_bindings
                .general
                .get(&KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE))
                .unwrap(),
            Command::OnLeft,
        );
        assert_eq!(
            *key_bindings
                .general
                .get(&KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
                .unwrap(),
            Command::OnLeft
        );
        assert_eq!(
            *key_bindings
                .general
                .get(&KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
                .unwrap(),
            Command::Quit
        );
    }

    #[test]
    fn remove_binding() {
        use std::collections::HashMap;

        let mut user_key_bindings = UserKeyBindings {
            general: Some(HashMap::new()),
            import: None,
            tag: None,
            channel_selection: None,
            format_selection: None,
        };

        user_key_bindings
            .general
            .as_mut()
            .unwrap()
            .insert("q".to_string(), String::new());

        let key_bindings = KeyBindings::try_from(user_key_bindings).unwrap();

        assert!(
            !key_bindings
                .general
                .contains_key(&KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
        );
    }
}
