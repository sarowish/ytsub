use crate::{
    KEY_BINDINGS, OPTIONS,
    api::ApiBackend,
    app::{App, VideoPlayer},
    commands::{
        ChannelSelectionCommand, Command, FormatSelectionCommand, HelpCommand, ImportCommand,
        TagCommand,
    },
    help::HelpWindowState,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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

pub fn handle_event(key: KeyEvent, app: &mut App) -> bool {
    match app.input_mode {
        _ if app.help_window_state.show => {
            return handle_key_help_mode(key, &mut app.help_window_state);
        }
        InputMode::Normal => return handle_key_normal_mode(key, app),
        InputMode::Confirmation => handle_key_confirmation_mode(key, app),
        InputMode::Import => return handle_key_import_mode(key, app),
        InputMode::Tag => return handle_key_tag_mode(key, app),
        InputMode::ChannelSelection => return handle_key_channel_selection_mode(key, app),
        InputMode::FormatSelection => return handle_key_format_selection_mode(key, app),
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
            Command::SwitchApi => app.switch_api(),
            Command::RefreshChannel => app.refresh_channel(),
            Command::RefreshChannels => app.refresh_channels(),
            Command::RefreshFailedChannels => app.refresh_failed_channels(),
            Command::OpenInInvidious => app.open_in_browser(ApiBackend::Invidious),
            Command::OpenInYoutube => app.open_in_browser(ApiBackend::Local),
            Command::PlayFromFormats => app.play_from_formats(),
            Command::PlayUsingYtdlp => app.play_video(),
            Command::SelectFormats => app.enter_format_selection(),
            Command::ToggleWatched => app.toggle_watched(),
            Command::ToggleHelp => app.toggle_help(),
            Command::ToggleTag => app.toggle_tag_selection(),
            Command::Quit => return true,
        }
    }

    false
}

fn handle_key_help_mode(key: KeyEvent, help_window_state: &mut HelpWindowState) -> bool {
    if let Some(command) = KEY_BINDINGS.help.get(&key) {
        match command {
            HelpCommand::ScrollUp => help_window_state.scroll_up(),
            HelpCommand::ScrollDown => help_window_state.scroll_down(),
            HelpCommand::GoToTop => help_window_state.scroll_top(),
            HelpCommand::GoToBottom => help_window_state.scroll_bottom(),
            HelpCommand::Abort => help_window_state.toggle(),
        }
    } else if let Some(command) = KEY_BINDINGS.get(&key) {
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
    if let Some(command) = KEY_BINDINGS.import.get(&key) {
        match command {
            ImportCommand::ToggleSelection => app.import_state.toggle_selected(),
            ImportCommand::SelectAll => app.import_state.select_all(),
            ImportCommand::DeselectAll => app.import_state.deselect_all(),
            ImportCommand::Import => app.confirm_import(),
        }
    } else if let Some(command) = KEY_BINDINGS.get(&key) {
        match command {
            Command::OnDown => app.import_state.next(),
            Command::OnUp => app.import_state.previous(),
            Command::SelectFirst => app.import_state.select_first(),
            Command::SelectLast => app.import_state.select_last(),
            Command::SearchForward => app.search_forward(),
            Command::SearchBackward => app.search_backward(),
            Command::RepeatLastSearch => app.repeat_last_search(),
            Command::RepeatLastSearchOpposite => app.repeat_last_search_opposite(),
            Command::Quit => return true,
            _ => (),
        }
    }

    false
}

fn handle_key_tag_mode(key: KeyEvent, app: &mut App) -> bool {
    if let Some(command) = KEY_BINDINGS.tag.get(&key) {
        let mut updated = false;

        match command {
            TagCommand::ToggleSelection => {
                app.tags.toggle_selected();
                updated = true;
            }
            TagCommand::SelectAll => {
                app.tags.select_all();
                updated = true;
            }
            TagCommand::DeselectAll => {
                app.tags.deselect_all();
                updated = true;
            }
            TagCommand::SelectChannels => app.enter_channel_selection(),
            TagCommand::CreateTag => app.enter_tag_creation(),
            TagCommand::DeleteTag => app.delete_selected_tag(),
            TagCommand::RenameTag => app.enter_tag_renaming(),
            TagCommand::Abort => app.toggle_tag_selection(),
        }

        if updated {
            app.load_channels();
            app.channels.select_first();
            app.on_change_channel();
        }
    } else if let Some(command) = KEY_BINDINGS.get(&key) {
        match command {
            Command::OnDown => app.tags.next(),
            Command::OnUp => app.tags.previous(),
            Command::SelectFirst => app.tags.select_first(),
            Command::SelectLast => app.tags.select_last(),
            Command::SearchForward => app.search_forward(),
            Command::SearchBackward => app.search_backward(),
            Command::RepeatLastSearch => app.repeat_last_search(),
            Command::RepeatLastSearchOpposite => app.repeat_last_search_opposite(),
            Command::ToggleTag => app.toggle_tag_selection(),
            Command::Quit => return true,
            _ => (),
        }
    }

    false
}

fn handle_key_channel_selection_mode(key: KeyEvent, app: &mut App) -> bool {
    if let Some(command) = KEY_BINDINGS.channel_selection.get(&key) {
        match command {
            ChannelSelectionCommand::Confirm => app.update_tag(),
            ChannelSelectionCommand::Abort => app.input_mode = InputMode::Tag,
            ChannelSelectionCommand::ToggleSelection => app.channel_selection.toggle_selected(),
            ChannelSelectionCommand::SelectAll => app.channel_selection.select_all(),
            ChannelSelectionCommand::DeselectAll => app.channel_selection.deselect_all(),
        }
    } else if let Some(command) = KEY_BINDINGS.get(&key) {
        match command {
            Command::OnDown => app.channel_selection.next(),
            Command::OnUp => app.channel_selection.previous(),
            Command::SelectFirst => app.channel_selection.select_first(),
            Command::SelectLast => app.channel_selection.select_last(),
            Command::SearchForward => app.search_forward(),
            Command::SearchBackward => app.search_backward(),
            Command::RepeatLastSearch => app.repeat_last_search(),
            Command::RepeatLastSearchOpposite => app.repeat_last_search_opposite(),
            Command::Quit => return true,
            _ => (),
        }
    }

    false
}

fn handle_key_format_selection_mode(key: KeyEvent, app: &mut App) -> bool {
    if let Some(command) = KEY_BINDINGS.format_selection.get(&key) {
        match command {
            FormatSelectionCommand::PlayVideo => app.confirm_selected_streams(),
            FormatSelectionCommand::Abort => app.input_mode = InputMode::Normal,
            FormatSelectionCommand::Select => {
                let tab_index = app.stream_formats.selected_tab;
                let formats = app.stream_formats.get_mut_selected_tab();

                if tab_index == 2
                    && matches!(OPTIONS.video_player_for_stream_formats, VideoPlayer::Mpv)
                {
                    formats.toggle_selected();
                } else {
                    formats.select();
                }
            }
            FormatSelectionCommand::PreviousTab => app.stream_formats.previous_tab(),
            FormatSelectionCommand::NextTab => app.stream_formats.next_tab(),
            FormatSelectionCommand::SwitchFormatType => app.stream_formats.switch_format_type(),
        }
    } else if let Some(command) = KEY_BINDINGS.get(&key) {
        match command {
            Command::OnDown => app.stream_formats.get_mut_selected_tab().next(),
            Command::OnUp => app.stream_formats.get_mut_selected_tab().previous(),
            Command::SelectFirst => app.stream_formats.get_mut_selected_tab().select_first(),
            Command::SelectLast => app.stream_formats.get_mut_selected_tab().select_last(),
            Command::SearchForward => app.search_forward(),
            Command::SearchBackward => app.search_backward(),
            Command::RepeatLastSearch => app.repeat_last_search(),
            Command::RepeatLastSearchOpposite => app.repeat_last_search_opposite(),
            Command::Quit => return true,
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
            app.move_cursor_right();
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
        InputMode::TagCreation => app.create_tag(),
        InputMode::TagRenaming => app.rename_selected_tag(),
        _ => (),
    }
}

fn abort(app: &mut App) {
    match app.input_mode {
        InputMode::Subscribe | InputMode::TagCreation | InputMode::TagRenaming => {
            app.input_mode = app.prev_input_mode.clone();
            app.input.clear();
        }
        InputMode::Search => app.abort_search(),
        _ => (),
    }
}
