use chrono::DateTime;
use serde::{de, Deserialize};
use serde_json::Value;
use std::fmt::Display;

#[derive(Clone, Copy)]
pub enum RefreshState {
    ToBeRefreshed,
    Refreshing,
    Completed,
    Failed,
}

pub trait ListItem {
    fn id(&self) -> &str;
}

pub struct Channel {
    pub channel_id: String,
    pub channel_name: String,
    pub refresh_state: RefreshState,
    pub new_video: bool,
    pub last_refreshed: Option<u64>,
}

impl Channel {
    pub fn new(channel_id: String, channel_name: String, last_refreshed: Option<u64>) -> Self {
        Self {
            channel_id,
            channel_name,
            refresh_state: RefreshState::Completed,
            new_video: false,
            last_refreshed,
        }
    }

    pub fn set_to_be_refreshed(&mut self) {
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
        let refresh_indicator = match self.refresh_state {
            RefreshState::ToBeRefreshed => "□ ",
            RefreshState::Refreshing => "■ ",
            RefreshState::Completed => "",
            RefreshState::Failed => "✗ ",
        };
        let new_video_indicator = if self.new_video { " [N]" } else { "" };
        write!(
            f,
            "{}{}{}",
            refresh_indicator, self.channel_name, new_video_indicator
        )
    }
}

fn deserialize_published_date<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: de::Deserializer<'de>,
{
    let date_str: &str = de::Deserialize::deserialize(deserializer)?;
    let date = DateTime::parse_from_rfc3339(date_str).unwrap();

    Ok(date.timestamp() as u64)
}

pub enum VideoType {
    Subscriptions,
    LatestVideos(String),
}

#[derive(Deserialize)]
pub struct Video {
    #[serde(skip_deserializing)]
    pub video_type: Option<VideoType>,
    #[serde(rename = "videoId")]
    pub video_id: String,
    pub title: String,
    #[serde(deserialize_with = "deserialize_published_date")]
    pub published: u64,
    #[serde(skip_deserializing)]
    pub published_text: String,
    pub length: Option<u32>,
    #[serde(skip_deserializing)]
    pub watched: bool,
    #[serde(skip_deserializing)]
    pub new: bool,
}

impl Video {
    pub fn vec_from_json(videos_json: Value) -> Vec<Video> {
        videos_json
            .as_array()
            .unwrap()
            .iter()
            .map(Video::from)
            .collect()
    }
}

impl From<&Value> for Video {
    fn from(video_json: &Value) -> Self {
        let is_upcoming = video_json.get("isUpcoming").unwrap().as_bool().unwrap();
        Video {
            video_type: Default::default(),
            video_id: video_json
                .get("videoId")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
            title: video_json
                .get("title")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
            published: if is_upcoming {
                video_json
                    .get("premiereTimestamp")
                    .unwrap()
                    .as_u64()
                    .unwrap()
            } else {
                video_json.get("published").unwrap().as_u64().unwrap()
            },
            published_text: Default::default(),
            length: Some(video_json.get("lengthSeconds").unwrap().as_u64().unwrap() as u32),
            watched: false,
            new: true,
        }
    }
}

impl ListItem for Video {
    fn id(&self) -> &str {
        &self.video_id
    }
}

impl Display for Video {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.title)
    }
}
