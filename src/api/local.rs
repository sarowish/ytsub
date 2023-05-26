use super::{Api, ChannelFeed};
use crate::{channel::Video, utils, OPTIONS};
use anyhow::Result;
use serde_json::Value;
use std::time::Duration;
use ureq::{Agent, AgentBuilder};

#[derive(Clone)]
pub struct Local {
    agent: Agent,
    continuation: Option<String>,
}

impl Local {
    pub fn new() -> Self {
        let agent = AgentBuilder::new()
            .timeout(Duration::from_secs(OPTIONS.request_timeout))
            .build();

        Self {
            agent,
            continuation: None,
        }
    }

    pub fn post_json(&self, items: &[(&str, &str)]) -> Result<Value> {
        const URL: &str =
            "https://www.youtube.com/youtubei/v1/browse?key=AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8";

        let mut data = ureq::json!({
            "context": {
                "client": {
                    "clientName": "WEB",
                    "clientVersion": "2.20230201.01.00"
                }
            }
        });

        let map = data.as_object_mut().unwrap();

        for (key, value) in items {
            map.insert(
                key.to_string(),
                serde_json::Value::String(value.to_string()),
            );
        }

        Ok(self.agent.post(URL).send_json(data)?.into_json::<Value>()?)
    }

    fn get_videos_tab(
        &mut self,
        channel_id: &str,
        channel_title: &mut Option<String>,
        shorts_available: &mut bool,
        streams_available: &mut bool,
    ) -> Result<Vec<Video>> {
        let response =
            self.post_json(&[("browseId", channel_id), ("params", "EgZ2aWRlb3PyBgQKAjoA")])?;

        let tabs = &response["contents"]["twoColumnBrowseResultsRenderer"]["tabs"];

        let videos = &tabs[1]["tabRenderer"]["content"]["richGridRenderer"]["contents"];

        if videos.is_null() {
            return Err(anyhow::anyhow!("Channel doesn't exist"));
        }

        let mut videos = videos.as_array().unwrap().as_slice();

        *channel_title = Some(
            response["header"]["c4TabbedHeaderRenderer"]["title"]
                .as_str()
                .unwrap()
                .to_string(),
        );

        if tabs[2]["tabRenderer"]["title"].as_str().unwrap() == "Shorts" {
            *shorts_available = true;
        } else if tabs[2]["tabRenderer"]["title"].as_str().unwrap() == "Live" {
            *streams_available = true;
        }

        if tabs[3]["tabRenderer"]["title"].as_str().unwrap() == "Live" {
            *streams_available = true;
        }

        if let Some(token) = self.extract_continuation_token(videos) {
            self.continuation = Some(token);
            videos = videos.split_last().unwrap().1;
        }

        self.extract_videos_tab(videos)
    }

    fn extract_videos_tab(&self, value: &[Value]) -> Result<Vec<Video>> {
        let mut videos: Vec<Video> = Vec::new();

        for video in value {
            let video = &video["richItemRenderer"]["content"]["videoRenderer"];

            let title = video["title"]["runs"][0]["text"]
                .as_str()
                .unwrap()
                .to_string();

            let video_id = video["videoId"].as_str().unwrap().to_string();

            let published = if let Some(t) = video.get("publishedTimeText") {
                let published_text = t["simpleText"].as_str().unwrap().to_string();
                utils::published(&published_text)?
            } else if let Some(time) = video["upcomingEventData"]["startTime"].as_str() {
                time.parse::<u64>().unwrap()
            } else {
                utils::now()?
            };

            let length = video["lengthText"]["simpleText"]
                .as_str()
                .unwrap()
                .to_string();
            let length = utils::length_as_seconds(&length);

            videos.push(Video {
                channel_name: Default::default(),
                video_id,
                title,
                published,
                published_text: String::new(),
                length: Some(length),
                watched: false,
                new: true,
            })
        }

        Ok(videos)
    }

    fn get_shorts_tab(&mut self, channel_id: &str) -> Result<Vec<Video>> {
        let response = self.post_json(&[
            ("browseId", channel_id),
            ("params", "EgZzaG9ydHPyBgUKA5oBAA"),
        ])?;

        let tab = &response["contents"]["twoColumnBrowseResultsRenderer"]["tabs"][2]["tabRenderer"];

        if tab["title"].as_str().unwrap() != "Shorts" {
            return Ok(Vec::new());
        }

        let videos = &tab["content"]["richGridRenderer"]["contents"];

        if videos.is_null() {
            return Ok(Vec::new());
        }

        let mut videos = videos.as_array().unwrap().as_slice();

        if self.extract_continuation_token(videos).is_some() {
            videos = videos.split_last().unwrap().1;
        }

        self.extract_shorts_tab(videos)
    }

    fn extract_shorts_tab(&self, value: &[Value]) -> Result<Vec<Video>> {
        let mut videos: Vec<Video> = Vec::new();

        for video in value {
            let video = &video["richItemRenderer"]["content"]["reelItemRenderer"];

            let title = video["headline"]["simpleText"]
                .as_str()
                .unwrap()
                .to_string();
            let video_id = video["videoId"].as_str().unwrap().to_string();

            let published_text = &video["navigationEndpoint"]["reelWatchEndpoint"]["overlay"]
                ["reelPlayerOverlayRenderer"]["reelPlayerHeaderSupportedRenderers"]
                ["reelPlayerHeaderRenderer"]["timestampText"]["simpleText"];

            if published_text.is_null() {
                return Ok(Vec::new());
            }
            let published = utils::published(published_text.as_str().unwrap())?;

            let accessibility = video["accessibility"]["accessibilityData"]["label"]
                .as_str()
                .unwrap()
                .to_string();
            let accessibility = accessibility.split(" - ").collect::<Vec<&str>>();

            let length_text = accessibility[accessibility.len() - 2];
            let mut length = 0;

            for t in length_text.split(", ") {
                let (num, time_frame) = t.split_once(' ').unwrap();

                if time_frame == "minute" {
                    length = 60;
                } else {
                    length += num.parse::<u32>().unwrap();
                }
            }

            videos.push(Video {
                channel_name: Default::default(),
                video_id,
                title,
                published,
                published_text: String::new(),
                length: Some(length),
                watched: false,
                new: true,
            })
        }

        Ok(videos)
    }

    fn get_streams_tab(&mut self, channel_id: &str) -> Result<Vec<Video>> {
        let response = self.post_json(&[
            ("browseId", channel_id),
            ("params", "EgdzdHJlYW1z8gYECgJ6AA"),
        ])?;

        let tabs = &response["contents"]["twoColumnBrowseResultsRenderer"]["tabs"];

        let tab = if tabs[2]["tabRenderer"]["title"].as_str().unwrap() == "Live" {
            &tabs[2]
        } else if tabs[3]["tabRenderer"]["title"].as_str().unwrap() == "Live" {
            &tabs[3]
        } else {
            return Ok(Vec::new());
        };

        let videos = &tab["tabRenderer"]["content"]["richGridRenderer"]["contents"];

        if videos.is_null() {
            return Ok(Vec::new());
        }

        let mut videos = videos.as_array().unwrap().as_slice();

        if self.extract_continuation_token(videos).is_some() {
            videos = videos.split_last().unwrap().1;
        }

        self.extract_streams_tab(videos)
    }

    fn extract_streams_tab(&self, value: &[Value]) -> Result<Vec<Video>> {
        let mut videos: Vec<Video> = Vec::new();

        for video in value {
            let video = &video["richItemRenderer"]["content"]["videoRenderer"];

            if video.is_null() {
                continue;
            }

            let title = video["title"]["runs"][0]["text"]
                .as_str()
                .unwrap()
                .to_string();
            let video_id = video["videoId"].as_str().unwrap().to_string();

            let published = if let Some(t) = video.get("publishedTimeText") {
                let published_text = t["simpleText"]
                    .as_str()
                    .unwrap()
                    .splitn(2, ' ')
                    .collect::<Vec<&str>>()[1];
                utils::published(published_text)?
            } else if let Some(time) = video["upcomingEventData"]["startTime"].as_str() {
                time.parse::<u64>().unwrap()
            } else {
                utils::now()?
            };

            let length = if let Some(t) = video.get("lengthText") {
                let length_text = t["simpleText"].as_str().unwrap().to_string();
                utils::length_as_seconds(&length_text)
            } else {
                0
            };

            videos.push(Video {
                channel_name: Default::default(),
                video_id,
                title,
                published,
                published_text: String::new(),
                length: Some(length),
                watched: false,
                new: true,
            })
        }

        Ok(videos)
    }

    fn get_continuation(&mut self) -> Result<Vec<Video>> {
        let response = self.post_json(&[("continuation", self.continuation.as_ref().unwrap())])?;

        let mut videos = response["onResponseReceivedActions"][0]["appendContinuationItemsAction"]
            ["continuationItems"]
            .as_array()
            .unwrap()
            .as_slice();

        if self.extract_continuation_token(videos).is_some() {
            videos = videos.split_last().unwrap().1;
        }

        self.extract_videos_tab(videos)
    }

    fn extract_continuation_token(&mut self, value: &[Value]) -> Option<String> {
        if let Some(video) = value.last() {
            if let Some(value) = video.get("continuationItemRenderer") {
                return Some(
                    value["continuationEndpoint"]["continuationCommand"]["token"]
                        .as_str()
                        .unwrap()
                        .to_string(),
                );
            }
        }

        None
    }
}

impl Api for Local {
    fn get_videos_for_the_first_time(&mut self, channel_id: &str) -> Result<ChannelFeed> {
        let mut channel_feed = self.get_videos_of_channel(channel_id)?;

        if OPTIONS.videos_tab && self.continuation.is_some() {
            let videos = self.get_continuation()?;
            channel_feed.videos.extend(videos);
        }

        Ok(channel_feed)
    }

    fn get_videos_of_channel(&mut self, channel_id: &str) -> Result<ChannelFeed> {
        let mut channel_title = None;
        let mut shorts_available = false;
        let mut streams_available = false;
        let mut videos = self.get_videos_tab(
            channel_id,
            &mut channel_title,
            &mut shorts_available,
            &mut streams_available,
        )?;

        if !OPTIONS.videos_tab {
            videos.drain(..);
        }

        if OPTIONS.shorts_tab && shorts_available {
            let shorts = self.get_shorts_tab(channel_id)?;
            videos.extend(shorts);
        }

        if OPTIONS.streams_tab && streams_available {
            let streams = self.get_streams_tab(channel_id)?;
            videos.extend(streams);
        }

        Ok(ChannelFeed {
            channel_title,
            channel_id: Some(channel_id.to_string()),
            videos,
        })
    }

    fn get_rss_feed_of_channel(&self, channel_id: &str) -> Result<ChannelFeed> {
        let url = format!("https://www.youtube.com/feeds/videos.xml?channel_id={channel_id}");
        let response = self.agent.get(&url).call()?;

        let mut channel_feed: ChannelFeed =
            quick_xml::de::from_str(&response.into_string()?).unwrap();
        channel_feed.channel_id = Some(channel_id.to_string());

        Ok(channel_feed)
    }
}
