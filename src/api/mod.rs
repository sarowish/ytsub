pub mod invidious;
pub mod local;

use crate::channel::Video;
use anyhow::Result;
use dyn_clone::DynClone;
use serde::Deserialize;
use std::fmt::Display;

#[derive(Deserialize, PartialEq)]
#[serde(rename_all(deserialize = "lowercase"))]
pub enum ChannelTab {
    Videos,
    Shorts,
    Streams,
}

#[derive(Deserialize)]
pub struct ChannelFeed {
    #[serde(rename = "title")]
    pub channel_title: Option<String>,
    #[serde(rename = "channelId")]
    pub channel_id: Option<String>,
    #[serde(rename = "entry")]
    pub videos: Vec<Video>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all(deserialize = "lowercase"))]
pub enum ApiBackend {
    Local,
    Invidious,
}

impl Display for ApiBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ApiBackend::Invidious => "Invidious",
                ApiBackend::Local => "Local",
            }
        )
    }
}

pub trait Api: Send + DynClone {
    fn get_videos_for_the_first_time(&mut self, channel_id: &str) -> Result<ChannelFeed>;
    fn get_videos_of_channel(&mut self, channel_id: &str) -> Result<ChannelFeed>;
    fn get_rss_feed_of_channel(&self, channel_id: &str) -> Result<ChannelFeed>;
}
