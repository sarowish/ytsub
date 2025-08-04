use super::{Api, ApiBackend, Chapters, Format, VideoInfo};
use crate::OPTIONS;
use crate::api::{ChannelFeed, ChannelTab};
use crate::channel::Video;
use crate::stream_formats::Formats;
use anyhow::Result;
use async_trait::async_trait;
use rand::prelude::*;
use reqwest::Client;
use serde_json::Value;
use std::collections::HashSet;
use std::time::Duration;

const API_BACKEND: ApiBackend = ApiBackend::Invidious;

fn extract_tab(videos_array: &Value) -> Option<Vec<Video>> {
    videos_array
        .as_array()
        .map(|array| array.iter().map(Video::from).collect())
}

#[derive(Clone)]
pub struct Instance {
    pub domain: String,
    client: Client,
    continuation: Option<String>,
}

impl Instance {
    pub fn new(invidious_instances: &[String]) -> Self {
        let mut rng = rand::rng();
        let domain =
            invidious_instances[rng.random_range(0..invidious_instances.len())].to_string();
        let client = Client::builder()
            .timeout(Duration::from_secs(OPTIONS.request_timeout))
            .build()
            .unwrap();

        Self {
            domain,
            client,
            continuation: None,
        }
    }

    async fn get_tab_of_channel(&self, channel_id: &str, tab: ChannelTab) -> Result<Value> {
        let url = format!("{}/api/v1/channels/{}/{}", self.domain, channel_id, tab);

        let response = self.client.get(&url).send().await?;
        let mut value = response.error_for_status()?.json::<Value>().await?;

        Ok(value["videos"].take())
    }

    async fn get_more_videos_helper(
        &mut self,
        channel_id: &str,
        tab: ChannelTab,
    ) -> Result<Vec<Video>> {
        let url = format!("{}/api/v1/channels/{}/{}", self.domain, channel_id, tab);
        let mut request = self.client.get(&url);

        if let Some(token) = &self.continuation {
            request = request.query(&[("continuation", token)]);
        }

        let response = request.send().await?;
        let value = response.error_for_status()?.json::<Value>().await?;

        self.continuation = value
            .get("continuation")
            .and_then(Value::as_str)
            .map(ToString::to_string);

        Ok(extract_tab(&value["videos"]).unwrap_or_default())
    }
}

#[async_trait]
impl Api for Instance {
    async fn resolve_url(&self, channel_url: &str) -> Result<String> {
        let url = format!("{}/api/v1/resolveurl", self.domain);
        let response = self
            .client
            .get(&url)
            .query(&[("url", channel_url)])
            .send()
            .await?;

        let value: Value = response.error_for_status()?.json().await?;

        Ok(value["ucid"].as_str().unwrap().to_string())
    }

    async fn get_videos_of_channel(&mut self, channel_id: &str) -> Result<ChannelFeed> {
        let mut channel_feed = ChannelFeed::new(channel_id);

        for tab in OPTIONS.tabs.iter().map(|tab| tab.bits().into()) {
            let videos_array = self.get_tab_of_channel(channel_id, tab).await?;

            if let Some(videos) = extract_tab(&videos_array) {
                *channel_feed.get_mut_videos(tab) = videos;
            }
        }

        Ok(channel_feed)
    }

    async fn get_videos_for_the_first_time(&mut self, channel_id: &str) -> Result<ChannelFeed> {
        let mut channel_feed = ChannelFeed::new(channel_id);

        for tab in OPTIONS.tabs.iter().map(|tab| tab.bits().into()) {
            let videos_array = self.get_tab_of_channel(channel_id, tab).await?;

            if channel_feed.channel_title.is_none()
                && let Some(video) = videos_array.get(0)
            {
                channel_feed.channel_title = video["author"].as_str().map(ToString::to_string);
            }

            if let Some(videos) = extract_tab(&videos_array) {
                *channel_feed.get_mut_videos(tab) = videos;
            }
        }

        Ok(channel_feed)
    }

    async fn get_rss_feed_of_channel(&self, channel_id: &str) -> Result<ChannelFeed> {
        let url = format!("{}/feed/channel/{}", self.domain, channel_id);
        let response = self.client.get(&url).send().await?.error_for_status()?;

        Ok(quick_xml::de::from_str(&response.text().await?)?)
    }

    async fn get_more_videos(
        &mut self,
        channel_id: &str,
        tab: ChannelTab,
        present_videos: HashSet<String>,
    ) -> Result<ChannelFeed> {
        let mut feed = ChannelFeed::new(channel_id);
        let videos = self.get_more_videos_helper(channel_id, tab).await?;

        match tab {
            ChannelTab::Videos => feed.videos = videos,
            ChannelTab::Shorts => feed.shorts = videos,
            ChannelTab::Streams => feed.live_streams = videos,
        }

        let new_video_present = |videos: &[Video]| {
            !videos
                .iter()
                .all(|video| present_videos.contains(&video.video_id))
        };

        if new_video_present(&feed.videos) {
            return Ok(feed);
        }

        while self.continuation.is_some()
            && let Ok(videos) = self.get_more_videos_helper(channel_id, tab).await
        {
            let new = new_video_present(&videos);
            feed.extend_videos(videos, tab);

            if new {
                return Ok(feed);
            }
        }

        Ok(ChannelFeed::default())
    }

    async fn get_video_formats(&self, video_id: &str) -> Result<VideoInfo> {
        let url = format!("{}/api/v1/videos/{}", self.domain, video_id);
        let response = self.client.get(&url).send().await?;
        let value = match response.error_for_status() {
            Ok(response) => response.json::<Value>().await?,
            Err(_e) => {
                anyhow::bail!(format!("Stream formats are not available: ",));
            }
        };

        let mut format_streams: Vec<Format> = value["formatStreams"]
            .as_array()
            .unwrap()
            .iter()
            .map(|format| Format::from_stream(format, API_BACKEND))
            .collect();

        let adaptive_formats = value["adaptiveFormats"].as_array().unwrap();

        let mut video_formats = Vec::new();
        let mut audio_formats = Vec::new();

        for format in adaptive_formats {
            if format.get("qualityLabel").is_some() {
                video_formats.push(Format::from_video(format, API_BACKEND));
            } else if format.get("audioQuality").is_some() {
                audio_formats.push(Format::from_audio(format, API_BACKEND));
            }
        }

        format_streams.reverse();
        video_formats.reverse();

        let captions = value["captions"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|caption| Format::from_caption(caption, API_BACKEND))
            .collect();

        let chapters = OPTIONS
            .chapters
            .then(|| Chapters::try_from(value["description"].as_str()).ok())
            .flatten();

        Ok(VideoInfo::new(
            video_formats,
            audio_formats,
            format_streams,
            captions,
            chapters,
        ))
    }

    async fn get_caption_paths(&self, formats: &Formats) -> Vec<String> {
        formats
            .captions
            .get_selected_items()
            .iter()
            .map(|caption| format!("{}{}", self.domain, caption.get_url()))
            .collect()
    }
}
