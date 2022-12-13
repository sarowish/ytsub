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
    JumpToChannel,
    ToggleHide,
    Subscribe,
    Unsubscribe,
    DeleteVideo,
    SearchForward,
    SearchBackward,
    RepeatLastSearch,
    RepeatLastSearchOpposite,
    RefreshChannel,
    RefreshChannels,
    RefreshFailedChannels,
    OpenInInvidious,
    OpenInYoutube,
    PlayVideo,
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
            "jump_to_channel" => Command::JumpToChannel,
            "toggle_hide" => Command::ToggleHide,
            "subscribe" => Command::Subscribe,
            "unsubscribe" => Command::Unsubscribe,
            "delete_video" => Command::DeleteVideo,
            "search_forward" => Command::SearchForward,
            "search_backward" => Command::SearchBackward,
            "repeat_last_search" => Command::RepeatLastSearch,
            "repeat_last_search_opposite" => Command::RepeatLastSearchOpposite,
            "refresh_channel" => Command::RefreshChannel,
            "refresh_channels" => Command::RefreshChannels,
            "refresh_failed_channels" => Command::RefreshFailedChannels,
            "open_in_invidious" => Command::OpenInInvidious,
            "open_in_youtube" => Command::OpenInYoutube,
            "play_video" => Command::PlayVideo,
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
