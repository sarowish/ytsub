#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Command {
    SetModeSubs,
    SetModeLatestVideos,
    OnDown,
    OnUp,
    OnLeft,
    OnRight,
    SelectFirst,
    SelectLast,
    NextTab,
    PreviousTab,
    JumpToChannel,
    ToggleHide,
    Subscribe,
    Unsubscribe,
    DeleteVideo,
    SearchForward,
    SearchBackward,
    RepeatLastSearch,
    RepeatLastSearchOpposite,
    SwitchApi,
    RefreshChannel,
    RefreshChannels,
    RefreshFailedChannels,
    LoadMoreVideos,
    LoadAllVideos,
    OpenInInvidious,
    OpenInYoutube,
    PlayFromFormats,
    PlayUsingYtdlp,
    SelectFormats,
    ToggleWatched,
    ToggleHelp,
    ToggleTag,
    Quit,
}

impl TryFrom<&str> for Command {
    type Error = anyhow::Error;

    fn try_from(command: &str) -> Result<Self, Self::Error> {
        let command = match command {
            "set_mode_subs" => Command::SetModeSubs,
            "set_mode_latest_videos" => Command::SetModeLatestVideos,
            "on_down" => Command::OnDown,
            "on_up" => Command::OnUp,
            "on_left" => Command::OnLeft,
            "on_right" => Command::OnRight,
            "select_first" => Command::SelectFirst,
            "select_last" => Command::SelectLast,
            "next_tab" => Command::NextTab,
            "previous_tab" => Command::PreviousTab,
            "jump_to_channel" => Command::JumpToChannel,
            "toggle_hide" => Command::ToggleHide,
            "subscribe" => Command::Subscribe,
            "unsubscribe" => Command::Unsubscribe,
            "delete_video" => Command::DeleteVideo,
            "search_forward" => Command::SearchForward,
            "search_backward" => Command::SearchBackward,
            "repeat_last_search" => Command::RepeatLastSearch,
            "repeat_last_search_opposite" => Command::RepeatLastSearchOpposite,
            "switch_api" => Command::SwitchApi,
            "refresh_channel" => Command::RefreshChannel,
            "refresh_channels" => Command::RefreshChannels,
            "refresh_failed_channels" => Command::RefreshFailedChannels,
            "load_more_videos" => Command::LoadMoreVideos,
            "load_all_videos" => Command::LoadAllVideos,
            "open_in_invidious" => Command::OpenInInvidious,
            "open_in_youtube" => Command::OpenInYoutube,
            "play_from_formats" => Command::PlayFromFormats,
            "play_using_ytdlp" => Command::PlayUsingYtdlp,
            "select_formats" => Command::SelectFormats,
            "toggle_watched" => Command::ToggleWatched,
            "toggle_help" => Command::ToggleHelp,
            "toggle_tag" => Command::ToggleTag,
            "quit" => Command::Quit,
            _ => anyhow::bail!("\"{}\" is an invalid command", command),
        };

        Ok(command)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ImportCommand {
    Import,
    ToggleSelection,
    SelectAll,
    DeselectAll,
}

impl TryFrom<&str> for ImportCommand {
    type Error = anyhow::Error;

    fn try_from(command: &str) -> Result<Self, Self::Error> {
        let command = match command {
            "toggle_selection" => ImportCommand::ToggleSelection,
            "select_all" => ImportCommand::SelectAll,
            "deselect_all" => ImportCommand::DeselectAll,
            "import" => ImportCommand::Import,
            _ => anyhow::bail!("\"{}\" is an invalid command", command),
        };

        Ok(command)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum TagCommand {
    CreateTag,
    DeleteTag,
    RenameTag,
    SelectChannels,
    ToggleSelection,
    SelectAll,
    DeselectAll,
    Abort,
}

impl TryFrom<&str> for TagCommand {
    type Error = anyhow::Error;

    fn try_from(command: &str) -> Result<Self, Self::Error> {
        let command = match command {
            "toggle_selection" => TagCommand::ToggleSelection,
            "select_all" => TagCommand::SelectAll,
            "deselect_all" => TagCommand::DeselectAll,
            "select_channels" => TagCommand::SelectChannels,
            "create_tag" => TagCommand::CreateTag,
            "delete_tag" => TagCommand::DeleteTag,
            "rename_tag" => TagCommand::RenameTag,
            "abort" => TagCommand::Abort,
            _ => anyhow::bail!("\"{}\" is an invalid command", command),
        };

        Ok(command)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ChannelSelectionCommand {
    Confirm,
    Abort,
    ToggleSelection,
    SelectAll,
    DeselectAll,
}

impl TryFrom<&str> for ChannelSelectionCommand {
    type Error = anyhow::Error;

    fn try_from(command: &str) -> Result<Self, Self::Error> {
        let command = match command {
            "confirm" => ChannelSelectionCommand::Confirm,
            "abort" => ChannelSelectionCommand::Abort,
            "toggle_selection" => ChannelSelectionCommand::ToggleSelection,
            "select_all" => ChannelSelectionCommand::SelectAll,
            "deselect_all" => ChannelSelectionCommand::DeselectAll,
            _ => anyhow::bail!("\"{}\" is an invalid command", command),
        };

        Ok(command)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum FormatSelectionCommand {
    PreviousTab,
    NextTab,
    SwitchFormatType,
    Select,
    PlayVideo,
    Abort,
}

impl TryFrom<&str> for FormatSelectionCommand {
    type Error = anyhow::Error;

    fn try_from(command: &str) -> Result<Self, Self::Error> {
        let command = match command {
            "previous_tab" => FormatSelectionCommand::PreviousTab,
            "next_tab" => FormatSelectionCommand::NextTab,
            "switch_format_type" => FormatSelectionCommand::SwitchFormatType,
            "select" => FormatSelectionCommand::Select,
            "play_video" => FormatSelectionCommand::PlayVideo,
            "abort" => FormatSelectionCommand::Abort,
            _ => anyhow::bail!("\"{}\" is an invalid command", command),
        };

        Ok(command)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum HelpCommand {
    ScrollUp,
    ScrollDown,
    GoToTop,
    GoToBottom,
    Abort,
}

impl TryFrom<&str> for HelpCommand {
    type Error = anyhow::Error;

    fn try_from(command: &str) -> Result<Self, Self::Error> {
        let command = match command {
            "scroll_up" => HelpCommand::ScrollUp,
            "scroll_down" => HelpCommand::ScrollDown,
            "go_to_top" => HelpCommand::GoToTop,
            "go_to_bottom" => HelpCommand::GoToBottom,
            "abort" => HelpCommand::Abort,
            _ => anyhow::bail!("\"{}\" is an invalid command", command),
        };

        Ok(command)
    }
}
