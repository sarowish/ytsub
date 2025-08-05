use super::{Api, ApiBackend, ChannelFeed, Chapters, Format, VideoInfo};
use crate::channel::ListItem;
use crate::stream_formats::Formats;
use crate::{OPTIONS, channel::Video, utils};
use anyhow::Result;
use async_trait::async_trait;
use futures_util::future::join_all;
use reqwest::Client;
use serde_json::Value;
use std::collections::HashSet;
use std::time::Duration;
use std::{io::Write, path::PathBuf};

const API_BACKEND: ApiBackend = ApiBackend::Local;
const ANDROID_USER_AGENT: &str =
    "com.google.android.youtube/20.10.38 (Linux; U; Android 12; US) gzip";

#[derive(Clone)]
pub struct Local {
    client: Client,
    shorts_available: bool,
    streams_available: bool,
    continuation: Option<String>,
}

fn extract_videos_tab(value: &[Value]) -> Result<Vec<Video>> {
    let mut videos: Vec<Video> = Vec::new();

    for video in value {
        let video = &video["richItemRenderer"]["content"]["videoRenderer"];

        let title = video["title"]["runs"][0]["text"]
            .as_str()
            .unwrap()
            .to_string();

        let video_id = video["videoId"].as_str().unwrap().to_string();

        let published_text = video
            .get("publishedTimeText")
            .and_then(|t| t.get("simpleText"))
            .and_then(|t| t.as_str())
            .map(ToOwned::to_owned);

        let published = if let Some(t) = &published_text {
            utils::published(t)?
        } else if let Some(time) = video["upcomingEventData"]["startTime"].as_str() {
            time.parse::<u64>()?
        } else {
            utils::now()?
        };

        let length = video["lengthText"]["simpleText"]
            .as_str()
            .unwrap()
            .to_string();
        let length = utils::length_as_seconds(&length);

        let badges = video["badges"].as_array();

        let members_only = badges.is_some_and(|badges| {
            badges.iter().any(|badge| {
                badge["metadataBadgeRenderer"]["style"]
                    .as_str()
                    .is_some_and(|s| s == "BADGE_STYLE_TYPE_MEMBERS_ONLY")
            })
        });

        videos.push(Video {
            channel_name: None,
            video_id,
            title,
            published,
            published_text: published_text.unwrap_or_default(),
            length: Some(length),
            watched: false,
            members_only,
            new: true,
        });
    }

    Ok(videos)
}

fn extract_shorts_tab(value: &[Value]) -> Result<Vec<Video>> {
    let mut videos: Vec<Video> = Vec::new();

    for video in value {
        let video = &video["richItemRenderer"]["content"]["shortsLockupViewModel"];

        let title = video["overlayMetadata"]["primaryText"]["content"]
            .as_str()
            .unwrap()
            .to_string();
        let video_id = video["onTap"]["innertubeCommand"]["reelWatchEndpoint"]["videoId"]
            .as_str()
            .unwrap()
            .to_string();

        videos.push(Video {
            channel_name: None,
            video_id,
            title,
            published: utils::now()?,
            published_text: String::new(),
            length: None,
            watched: false,
            members_only: false,
            new: true,
        });
    }

    Ok(videos)
}

fn extract_streams_tab(value: &[Value]) -> Result<Vec<Video>> {
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
            channel_name: None,
            video_id,
            title,
            published,
            published_text: String::new(),
            length: Some(length),
            watched: false,
            members_only: false,
            new: true,
        });
    }

    Ok(videos)
}

fn extract_continuation_token(value: &[Value]) -> Option<String> {
    value
        .last()
        .and_then(|video| video.get("continuationItemRenderer"))
        .and_then(|value| value["continuationEndpoint"]["continuationCommand"]["token"].as_str())
        .map(ToString::to_string)
}

fn get_tab_by_title<'a>(value: &'a Value, title: &str) -> Option<&'a Value> {
    let tabs = value["contents"]["twoColumnBrowseResultsRenderer"]["tabs"].as_array()?;

    for tab in tabs {
        let tab = &tab["tabRenderer"];
        if matches!(tab["title"].as_str(), Some(s) if s == title) {
            return Some(tab);
        }
    }

    None
}

fn extract_videos_from_tab(tab: &Value) -> Option<&[Value]> {
    tab["content"]["richGridRenderer"]["contents"]
        .as_array()
        .map(Vec::as_slice)
}

impl Local {
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent(ANDROID_USER_AGENT)
            .timeout(Duration::from_secs(OPTIONS.request_timeout))
            .build()
            .unwrap();

        Self {
            client,
            shorts_available: false,
            streams_available: false,
            continuation: None,
        }
    }

    pub async fn post_player(&self, video_id: &str) -> Result<Value> {
        let url = "https://www.youtube.com/youtubei/v1/player?key=AIzaSyA8eiZmM1FaDVjRy-df2KTyQ_vz_yYM39w";

        let data = serde_json::json!({
            "context": {
                "client": {
                    "clientName": "ANDROID",
                    "clientVersion": "20.10.38",
                    "userAgent": ANDROID_USER_AGENT,
                },
            },
            "videoId": video_id
        });

        let response = self.client.post(url).json(&data).send().await?;
        Ok(response.error_for_status()?.json().await?)
    }

    pub async fn post_browse(&self, items: &[(&str, &str)]) -> Result<Value> {
        let url = "https://www.youtube.com/youtubei/v1/browse?key=AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8";

        let mut data = serde_json::json!({
            "context": {
                "client": {
                    "clientName": "WEB",
                    "clientVersion": "2.20240304.00.00"
                }
            }
        });

        let map = data.as_object_mut().unwrap();

        for (key, value) in items {
            map.insert((*key).to_string(), Value::String((*value).to_string()));
        }

        let response = self.client.post(url).json(&data).send().await?;
        Ok(response.error_for_status()?.json().await?)
    }

    async fn get_videos_tab(
        &mut self,
        channel_id: &str,
        channel_title: &mut Option<String>,
    ) -> Result<Vec<Video>> {
        let response = self
            .post_browse(&[("browseId", channel_id), ("params", "EgZ2aWRlb3PyBgQKAjoA")])
            .await?;

        let mut videos = get_tab_by_title(&response, "Videos")
            .and_then(|tab| extract_videos_from_tab(tab))
            .unwrap_or_default();

        *channel_title = response["metadata"]["channelMetadataRenderer"]["title"]
            .as_str()
            .map(std::string::ToString::to_string);

        if get_tab_by_title(&response, "Shorts").is_some() {
            self.shorts_available = true;
        }

        if get_tab_by_title(&response, "Live").is_some() {
            self.streams_available = true;
        }

        if let Some(token) = extract_continuation_token(videos) {
            self.continuation = Some(token);
            videos = videos.split_last().unwrap().1;
        }

        extract_videos_tab(videos)
    }

    async fn get_shorts_tab(&mut self, channel_id: &str) -> Result<Vec<Video>> {
        let response = self
            .post_browse(&[
                ("browseId", channel_id),
                ("params", "EgZzaG9ydHPyBgUKA5oBAA"),
            ])
            .await?;

        let Some(mut shorts) =
            get_tab_by_title(&response, "Shorts").and_then(|tab| extract_videos_from_tab(tab))
        else {
            return Ok(Vec::new());
        };

        if extract_continuation_token(shorts).is_some() {
            shorts = shorts.split_last().unwrap().1;
        }

        extract_shorts_tab(shorts)
    }

    async fn get_streams_tab(&mut self, channel_id: &str) -> Result<Vec<Video>> {
        let response = self
            .post_browse(&[
                ("browseId", channel_id),
                ("params", "EgdzdHJlYW1z8gYECgJ6AA"),
            ])
            .await?;

        let Some(mut streams) =
            get_tab_by_title(&response, "Live").and_then(|tab| extract_videos_from_tab(tab))
        else {
            return Ok(Vec::new());
        };

        if extract_continuation_token(streams).is_some() {
            streams = streams.split_last().unwrap().1;
        }

        extract_streams_tab(streams)
    }

    async fn get_continuation(&mut self) -> Result<Vec<Video>> {
        let Some(continuation_token) = &self.continuation else {
            return Err(anyhow::anyhow!("No continuation token"));
        };

        let response = self
            .post_browse(&[("continuation", continuation_token)])
            .await?;

        let mut videos = response["onResponseReceivedActions"][0]["appendContinuationItemsAction"]
            ["continuationItems"]
            .as_array()
            .unwrap()
            .as_slice();

        self.continuation = extract_continuation_token(videos);

        if self.continuation.is_some() {
            videos = videos.split_last().unwrap().1;
        }

        extract_videos_tab(videos)
    }

    pub async fn get_caption(
        &self,
        url: &str,
        video_id: &str,
        language_code: &str,
    ) -> Result<PathBuf> {
        let path = utils::get_cache_dir()?.join(format!("{video_id}_{language_code}.srt"));

        if let Ok(true) = path.try_exists() {
            return Ok(path);
        }

        let response = self
            .client
            .get(url.replace("fmt=srv3", "fmt=vtt"))
            .send()
            .await?
            .error_for_status()?;

        let mut file = std::fs::File::create(&path)?;
        file.write_all(response.text().await?.as_bytes())?;

        Ok(path)
    }
}

#[async_trait]
impl Api for Local {
    async fn resolve_url(&self, channel_url: &str) -> Result<String> {
        let url = "https://www.youtube.com/youtubei/v1/navigation/resolve_url";

        let data = serde_json::json!({
            "context": {
                "client": {
                    "clientName": "WEB",
                    "clientVersion": "2.20240304.00.00"
                },
            },
            "url": channel_url
        });

        let response = self.client.post(url).json(&data).send().await?;
        let value = response.json::<Value>().await?;
        let endpoint = &value["endpoint"];

        if let Some(browse_endpoint) = endpoint.get("browseEndpoint") {
            let channel_id = browse_endpoint["browseId"].as_str().unwrap().to_string();
            Ok(channel_id)
        } else if let Some(url_endpoint) = endpoint.get("urlEndpoint") {
            Box::pin(self.resolve_url(url_endpoint["url"].as_str().unwrap())).await
        } else {
            Err(anyhow::anyhow!("Couldn't resolve url"))
        }
    }

    async fn get_videos_for_the_first_time(&mut self, channel_id: &str) -> Result<ChannelFeed> {
        let mut channel_feed = self.get_videos_of_channel(channel_id).await?;

        if OPTIONS.videos_tab && self.continuation.is_some() {
            let videos = self.get_continuation().await?;
            channel_feed.extend_videos(videos);
        }

        Ok(channel_feed)
    }

    async fn get_videos_of_channel(&mut self, channel_id: &str) -> Result<ChannelFeed> {
        let mut channel_title = None;
        let mut videos = self.get_videos_tab(channel_id, &mut channel_title).await?;

        if !OPTIONS.videos_tab {
            videos.drain(..);
        }

        if OPTIONS.shorts_tab && self.shorts_available {
            let shorts = self.get_shorts_tab(channel_id).await?;
            videos.extend(shorts);
        }

        if OPTIONS.streams_tab && self.streams_available {
            let streams = self.get_streams_tab(channel_id).await?;
            videos.extend(streams);
        }

        Ok(ChannelFeed {
            channel_title,
            channel_id: Some(channel_id.to_string()),
            videos,
        })
    }

    async fn get_rss_feed_of_channel(&self, channel_id: &str) -> Result<ChannelFeed> {
        let url = format!("https://www.youtube.com/feeds/videos.xml?channel_id={channel_id}");
        let response = self.client.get(&url).send().await?.error_for_status()?;

        let mut channel_feed: ChannelFeed = quick_xml::de::from_str(&response.text().await?)?;
        channel_feed.channel_id = Some(channel_id.to_string());

        Ok(channel_feed)
    }

    async fn get_more_videos(
        &mut self,
        channel_id: &str,
        present_videos: HashSet<String>,
    ) -> Result<ChannelFeed> {
        let mut feed = self.get_videos_of_channel(channel_id).await?;

        let new_video_present = |videos: &[Video]| {
            !videos
                .iter()
                .all(|video| present_videos.contains(&video.video_id))
        };

        if new_video_present(&feed.videos) {
            return Ok(feed);
        }

        while let Ok(videos) = self.get_continuation().await {
            let new = new_video_present(&videos);
            feed.extend_videos(videos);

            if new {
                return Ok(feed);
            }
        }

        Ok(ChannelFeed::default())
    }

    async fn get_video_formats(&self, video_id: &str) -> Result<VideoInfo> {
        let response = self.post_player(video_id).await?;

        let formats = response["streamingData"]
            .get("formats")
            .map_or(&Vec::new(), |formats| formats.as_array().unwrap())
            .iter()
            .map(|format| Format::from_stream(format, API_BACKEND))
            .rev()
            .collect();

        let Some(adaptive_formats) = response["streamingData"]["adaptiveFormats"].as_array() else {
            let reason = response["playabilityStatus"]["reason"]
                .as_str()
                .unwrap_or_default();
            anyhow::bail!("Stream formats are not available: {reason}")
        };

        let mut video_formats = Vec::new();
        let mut audio_formats = Vec::new();

        for format in adaptive_formats {
            if format.get("qualityLabel").is_some() {
                video_formats.push(Format::from_video(format, API_BACKEND));
            } else if format.get("audioQuality").is_some() {
                audio_formats.push(Format::from_audio(format, API_BACKEND));
            }
        }

        let captions = response["captions"]["playerCaptionsTracklistRenderer"]["captionTracks"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .filter_map(|caption| Format::from_caption(caption, API_BACKEND))
            .collect();

        let chapters = OPTIONS
            .chapters
            .then(|| Chapters::try_from(response["videoDetails"]["shortDescription"].as_str()).ok())
            .flatten();

        Ok(VideoInfo::new(
            video_formats,
            audio_formats,
            formats,
            captions,
            chapters,
        ))
    }

    async fn get_caption_paths(&self, formats: &Formats) -> Vec<String> {
        let captions = formats.captions.get_selected_items();

        join_all(captions.iter().map(|captions| async {
            self.get_caption(captions.get_url(), &formats.id, captions.id())
                .await
        }))
        .await
        .into_iter()
        .map_while(Result::ok)
        .map(|path| path.to_string_lossy().to_string())
        .collect()
    }
}
