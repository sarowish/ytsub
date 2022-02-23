use crate::app::{App, Mode};
use crossterm::event::{KeyCode, KeyEvent};

#[derive(Clone)]
pub enum InputMode {
    Normal,
    Editing,
}

pub fn handle_key_normal_mode(key: KeyEvent, app: &mut App) {
    match key.code {
        KeyCode::Char('1') => app.set_mode(Mode::Subscriptions),
        KeyCode::Char('2') => app.set_mode(Mode::LatestVideos),
        KeyCode::Char('j') => app.on_down(),
        KeyCode::Char('k') => app.on_up(),
        KeyCode::Char('h') => app.on_left(),
        KeyCode::Char('l') => app.on_right(),
        KeyCode::Char('g') => app.select_first(),
        KeyCode::Char('G') => app.select_last(),
        KeyCode::Char('t') => app.toggle_hide(),
        KeyCode::Char('/') => app.search_forward(),
        KeyCode::Char('?') => app.search_backward(),
        KeyCode::Char('n') => app.next_match(),
        KeyCode::Char('N') => app.prev_match(),
        KeyCode::Char('o') => {
            app.mark_as_watched();
            app.open_video_in_browser();
        }
        KeyCode::Char('p') => {
            app.mark_as_watched();
            app.play_video();
        }
        KeyCode::Char('m') => app.toggle_watched(),
        _ => {}
    }
}

pub fn handle_key_input_mode(key: KeyEvent, app: &mut App) {
    match key.code {
        KeyCode::Enter => {
            app.complete_search();
        }
        KeyCode::Char(c) => {
            app.push_key(c);
        }
        KeyCode::Backspace => {
            app.pop_key();
        }
        KeyCode::Esc => {
            app.abort_search();
        }
        _ => {}
    }
}
