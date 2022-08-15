use crate::{app::App, commands::Command, help::HelpWindowState, KEY_BINDINGS};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone)]
pub enum InputMode {
    Normal,
    Subscribe,
    Search,
    Confirmation,
    Import,
}

pub fn handle_event(key: KeyEvent, app: &mut App) -> bool {
    match app.input_mode {
        _ if app.help_window_state.show => {
            return handle_key_help_mode(key, &mut app.help_window_state)
        }
        InputMode::Normal => return handle_key_normal_mode(key, app),
        InputMode::Confirmation => handle_key_confirmation_mode(key, app),
        InputMode::Import => return handle_key_import_mode(key, app),
        _ => handle_key_editing_mode(key, app),
    }

    false
}

fn handle_key_normal_mode(key: KeyEvent, app: &mut App) -> bool {
    if let Some(command) = KEY_BINDINGS.get(&key) {
        match command {
            Command::SetModeSubs => app.set_mode_subs(),
            Command::SetModeLatestVideos => app.set_mode_latest_videos(),
            Command::OnDown => app.on_down(),
            Command::OnUp => app.on_up(),
            Command::OnLeft => app.on_left(),
            Command::OnRight => app.on_right(),
            Command::SelectFirst => app.select_first(),
            Command::SelectLast => app.select_last(),
            Command::JumpToChannel => app.jump_to_channel(),
            Command::ToggleHide => app.toggle_hide(),
            Command::Subscribe => app.prompt_for_subscription(),
            Command::Unsubscribe => app.prompt_for_unsubscribing(),
            Command::DeleteVideo => app.delete_selected_video(),
            Command::SearchForward => app.search_forward(),
            Command::SearchBackward => app.search_backward(),
            Command::RepeatLastSearch => app.repeat_last_search(),
            Command::RepeatLastSearchOpposite => app.repeat_last_search_opposite(),
            Command::RefreshChannel => app.refresh_channel(),
            Command::RefreshChannels => app.refresh_channels(),
            Command::RefreshFailedChannels => app.refresh_failed_channels(),
            Command::OpenInBrowser => app.open_in_browser(),
            Command::PlayVideo => app.play_video(),
            Command::ToggleWatched => app.toggle_watched(),
            Command::ToggleHelp => app.toggle_help(),
            Command::Quit => return true,
        }
    }

    false
}

fn handle_key_help_mode(key: KeyEvent, help_window_state: &mut HelpWindowState) -> bool {
    if let Some(command) = KEY_BINDINGS.get(&key) {
        match command {
            Command::OnDown => help_window_state.scroll_down(),
            Command::OnUp => help_window_state.scroll_up(),
            Command::SelectFirst => help_window_state.scroll_top(),
            Command::SelectLast => help_window_state.scroll_bottom(),
            Command::ToggleHelp => help_window_state.toggle(),
            Command::Quit => return true,
            _ => (),
        }
    }

    false
}

fn handle_key_confirmation_mode(key: KeyEvent, app: &mut App) {
    match key.code {
        KeyCode::Char('y') => app.unsubscribe(),
        KeyCode::Char('n') => app.input_mode = InputMode::Normal,
        _ => (),
    }
}

fn handle_key_import_mode(key: KeyEvent, app: &mut App) -> bool {
    if let Some(command) = KEY_BINDINGS.get(&key) {
        match command {
            Command::OnDown => app.import_state.next(),
            Command::OnUp => app.import_state.previous(),
            Command::SelectFirst => app.import_state.select_first(),
            Command::SelectLast => app.import_state.select_last(),
            Command::Quit => return true,
            _ => (),
        }
    } else {
        match key.code {
            KeyCode::Char(' ') => app.import_state.toggle(),
            KeyCode::Char('a') => app.import_state.select_all(),
            KeyCode::Char('z') => app.import_state.deselect_all(),
            KeyCode::Enter => app.import_subscriptions(),
            _ => (),
        }
    }

    false
}

fn handle_key_editing_mode(key: KeyEvent, app: &mut App) {
    match (key.code, key.modifiers) {
        (KeyCode::Left, KeyModifiers::CONTROL) => app.move_cursor_one_word_left(),
        (KeyCode::Right, KeyModifiers::CONTROL) => app.move_cursor_one_word_right(),
        (KeyCode::Left, _) | (KeyCode::Char('b'), KeyModifiers::CONTROL) => app.move_cursor_left(),
        (KeyCode::Right, _) | (KeyCode::Char('f'), KeyModifiers::CONTROL) => {
            app.move_cursor_right()
        }
        (KeyCode::Char('a'), KeyModifiers::CONTROL) => app.move_cursor_to_beginning_of_line(),
        (KeyCode::Char('e'), KeyModifiers::CONTROL) => app.move_cursor_to_end_of_line(),
        (KeyCode::Char('w'), KeyModifiers::CONTROL) => app.delete_word_before_cursor(),
        (KeyCode::Char('u'), KeyModifiers::CONTROL) => app.clear_line(),
        (KeyCode::Char('k'), KeyModifiers::CONTROL) => app.clear_to_right(),
        (KeyCode::Enter, _) => complete(app),
        (KeyCode::Backspace, _) | (KeyCode::Char('h'), KeyModifiers::CONTROL) => app.pop_key(),
        (KeyCode::Char(c), _) => app.push_key(c),
        (KeyCode::Esc, _) => abort(app),
        _ => {}
    }
}

fn complete(app: &mut App) {
    match app.input_mode {
        InputMode::Subscribe => app.subscribe(),
        InputMode::Search => app.complete_search(),
        _ => (),
    }
}

fn abort(app: &mut App) {
    match app.input_mode {
        InputMode::Subscribe => {
            app.input_mode = InputMode::Normal;
            app.input.clear();
        }
        InputMode::Search => app.abort_search(),
        _ => (),
    }
}
