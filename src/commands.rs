use crate::app::Mode;
use crate::search::SearchDirection;

#[derive(Debug, PartialEq)]
pub enum Command {
    SetMode(Mode),
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
    Search(SearchDirection),
    RepeatLastSearch(bool),
    RefreshChannel,
    RefreshChannels,
    RefreshFailedChannels,
    OpenInBrowser,
    PlayVideo,
    ToggleWatched,
    Quit,
}

impl TryFrom<&str> for Command {
    type Error = anyhow::Error;

    fn try_from(command: &str) -> Result<Self, Self::Error> {
        let command = match command {
            "set_mode_subs" => Command::SetMode(Mode::Subscriptions),
            "set_mode_latest_videos" => Command::SetMode(Mode::LatestVideos),
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
            "search_forward" => Command::Search(SearchDirection::Forward),
            "search_backward" => Command::Search(SearchDirection::Backward),
            "repeat_last_search" => Command::RepeatLastSearch(false),
            "repeat_last_search_opposite" => Command::RepeatLastSearch(true),
            "refresh_channel" => Command::RefreshChannel,
            "refresh_channels" => Command::RefreshChannels,
            "refresh_failed_channels" => Command::RefreshFailedChannels,
            "open_in_browser" => Command::OpenInBrowser,
            "play_video" => Command::PlayVideo,
            "toggle_watched" => Command::ToggleWatched,
            "quit" => Command::Quit,
            _ => anyhow::bail!("\"{}\" is an invalid command", command),
        };

        Ok(command)
    }
}
