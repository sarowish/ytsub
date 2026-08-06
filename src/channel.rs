use crate::{CONFIG, THEME, config::EnabledTabs, list::ListItem};
use bitflags::bitflags;
use ratatui::text::{Line, Span};
use serde::Deserialize;
use std::fmt::Display;

#[derive(Deserialize, PartialEq, Eq, Debug, Clone, Copy)]
#[serde(rename_all(deserialize = "lowercase"))]
pub enum ChannelTab {
    Videos,
    Shorts,
    Streams,
}

impl From<u8> for ChannelTab {
    fn from(value: u8) -> Self {
        match value {
            0b001 => Self::Videos,
            0b010 => Self::Shorts,
            0b100 => Self::Streams,
            _ => unreachable!("The function should only be used for `EnabledTabs` names."),
        }
    }
}

impl Display for ChannelTab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Videos => "videos",
                Self::Shorts => "shorts",
                Self::Streams => "streams",
            }
        )
    }
}

pub fn tabs_to_be_loaded() -> impl Iterator<Item = ChannelTab> {
    if CONFIG.hide_disabled_tabs {
        CONFIG.tabs.iter()
    } else {
        EnabledTabs::all().iter()
    }
    .map(|tab| tab.bits().into())
}

#[derive(Clone, Copy)]
pub enum RefreshState {
    ToBeRefreshed,
    Refreshing,
    Completed,
    Failed,
}

impl Display for RefreshState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let symbol = match self {
            Self::ToBeRefreshed => &CONFIG.to_be_refreshed_symbol,
            Self::Refreshing => &CONFIG.refreshing_symbol,
            Self::Completed => "",
            Self::Failed => &CONFIG.failed_symbol,
        };

        write!(f, "{symbol}")
    }
}

pub struct Channel {
    pub channel_id: String,
    pub channel_name: String,
    pub refresh_state: RefreshState,
    pub new_video: bool,
    pub last_refreshed: Option<u64>,
}

impl Channel {
    pub const fn new(
        channel_id: String,
        channel_name: String,
        last_refreshed: Option<u64>,
    ) -> Self {
        Self {
            channel_id,
            channel_name,
            refresh_state: RefreshState::Completed,
            new_video: false,
            last_refreshed,
        }
    }

    pub const fn set_to_be_refreshed(&mut self) {
        self.refresh_state = RefreshState::ToBeRefreshed;
    }
}

impl ListItem for Channel {
    fn id(&self) -> &str {
        &self.channel_id
    }
}

impl Display for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.channel_name)
    }
}

impl From<&Channel> for Line<'_> {
    fn from(value: &Channel) -> Self {
        Line::from(vec![
            Span::raw(format!("{}{}", value.refresh_state, value.channel_name)),
            Span::styled(
                if value.new_video { " [N]" } else { "" },
                THEME.new_video_indicator,
            ),
        ])
    }
}

bitflags! {
    pub struct HideVideos: u8 {
        const WATCHED      = 0b0001;
        const MEMBERS_ONLY = 0b0010;
    }
}
