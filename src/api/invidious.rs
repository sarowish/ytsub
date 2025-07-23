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
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const API_BACKEND: ApiBackend = ApiBackend::Invidious;

impl From<Value> for ChannelFeed {
    fn from(mut value: Value) -> Self {
        let mut channel_feed = Self::default();

        let videos = if value["videos"].is_null() {
            value
        } else {
            value["videos"].take()
        };

        if let Some(video) = videos.get(0) {
            channel_feed.channel_title = Some(video["author"].as_str().unwrap().to_string());
            channel_feed.videos = Video::vec_from_json(videos);
        }

        channel_feed
    }
}

#[derive(Clone)]
pub struct Instance {
    pub domain: String,
    client: Client,
    continuation: Option<String>,
    old_version: Arc<AtomicBool>,
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
            old_version: Arc::new(AtomicBool::new(false)),
        }
    }

    async fn get_tab_of_channel(&self, channel_id: &str, tab: ChannelTab) -> Result<Vec<Video>> {
        let url = format!(
            "{}/api/v1/channels/{}/{}",
            self.domain,
            channel_id,
            match tab {
                ChannelTab::Videos => "",
                ChannelTab::Shorts => "shorts",
                ChannelTab::Streams => "streams",
            }
        );

        let response = self
            .client
            .get(&url)
            .query(&[(
                "fields",
                &format!(
                    "{}(title,videoId,published,lengthSeconds,isUpcoming,premiereTimestamp)",
                    match tab {
                        ChannelTab::Videos => "latestVideos",
                        _ => "videos",
                    },
                ),
            )])
            .send()
            .await?;
        let mut value = response.error_for_status()?.json::<Value>().await?;

        let videos_array = match tab {
            ChannelTab::Videos => value["latestVideos"].take(),
            _ => value["videos"].take(),
        };

        // if the key doesn't exist, assume that the tab is not available
        if (videos_array.get(0))
            .and_then(|video| video.get("videoId"))
            .is_none()
        {
            return Ok(Vec::new());
        }

        Ok(Video::vec_from_json(videos_array))
    }

    async fn get_more_videos_helper(&mut self, channel_id: &str) -> Result<Vec<Video>> {
        let url = format!("{}/api/v1/channels/{}/videos", self.domain, channel_id,);
        let mut query = vec![(
            "fields",
            "videos(title,videoId,published,publishedText,lengthSeconds,isUpcoming,premiereTimestamp)",
        )];

        let continuation_token;

        if let Some(token) = &self.continuation {
            continuation_token = token.to_owned();
            query.push(("continuation", &continuation_token));
        }

        let response = self.client.get(&url).query(&query).send().await?;
        let mut value = response.error_for_status()?.json::<Value>().await?;

        self.continuation = value
            .get("continuation")
            .and_then(Value::as_str)
            .map(ToString::to_string);

        Ok(Video::vec_from_json(value["videos"].take()))
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

        if OPTIONS.videos_tab
            && let Ok(videos) = self
                .get_tab_of_channel(channel_id, ChannelTab::Videos)
                .await
        {
            channel_feed.videos.extend(videos);
        }

        let old_version = self.old_version.load(Ordering::SeqCst);

        if OPTIONS.shorts_tab && !old_version {
            match self
                .get_tab_of_channel(channel_id, ChannelTab::Shorts)
                .await
            {
                Ok(videos) => channel_feed.videos.extend(videos),
                Err(e) => {
                    // if the error code is 500 don't try to fetch shorts and streams tabs
                    if let Some(reqwest::StatusCode::INTERNAL_SERVER_ERROR) =
                        e.downcast_ref::<reqwest::Error>().and_then(|e| e.status())
                    {
                        self.old_version.store(true, Ordering::SeqCst);
                        return Box::pin(self.get_videos_of_channel(channel_id)).await;
                    }

                    return Err(anyhow::anyhow!(e));
                }
            }
        }

        if OPTIONS.streams_tab
            && !old_version
            && let Ok(videos) = self
                .get_tab_of_channel(channel_id, ChannelTab::Streams)
                .await
        {
            channel_feed.videos.extend(videos);
        }

        Ok(channel_feed)
    }

    async fn get_videos_for_the_first_time(&mut self, channel_id: &str) -> Result<ChannelFeed> {
        let mut channel_feed;
        let url = format!("{}/api/v1/channels/{}/videos", self.domain, channel_id,);
        let response = self
            .client
            .get(&url)
            .query(&[(
                "fields",
                if self.old_version.load(Ordering::SeqCst) {
                    "author,title,videoId,published,lengthSeconds,isUpcoming,premiereTimestamp"
                }
                else {
                    "videos(author,title,videoId,published,lengthSeconds,isUpcoming,premiereTimestamp)"
                })],
            )
            .send().await?;

        match response.error_for_status() {
            Ok(response) => channel_feed = ChannelFeed::from(response.json::<Value>().await?),
            Err(e) => {
                // if the error code is 400, retry with the old api
                if let Some(reqwest::StatusCode::BAD_REQUEST) = e.status() {
                    self.old_version.store(true, Ordering::SeqCst);
                    return Box::pin(self.get_videos_for_the_first_time(channel_id)).await;
                }

                return Err(anyhow::anyhow!(e));
            }
        }

        channel_feed.channel_id = Some(channel_id.to_string());

        if !self.old_version.load(Ordering::SeqCst) {
            if !OPTIONS.videos_tab {
                channel_feed.videos.drain(..);
            }

            if OPTIONS.shorts_tab
                && let Ok(videos) = self
                    .get_tab_of_channel(channel_id, ChannelTab::Shorts)
                    .await
            {
                channel_feed.videos.extend(videos);
            }

            if OPTIONS.streams_tab
                && let Ok(videos) = self
                    .get_tab_of_channel(channel_id, ChannelTab::Streams)
                    .await
            {
                channel_feed.videos.extend(videos);
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
        present_videos: HashSet<String>,
    ) -> Result<ChannelFeed> {
        let mut feed = ChannelFeed {
            channel_title: None,
            channel_id: Some(channel_id.to_owned()),
            videos: self.get_more_videos_helper(channel_id).await?,
        };

        let new_video_present = |videos: &[Video]| {
            !videos
                .iter()
                .all(|video| present_videos.contains(&video.video_id))
        };

        if new_video_present(&feed.videos) {
            return Ok(feed);
        }

        while self.continuation.is_some()
            && let Ok(videos) = self.get_more_videos_helper(channel_id).await
        {
            let new = new_video_present(&videos);
            feed.extend_videos(videos);

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
