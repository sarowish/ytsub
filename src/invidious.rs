use crate::channel::Video;
use crate::OPTIONS;
use anyhow::Result;
use rand::prelude::*;
use rand::thread_rng;
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;
use ureq::{Agent, AgentBuilder};

#[derive(Deserialize)]
pub struct ChannelFeed {
    #[serde(rename = "title")]
    pub channel_title: Option<String>,
    #[serde(rename = "channelId")]
    pub channel_id: Option<String>,
    #[serde(rename = "entry")]
    pub videos: Vec<Video>,
}

impl From<Value> for ChannelFeed {
    fn from(mut value: Value) -> Self {
        let channel_title = value.get("author");

        if let Some(channel_title) = channel_title {
            Self {
                channel_title: Some(channel_title.as_str().unwrap().to_string()),
                channel_id: Some(value.get("authorId").unwrap().as_str().unwrap().to_string()),
                videos: Video::vec_from_json(value["latestVideos"].take()),
            }
        } else {
            Self {
                channel_title: None,
                channel_id: None,
                videos: Video::vec_from_json(value),
            }
        }
    }
}

#[derive(Clone)]
pub struct Instance {
    pub domain: String,
    agent: Agent,
}

impl Instance {
    pub fn new(invidious_instances: &[String]) -> Result<Self> {
        let mut rng = thread_rng();
        let domain = invidious_instances[rng.gen_range(0..invidious_instances.len())].to_string();
        let agent = AgentBuilder::new()
            .timeout(Duration::from_secs(OPTIONS.request_timeout))
            .build();
        Ok(Self { domain, agent })
    }

    pub fn get_videos_of_channel(&self, channel_id: &str) -> Result<ChannelFeed> {
        let url = format!("{}/api/v1/channels/{}", self.domain, channel_id);
        Ok(ChannelFeed::from(self
            .agent
            .get(&url)
            .query(
                "fields",
                "author,authorId,latestVideos(title,videoId,published,lengthSeconds,isUpcoming,premiereTimestamp)",
            )
            .call()?
            .into_json::<Value>()?))
    }

    pub fn get_latest_videos_of_channel(&self, channel_id: &str) -> Result<ChannelFeed> {
        let url = format!("{}/api/v1/channels/latest/{}", self.domain, channel_id);
        let mut res = ChannelFeed::from(
            self.agent
                .get(&url)
                .query(
                    "fields",
                    "title,videoId,published,lengthSeconds,isUpcoming,premiereTimestamp",
                )
                .call()?
                .into_json::<Value>()?,
        );

        res.channel_id = Some(channel_id.to_string());

        Ok(res)
    }

    pub fn get_rss_feed_of_channel(&self, channel_id: &str) -> Result<ChannelFeed> {
        let url = format!("{}/feed/channel/{}", self.domain, channel_id);
        let response = self.agent.get(&url).call()?;

        Ok(quick_xml::de::from_str(&response.into_string()?).unwrap())
    }
}
