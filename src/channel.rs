use chrono::DateTime;
use serde::{Deserialize, de};
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

#[derive(Deserialize)]
pub struct Video {
    #[serde(skip_deserializing)]
    pub channel_name: Option<String>,
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
        let is_upcoming = video_json["isUpcoming"].as_bool().unwrap();
        let mut published = video_json["published"].as_u64().unwrap();
        let mut length = video_json["lengthSeconds"].as_u64().unwrap();

        if is_upcoming {
            let premiere_timestamp = video_json["premiereTimestamp"].as_u64().unwrap();

            // In Invidious API, all shorts are marked as upcoming but the published key needs to be
            // used for the release time. If the premiere timestamp is 0, assume it is a shorts.
            if premiere_timestamp != 0 {
                published = premiere_timestamp;
            }
        }

        // In some Invidious instances length of shorts is 0
        if length == 0 {
            length = 60;
        }

        Video {
            channel_name: None,
            video_id: video_json["videoId"].as_str().unwrap().to_string(),
            title: video_json["title"].as_str().unwrap().to_string(),
            published,
            published_text: String::default(),
            length: Some(length as u32),
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
