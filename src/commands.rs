#[derive(Debug, PartialEq, Clone, Copy)]
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
    OpenInBrowser,
    PlayVideo,
    ToggleWatched,
    ToggleHelp,
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
            "open_in_browser" => Command::OpenInBrowser,
            "play_video" => Command::PlayVideo,
            "toggle_watched" => Command::ToggleWatched,
            "toggle_help" => Command::ToggleHelp,
            "quit" => Command::Quit,
            _ => anyhow::bail!("\"{}\" is an invalid command", command),
        };

        Ok(command)
    }
}
