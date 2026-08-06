use super::ChannelFeed;
use crate::video::{FetchedVideo, Video};
use chrono::DateTime;
use serde::{Deserialize, de};

#[derive(Deserialize)]
struct RssChannelFeed {
    #[serde(rename = "title")]
    channel_title: Option<String>,
    #[serde(rename = "channelId")]
    channel_id: Option<String>,
    #[serde(rename = "entry", default)]
    videos: Vec<RssVideo>,
}

#[derive(Deserialize)]
struct RssVideo {
    #[serde(rename = "videoId")]
    video_id: String,
    title: String,
    #[serde(deserialize_with = "deserialize_published_date")]
    published: u64,
    length: Option<u32>,
}

fn deserialize_published_date<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: de::Deserializer<'de>,
{
    let date_str: &str = de::Deserialize::deserialize(deserializer)?;
    let date = DateTime::parse_from_rfc3339(date_str).map_err(de::Error::custom)?;

    Ok(date.timestamp().cast_unsigned())
}

pub fn parse(value: &str) -> Result<ChannelFeed, quick_xml::DeError> {
    let feed: RssChannelFeed = quick_xml::de::from_str(value)?;

    Ok(ChannelFeed {
        channel_title: feed.channel_title,
        channel_id: feed.channel_id,
        videos: feed
            .videos
            .into_iter()
            .map(|video| FetchedVideo {
                video: Video {
                    video_id: video.video_id,
                    title: video.title,
                    published: video.published,
                    length: video.length,
                    members_only: false,
                },
                published_text: None,
            })
            .collect(),
        live_streams: Vec::new(),
        shorts: Vec::new(),
    })
}
