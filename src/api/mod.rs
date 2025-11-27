pub mod invidious;
pub mod local;

use crate::{
    OPTIONS,
    channel::{ChannelTab, ListItem, Video},
    protobuf::decode_protobuf,
    stream_formats::Formats,
    utils,
};
use anyhow::Result;
use async_trait::async_trait;
use dyn_clone::DynClone;
use regex_lite::Regex;
use serde::Deserialize;
use serde_json::Value;
use std::{collections::HashSet, fmt::Display, io::Write, path::PathBuf, sync::LazyLock};

#[derive(Default, Deserialize)]
pub struct ChannelFeed {
    #[serde(rename = "title")]
    pub channel_title: Option<String>,
    #[serde(rename = "channelId")]
    pub channel_id: Option<String>,
    #[serde(rename = "entry")]
    pub videos: Vec<Video>,
    pub live_streams: Vec<Video>,
    pub shorts: Vec<Video>,
}

impl ChannelFeed {
    pub fn new(channel_id: &str) -> Self {
        Self {
            channel_id: Some(channel_id.to_owned()),
            ..Self::default()
        }
    }

    pub fn channel_title(mut self, channel_title: Option<String>) -> Self {
        self.channel_title = channel_title;
        self
    }

    pub fn videos(mut self, videos: Vec<Video>) -> Self {
        self.videos = videos;
        self
    }

    pub fn get_videos(&self, tab: ChannelTab) -> &[Video] {
        match tab {
            ChannelTab::Videos => &self.videos,
            ChannelTab::Shorts => &self.shorts,
            ChannelTab::Streams => &self.live_streams,
        }
    }

    pub fn get_mut_videos(&mut self, tab: ChannelTab) -> &mut Vec<Video> {
        match tab {
            ChannelTab::Videos => &mut self.videos,
            ChannelTab::Shorts => &mut self.shorts,
            ChannelTab::Streams => &mut self.live_streams,
        }
    }

    pub fn extend_videos(&mut self, videos: Vec<Video>, tab: ChannelTab) {
        let Some(published_of_first) = videos.first().map(|video| video.published_text.clone())
        else {
            return;
        };

        let present_videos = self.get_mut_videos(tab);
        present_videos.extend(videos);

        present_videos
            .iter_mut()
            .filter(|video| video.published_text == published_of_first)
            .for_each(|video| {
                if let Ok(published) = utils::published(&video.published_text) {
                    video.published = published;
                }
            });
    }
}

pub struct VideoInfo {
    pub video_formats: Vec<Format>,
    pub audio_formats: Vec<Format>,
    pub format_streams: Vec<Format>,
    pub captions: Vec<Format>,
    pub chapters: Option<Chapters>,
}

impl VideoInfo {
    pub fn new(
        video_formats: Vec<Format>,
        mut audio_formats: Vec<Format>,
        format_streams: Vec<Format>,
        captions: Vec<Format>,
        chapters: Option<Chapters>,
    ) -> Self {
        audio_formats.reverse();

        Self {
            video_formats,
            audio_formats,
            format_streams,
            captions,
            chapters,
        }
    }
}

fn extract_track_type(format: &Value) -> Option<String> {
    format
        .get("xtags")
        .and_then(Value::as_str)
        .and_then(decode_protobuf)
        .and_then(|v| v["1"][0]["2"].as_str().map(ToOwned::to_owned))
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
        let url = format_json["url"].as_str().unwrap().to_string();
        let mime_type;
        let bitrate;
        let language;

        match api_backend {
            ApiBackend::Local => {
                mime_type = &format_json["mimeType"];
                bitrate = format_json["bitrate"].as_u64().unwrap().to_string();
                language = format_json.get("audioTrack").map(|audio_track| {
                    (
                        audio_track["displayName"].as_str().unwrap().to_string(),
                        OPTIONS
                            .prefer_original_audio
                            .then(|| extract_track_type(format_json).map(|s| s == "original"))
                            .flatten()
                            .or(audio_track["audioIsDefault"].as_bool())
                            .unwrap_or_default(),
                    )
                });
            }
            ApiBackend::Invidious => {
                mime_type = &format_json["type"];
                bitrate = format_json["bitrate"].as_str().unwrap().to_string();
                let mut default = None;
                let mut lang = None;

                if let Some(param) = utils::params_from_url(&url)
                    .ok()
                    .as_mut()
                    .and_then(|params| params.remove("xtags"))
                {
                    param
                        .split(':')
                        .filter_map(|param| param.split_once('='))
                        .for_each(|(key, value)| match key {
                            "acont" => default = Some(value == "original"),
                            "lang" => lang = Some(value.to_owned()),
                            _ => {}
                        });
                };

                language = lang.zip(default);
            }
        }

        Format::Audio {
            url,
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

    pub fn get_quality(&self) -> u16 {
        if let Format::Video { quality, .. } = self {
            quality
                .split_once('p')
                .and_then(|(quality, _)| quality.parse().ok())
                .unwrap_or_default()
        } else {
            panic!()
        }
    }

    pub fn get_codec(&self) -> VideoFormat {
        static RE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"(video|audio)\/(?<codec>webm|mp4);").unwrap());

        let (Format::Video { r#type, .. }
        | Format::Audio { r#type, .. }
        | Format::Stream { r#type, .. }) = self
        else {
            unreachable!()
        };

        let Some(captures) = RE.captures(r#type) else {
            return VideoFormat::Mp4;
        };

        match &captures["codec"] {
            "mp4" => VideoFormat::Mp4,
            "webm" => VideoFormat::WebM,
            _ => unreachable!(),
        }
    }
}

#[derive(Deserialize, Eq, PartialEq)]
#[serde(rename_all(deserialize = "lowercase"))]
pub enum VideoFormat {
    WebM,
    Mp4,
}

impl Display for VideoFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                VideoFormat::Mp4 => "mp4",
                VideoFormat::WebM => "webm",
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

#[derive(Default)]
pub struct Chapters {
    inner: Vec<Chapter>,
}

impl Chapters {
    pub fn write_to_file(&self, video_id: &str) -> Result<PathBuf> {
        let path = utils::get_cache_dir()?.join(format!("{video_id}.ffmetadata"));

        if let Ok(true) = path.try_exists() {
            return Ok(path);
        }

        let mut file = std::fs::File::create(&path)?;

        writeln!(file, ";FFMETADATA1")?;

        for chapter in &self.inner {
            writeln!(file, "[CHAPTER]")?;
            writeln!(file, "TIMEBASE=1/1")?;
            writeln!(file, "START={}", chapter.start)?;
            writeln!(file, "END={}", chapter.end)?;
            writeln!(file, "TITLE={}", chapter.title)?;
        }

        Ok(path)
    }
}

impl TryFrom<Option<&str>> for Chapters {
    type Error = anyhow::Error;

    fn try_from(value: Option<&str>) -> std::result::Result<Self, Self::Error> {
        let Some(description) = value else {
            return Err(anyhow::anyhow!("There is no description"));
        };

        let mut chapters = description
            .lines()
            .filter_map(|line| Chapter::try_from(line).ok())
            .collect::<Vec<_>>();

        let len = chapters.len();

        if len == 0 {
            return Err(anyhow::anyhow!("No chapters available in the description"));
        } else if len > 1 {
            // This doesn't set `end` for the last chapter. It should be fine since `end` doesn't
            // seem to be necessary to have functioning chapters in mpv.
            for idx in 1..chapters.len() {
                chapters[idx - 1].end = chapters[idx].start;
            }
        }

        Ok(Chapters { inner: chapters })
    }
}

// Can also use /next for this
pub struct Chapter {
    title: String,
    start: u64,
    end: u64,
}

impl TryFrom<&str> for Chapter {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        static RE: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"^((?<hours>\d+):)?(?<minutes>\d+):(?<seconds>\d+)(\s*[–—-]\s*(?:\d+:){1,2}\d+)?\s+([–—•-]\s*)?(?<title>.+)$").unwrap()
        });

        if let Some(captures) = RE.captures(value) {
            let hours = captures
                .name("hours")
                .map_or(0, |num| num.as_str().parse().unwrap());
            let minutes = captures["minutes"].parse::<u64>()?;
            let seconds = captures["seconds"].parse::<u64>()?;

            let timestamp = hours * 3600 + minutes * 60 + seconds;

            Ok(Chapter {
                title: captures["title"].to_owned(),
                start: timestamp,
                end: timestamp,
            })
        } else {
            Err(anyhow::anyhow!("No pattern match"))
        }
    }
}

#[derive(Deserialize, Copy, Clone)]
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

#[async_trait]
pub trait Api: Sync + Send + DynClone {
    async fn resolve_channel_id(&self, input: &str) -> Result<String> {
        if let Some((rest, channel_id)) = input.rsplit_once('/') {
            if let Some((_, path)) = rest.rsplit_once('/')
                && path == "channel"
            {
                return Ok(channel_id.to_owned());
            }
            self.resolve_url(input).await
        } else if input.starts_with('@') {
            self.resolve_url(&format!("youtube.com/{input}")).await
        } else {
            Ok(input.to_owned())
        }
    }
    async fn resolve_url(&self, channel_url: &str) -> Result<String>;
    async fn get_videos_for_the_first_time(&mut self, channel_id: &str) -> Result<ChannelFeed>;
    async fn get_videos_of_channel(&mut self, channel_id: &str) -> Result<ChannelFeed>;
    async fn get_rss_feed_of_channel(&self, channel_id: &str) -> Result<ChannelFeed>;
    async fn get_more_videos(
        &mut self,
        channel_id: &str,
        tab: ChannelTab,
        present_videos: HashSet<String>,
        get_all: bool,
    ) -> Result<ChannelFeed>;
    async fn get_video_formats(&self, video_id: &str) -> Result<VideoInfo>;
    async fn get_caption_paths(&self, formats: &Formats) -> Vec<String>;
}
