pub mod invidious;
pub mod local;

use crate::channel::{ListItem, Video};
use anyhow::Result;
use dyn_clone::DynClone;
use serde::Deserialize;
use serde_json::Value;
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

pub struct VideoInfo {
    pub video_formats: Vec<Format>,
    pub audio_formats: Vec<Format>,
    pub format_streams: Vec<Format>,
    pub captions: Vec<Format>,
}

impl VideoInfo {
    pub fn new(
        video_formats: Vec<Format>,
        audio_formats: Vec<Format>,
        format_streams: Vec<Format>,
        captions: Vec<Format>,
    ) -> Self {
        Self {
            video_formats,
            audio_formats,
            format_streams,
            captions,
        }
    }
}

pub enum Format {
    Video {
        url: String,
        quality: String,
        fps: u64,
        r#type: String,
    },
    Audio {
        url: String,
        bitrate: String,
        language: Option<(String, bool)>,
        r#type: String,
    },
    Stream {
        url: String,
        quality: String,
        fps: u64,
        bitrate: Option<String>,
        r#type: String,
    },
    Caption {
        url: String,
        label: String,
        language_code: String,
    },
}

impl Format {
    pub fn from_video(format_json: &Value, api_backend: ApiBackend) -> Self {
        let mime_type = match api_backend {
            ApiBackend::Local => &format_json["mimeType"],
            ApiBackend::Invidious => &format_json["type"],
        };

        Format::Video {
            url: format_json["url"].as_str().unwrap().to_string(),
            quality: format_json["qualityLabel"].as_str().unwrap().to_string(),
            fps: format_json["fps"].as_u64().unwrap(),
            r#type: mime_type.as_str().unwrap().to_string(),
        }
    }

    pub fn from_audio(format_json: &Value, api_backend: ApiBackend) -> Self {
        let mime_type;
        let bitrate;

        match api_backend {
            ApiBackend::Local => {
                mime_type = &format_json["mimeType"];
                bitrate = format_json["bitrate"].as_u64().unwrap().to_string();
            }
            ApiBackend::Invidious => {
                mime_type = &format_json["type"];
                bitrate = format_json["bitrate"].as_str().unwrap().to_string();
            }
        };

        let language = format_json.get("audioTrack").map(|audio_track| {
            (
                audio_track["displayName"].as_str().unwrap().to_string(),
                audio_track["audioIsDefault"].as_bool().unwrap(),
            )
        });

        Format::Audio {
            url: format_json["url"].as_str().unwrap().to_string(),
            bitrate,
            r#type: mime_type.as_str().unwrap().to_string(),
            language,
        }
    }

    pub fn from_stream(format_json: &Value, api_backend: ApiBackend) -> Self {
        let (mime_type, bitrate) = match api_backend {
            ApiBackend::Local => (
                &format_json["mimeType"],
                Some(format_json["audioSampleRate"].as_str().unwrap().to_string()),
            ),
            ApiBackend::Invidious => (&format_json["type"], None),
        };

        Format::Stream {
            url: format_json["url"].as_str().unwrap().to_string(),
            quality: format_json["qualityLabel"].as_str().unwrap().to_string(),
            fps: format_json["fps"].as_u64().unwrap(),
            bitrate,
            r#type: mime_type.as_str().unwrap().to_string(),
        }
    }

    pub fn from_caption(format_json: &Value, api_backend: ApiBackend) -> Option<Self> {
        let caption = match api_backend {
            ApiBackend::Local => Format::Caption {
                url: format_json["baseUrl"].as_str().unwrap().to_string(),
                label: format_json["name"]["runs"][0]["text"]
                    .as_str()
                    .unwrap()
                    .to_string(),
                language_code: format_json["languageCode"].as_str().unwrap().to_string(),
            },
            ApiBackend::Invidious => Format::Caption {
                url: format_json["url"].as_str().unwrap().to_string(),
                label: format_json["label"].as_str().unwrap().to_string(),
                language_code: format_json["language_code"].as_str().unwrap().to_string(),
            },
        };

        if matches!(&caption, Format::Caption { label, .. } if label.contains("auto-generated")) {
            return None;
        }

        Some(caption)
    }

    pub fn get_url(&self) -> &str {
        match self {
            Format::Video { url, .. }
            | Format::Audio { url, .. }
            | Format::Stream { url, .. }
            | Format::Caption { url, .. } => url,
        }
    }

    pub fn get_quality(&self) -> &str {
        if let Format::Video { quality, .. } = self {
            quality
        } else {
            panic!()
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all(deserialize = "lowercase"))]
pub enum PreferredVideoFormat {
    WebM,
    Mp4,
}

impl Display for PreferredVideoFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                PreferredVideoFormat::Mp4 => "mp4",
                PreferredVideoFormat::WebM => "webm",
            }
        )
    }
}

impl ListItem for Format {
    fn id(&self) -> &str {
        match self {
            Format::Video { url, .. } | Format::Audio { url, .. } | Format::Stream { url, .. } => {
                url
            }
            Format::Caption { language_code, .. } => language_code,
        }
    }
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
    fn get_video_formats(&self, video_id: &str) -> Result<VideoInfo>;
}
