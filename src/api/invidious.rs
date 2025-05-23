use super::{Api, ApiBackend, Format, VideoInfo};
use crate::api::{ChannelFeed, ChannelTab};
use crate::channel::Video;
use crate::OPTIONS;
use anyhow::Result;
use rand::prelude::*;
use rand::thread_rng;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use ureq::{Agent, AgentBuilder};

const API_BACKEND: ApiBackend = ApiBackend::Invidious;

impl From<Value> for ChannelFeed {
    fn from(mut value: Value) -> Self {
        let mut channel_feed = Self {
            channel_title: None,
            channel_id: None,
            videos: Vec::new(),
        };

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
    agent: Agent,
    old_version: Arc<AtomicBool>,
}

impl Instance {
    pub fn new(invidious_instances: &[String]) -> Self {
        let mut rng = thread_rng();
        let domain = invidious_instances[rng.gen_range(0..invidious_instances.len())].to_string();
        let agent = AgentBuilder::new()
            .timeout(Duration::from_secs(OPTIONS.request_timeout))
            .build();

        Self {
            domain,
            agent,
            old_version: Arc::new(AtomicBool::new(false)),
        }
    }

    fn get_tab_of_channel(&self, channel_id: &str, tab: ChannelTab) -> Result<Vec<Video>> {
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

        let mut value = self
            .agent
            .get(&url)
            .query(
                "fields",
                &format!(
                    "{}(title,videoId,published,lengthSeconds,isUpcoming,premiereTimestamp)",
                    match tab {
                        ChannelTab::Videos => "latestVideos",
                        _ => "videos",
                    },
                ),
            )
            .call()?
            .into_json::<Value>()?;

        let videos_array = match tab {
            ChannelTab::Videos => value["latestVideos"].take(),
            _ => value["videos"].take(),
        };

        if let Some(video) = videos_array.get(0) {
            // if the key doesn't exist, assume that the tab is not available
            if video.get("videoId").is_none() {
                return Ok(Vec::new());
            }
        }

        Ok(Video::vec_from_json(videos_array))
    }
}

impl Api for Instance {
    fn resolve_url(&mut self, channel_url: &str) -> Result<String> {
        let url = format!("{}/api/v1/resolveurl", self.domain);
        let response = self
            .agent
            .get(&url)
            .query("url", channel_url)
            .call()?
            .into_json::<Value>()?;

        Ok(response["ucid"].as_str().unwrap().to_string())
    }

    fn get_videos_of_channel(&mut self, channel_id: &str) -> Result<ChannelFeed> {
        let mut channel_feed = ChannelFeed {
            channel_title: None,
            channel_id: Some(channel_id.to_string()),
            videos: Vec::new(),
        };

        if OPTIONS.videos_tab {
            if let Ok(videos) = self.get_tab_of_channel(channel_id, ChannelTab::Videos) {
                channel_feed.videos.extend(videos);
            }
        }

        let old_version = self.old_version.load(Ordering::SeqCst);

        if OPTIONS.shorts_tab && !old_version {
            match self.get_tab_of_channel(channel_id, ChannelTab::Shorts) {
                Ok(videos) => channel_feed.videos.extend(videos),
                Err(e) => {
                    // if the error code is 500 don't try to fetch shorts and streams tabs
                    if let Some(ureq::Error::Status(500, _)) = e.downcast_ref::<ureq::Error>() {
                        self.old_version.store(true, Ordering::SeqCst);
                        return self.get_videos_of_channel(channel_id);
                    }

                    return Err(anyhow::anyhow!(e));
                }
            }
        }

        if OPTIONS.streams_tab && !old_version {
            if let Ok(videos) = self.get_tab_of_channel(channel_id, ChannelTab::Streams) {
                channel_feed.videos.extend(videos);
            }
        }

        Ok(channel_feed)
    }

    fn get_videos_for_the_first_time(&mut self, channel_id: &str) -> Result<ChannelFeed> {
        let mut channel_feed;
        let url = format!("{}/api/v1/channels/{}/videos", self.domain, channel_id,);
        let response = self
            .agent
            .get(&url)
            .query(
                "fields",
                if self.old_version.load(Ordering::SeqCst) {
                    "author,title,videoId,published,lengthSeconds,isUpcoming,premiereTimestamp"
                }
                else {
                    "videos(author,title,videoId,published,lengthSeconds,isUpcoming,premiereTimestamp)"
                },
            )
            .call();

        match response {
            Ok(response) => channel_feed = ChannelFeed::from(response.into_json::<Value>()?),
            Err(e) => {
                // if the error code is 400, retry with the old api
                if let ureq::Error::Status(400, _) = e {
                    self.old_version.store(true, Ordering::SeqCst);
                    return self.get_videos_for_the_first_time(channel_id);
                }

                return Err(anyhow::anyhow!(e));
            }
        }

        channel_feed.channel_id = Some(channel_id.to_string());

        if !self.old_version.load(Ordering::SeqCst) {
            if !OPTIONS.videos_tab {
                channel_feed.videos.drain(..);
            }

            if OPTIONS.shorts_tab {
                if let Ok(videos) = self.get_tab_of_channel(channel_id, ChannelTab::Shorts) {
                    channel_feed.videos.extend(videos);
                }
            }

            if OPTIONS.streams_tab {
                if let Ok(videos) = self.get_tab_of_channel(channel_id, ChannelTab::Streams) {
                    channel_feed.videos.extend(videos);
                }
            }
        }

        Ok(channel_feed)
    }

    fn get_rss_feed_of_channel(&self, channel_id: &str) -> Result<ChannelFeed> {
        let url = format!("{}/feed/channel/{}", self.domain, channel_id);
        let response = self.agent.get(&url).call()?;

        Ok(quick_xml::de::from_str(&response.into_string()?).unwrap())
    }

    fn get_video_formats(&self, video_id: &str) -> Result<VideoInfo> {
        let url = format!("{}/api/v1/videos/{}", self.domain, video_id);
        let response = match self.agent.get(&url).call() {
            Ok(response) => response.into_json::<Value>()?,
            Err(e) => {
                anyhow::bail!(format!(
                    "Stream formats are not available: {}",
                    e.into_response()
                        .and_then(|response| response
                            .into_json::<Value>()
                            .ok()
                            .and_then(|value| value["error"].as_str().map(ToOwned::to_owned)))
                        .unwrap_or_default()
                ));
            }
        };

        let mut format_streams: Vec<Format> = response["formatStreams"]
            .as_array()
            .unwrap()
            .iter()
            .map(|format| Format::from_stream(format, API_BACKEND))
            .collect();

        let adaptive_formats = response["adaptiveFormats"].as_array().unwrap();

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

        let captions = response["captions"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|caption| Format::from_caption(caption, API_BACKEND))
            .collect();

        Ok(VideoInfo::new(
            video_formats,
            audio_formats,
            format_streams,
            captions,
        ))
    }
}
