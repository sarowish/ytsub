use crate::app::App;
use crossterm::event::{KeyCode, KeyEvent};

pub fn handle_key(key: KeyEvent, app: &mut App) {
    match key.code {
        KeyCode::Char('j') => app.on_down(),
        KeyCode::Char('k') => app.on_up(),
        KeyCode::Char('h') => app.on_left(),
        KeyCode::Char('l') => app.on_right(),
        KeyCode::Char('g') => app.select_first(),
        KeyCode::Char('f') => app.select_last(),
        KeyCode::Char('n') => app.toggle_hide(),
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
