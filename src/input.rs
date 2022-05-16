use crate::app::App;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone)]
pub enum InputMode {
    Normal,
    Subscribe,
    Search,
    Confirmation,
}

pub fn handle_key_normal_mode(key: KeyEvent, app: &mut App) {
    match key.code {
        KeyCode::Char('1') => app.set_mode_subs(),
        KeyCode::Char('2') => app.set_mode_latest_videos(),
        KeyCode::Char('j') | KeyCode::Down => app.on_down(),
        KeyCode::Char('k') | KeyCode::Up => app.on_up(),
        KeyCode::Char('h') | KeyCode::Left => app.on_left(),
        KeyCode::Char('l') | KeyCode::Right => app.on_right(),
        KeyCode::Char('g') => app.select_first(),
        KeyCode::Char('G') => app.select_last(),
        KeyCode::Char('c') => app.jump_to_channel(),
        KeyCode::Char('t') => app.toggle_hide(),
        KeyCode::Char('i') => app.prompt_for_subscription(),
        KeyCode::Char('d') => app.prompt_for_unsubscribing(),
        KeyCode::Char('/') => app.search_forward(),
        KeyCode::Char('?') => app.search_backward(),
        KeyCode::Char('n') => app.repeat_last_search(),
        KeyCode::Char('N') => app.repeat_last_search_opposite(),
        KeyCode::Char('r') => app.refresh_channel(),
        KeyCode::Char('R') => app.refresh_channels(),
        KeyCode::Char('F') => app.refresh_failed_channels(),
        KeyCode::Char('o') => app.open_in_browser(),
        KeyCode::Char('p') => app.play_video(),
        KeyCode::Char('m') => app.toggle_watched(),
        _ => {}
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
