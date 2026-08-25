use crate::list::ListItem;
use std::{
    fmt::Display,
    ops::{Deref, DerefMut},
};

pub struct Video {
    pub video_id: String,
    pub title: String,
    pub published: u64,
    pub length: Option<u32>,
    pub members_only: bool,
}

impl Video {
    pub fn needs_update(&self, other: &Self) -> bool {
        self.length != other.length || self.members_only != other.members_only
    }
}

pub struct FetchedVideo {
    pub video: Video,
    pub published_text: Option<String>,
}

impl Deref for FetchedVideo {
    type Target = Video;

    fn deref(&self) -> &Self::Target {
        &self.video
    }
}

impl DerefMut for FetchedVideo {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.video
    }
}

pub struct VideoListItem {
    pub video: Video,
    pub channel_id: String,
    pub channel_name: Option<String>,
    pub published_text: String,
    pub watched: bool,
    pub position: Option<u64>,
    pub is_new: bool,
}

impl VideoListItem {
    pub fn resume_position(&self) -> Option<u64> {
        const MIN_RESUME_REMAINING_SECONDS: u64 = 8;

        self.position.filter(|position| {
            *position > 0
                && self.length.is_none_or(|duration| {
                    u64::from(duration).saturating_sub(*position) > MIN_RESUME_REMAINING_SECONDS
                })
        })
    }

    pub fn progress_percentage(&self) -> Option<u8> {
        let position = self.resume_position()?;
        let length = u64::from(self.length?);

        Some(((position * 100) / length).min(100) as u8)
    }
}

impl Deref for VideoListItem {
    type Target = Video;

    fn deref(&self) -> &Self::Target {
        &self.video
    }
}

impl ListItem for VideoListItem {
    fn id(&self) -> &str {
        &self.video_id
    }
}

impl Display for VideoListItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(channel_name) = &self.channel_name {
            write!(f, "{} {}", channel_name, self.title)
        } else {
            write!(f, "{}", self.title)
        }
    }
}

#[derive(Default, Clone)]
pub struct VideoMetadata {
    pub video_id: String,
    pub title: String,
    pub channel: String,
}

#[derive(Default, Clone)]
pub struct PlaybackSpec {
    pub metadata: VideoMetadata,
    pub start_position: Option<u64>,
}
