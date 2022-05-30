use crate::commands::Command;
use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::Deserialize;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

#[derive(Deserialize)]
pub struct UserKeyBindings {
    key_bindings: HashMap<String, String>,
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

#[derive(PartialEq, Debug)]
pub struct KeyBindings {
    pub key_bindings: HashMap<KeyEvent, Command>,
}

impl Default for KeyBindings {
    fn default() -> Self {
        let mut key_bindings = HashMap::new();

        macro_rules! insert_binding {
            ($key: expr, $command: expr) => {
                key_bindings.insert(parse_binding($key).unwrap(), $command);
            };
        }

        insert_binding!("1", Command::SetModeSubs);
        insert_binding!("2", Command::SetModeLatestVideos);
        insert_binding!("j", Command::OnDown);
        insert_binding!("down", Command::OnDown);
        insert_binding!("k", Command::OnUp);
        insert_binding!("up", Command::OnUp);
        insert_binding!("h", Command::OnLeft);
        insert_binding!("left", Command::OnLeft);
        insert_binding!("l", Command::OnRight);
        insert_binding!("right", Command::OnRight);
        insert_binding!("g", Command::SelectFirst);
        insert_binding!("G", Command::SelectLast);
        insert_binding!("c", Command::JumpToChannel);
        insert_binding!("t", Command::ToggleHide);
        insert_binding!("i", Command::Subscribe);
        insert_binding!("d", Command::Unsubscribe);
        insert_binding!("/", Command::SearchForward);
        insert_binding!("?", Command::SearchBackward);
        insert_binding!("n", Command::RepeatLastSearch);
        insert_binding!("N", Command::RepeatLastSearchOpposite);
        insert_binding!("r", Command::RefreshChannel);
        insert_binding!("R", Command::RefreshChannels);
        insert_binding!("F", Command::RefreshFailedChannels);
        insert_binding!("o", Command::OpenInBrowser);
        insert_binding!("p", Command::PlayVideo);
        insert_binding!("m", Command::ToggleWatched);
        insert_binding!("ctrl-h", Command::ToggleHelp);
        insert_binding!("q", Command::Quit);
        insert_binding!("ctrl-c", Command::Quit);

        Self { key_bindings }
    }
}

impl TryFrom<UserKeyBindings> for KeyBindings {
    type Error = anyhow::Error;

    fn try_from(user_key_bindings: UserKeyBindings) -> Result<Self, Self::Error> {
        let mut key_bindings = KeyBindings::default();

        for (bindings, command) in user_key_bindings.key_bindings.iter() {
            for binding in bindings.split_whitespace() {
                let binding = parse_binding(binding)
                    .with_context(|| format!("Error: failed to parse binding \"{}\"", binding))?;
                if command.is_empty() {
                    key_bindings.remove(&binding);
                } else {
                    key_bindings.insert(
                        binding,
                        command.as_str().try_into().with_context(|| {
                            format!("Error: failed to parse command \"{}\"", command)
                        })?,
                    );
                }
            }
        }

        Ok(key_bindings)
    }
}

impl Deref for KeyBindings {
    type Target = HashMap<KeyEvent, Command>;

    fn deref(&self) -> &Self::Target {
        &self.key_bindings
    }
}

impl DerefMut for KeyBindings {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.key_bindings
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_binding, KeyBindings, UserKeyBindings};
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
        )
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
            key_bindings: HashMap::new(),
        };

        user_key_bindings
            .key_bindings
            .insert("l right".to_string(), "on_left".to_string());

        user_key_bindings
            .key_bindings
            .insert("esc".to_string(), "quit".to_string());

        let key_bindings = KeyBindings::try_from(user_key_bindings).unwrap();

        assert_eq!(
            *key_bindings
                .key_bindings
                .get(&KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE))
                .unwrap(),
            Command::OnLeft,
        );
        assert_eq!(
            *key_bindings
                .key_bindings
                .get(&KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
                .unwrap(),
            Command::OnLeft
        );
        assert_eq!(
            *key_bindings
                .key_bindings
                .get(&KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
                .unwrap(),
            Command::Quit
        );
    }

    #[test]
    fn remove_binding() {
        use std::collections::HashMap;

        let mut user_key_bindings = UserKeyBindings {
            key_bindings: HashMap::new(),
        };

        user_key_bindings
            .key_bindings
            .insert("q".to_string(), "".to_string());

        let key_bindings = KeyBindings::try_from(user_key_bindings).unwrap();

        assert!(key_bindings
            .key_bindings
            .get(&KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
            .is_none());
    }
}
