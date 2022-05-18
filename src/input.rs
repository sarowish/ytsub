use crate::{
    app::{App, Mode},
    commands::Command,
    search::SearchDirection,
    KEY_BINDINGS,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone)]
pub enum InputMode {
    Normal,
    Subscribe,
    Search,
    Confirmation,
}

pub fn handle_key_normal_mode(key: KeyEvent, app: &mut App) {
    if let Some(command) = KEY_BINDINGS.get(&key) {
        match command {
            Command::SetMode(Mode::Subscriptions) => app.set_mode_subs(),
            Command::SetMode(Mode::LatestVideos) => app.set_mode_latest_videos(),
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
            Command::Search(SearchDirection::Forward) => app.search_forward(),
            Command::Search(SearchDirection::Backward) => app.search_backward(),
            Command::RepeatLastSearch(false) => app.repeat_last_search(),
            Command::RepeatLastSearch(true) => app.repeat_last_search_opposite(),
            Command::RefreshChannel => app.refresh_channel(),
            Command::RefreshChannels => app.refresh_channels(),
            Command::RefreshFailedChannels => app.refresh_failed_channels(),
            Command::OpenInBrowser => app.open_in_browser(),
            Command::PlayVideo => app.play_video(),
            Command::ToggleWatched => app.toggle_watched(),
            _ => (),
        }
    }
}

pub fn handle_key_confirmation_mode(key: KeyEvent, app: &mut App) {
    match key.code {
        KeyCode::Char('y') => app.unsubscribe(),
        KeyCode::Char('n') => app.input_mode = InputMode::Normal,
        _ => (),
    }
}

pub fn handle_key_editing_mode(key: KeyEvent, app: &mut App) {
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
