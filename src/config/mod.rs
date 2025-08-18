pub mod keys;
pub mod theme;

use self::{keys::KeyBindings, theme::Theme};
use crate::{
    CLAP_ARGS,
    api::{ApiBackend, VideoFormat},
    app::{Mode, VideoPlayer},
    channel::ChannelTab,
    utils,
};
use anyhow::Result;
use bitflags::bitflags;
use serde::{Deserialize, de};
use std::{fs, path::PathBuf};

const CONFIG_FILE: &str = "config.toml";

bitflags! {
    pub struct EnabledTabs: u8 {
        const VIDEOS  = 0b0001;
        const SHORTS  = 0b0010;
        const STREAMS = 0b0100;
    }
}

#[derive(Deserialize)]
#[serde(rename_all(deserialize = "snake_case"))]
pub enum VideoInfoPosition {
    Top,
    Bottom,
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub database: PathBuf,
    pub instances: PathBuf,
    pub mode: Mode,
    #[serde(deserialize_with = "deserialize_tabs")]
    pub tabs: EnabledTabs,
    pub hide_disabled_tabs: bool,
    pub api: ApiBackend,
    pub refresh_threshold: u64,
    pub refresh_on_launch: bool,
    pub rss_threshold: usize,
    pub tick_rate: u64,
    pub request_timeout: u64,
    pub highlight_symbol: String,
    pub video_player_for_stream_formats: VideoPlayer,
    #[serde(alias = "video_player")]
    pub mpv_path: PathBuf,
    pub vlc_path: PathBuf,
    pub hide_watched: bool,
    pub hide_members_only: bool,
    pub show_thumbnails: bool,
    pub video_info_position: VideoInfoPosition,
    pub always_show_video_info: bool,
    pub subtitle_languages: Vec<String>,
    pub prefer_dash_formats: bool,
    pub prefer_original_titles: bool,
    pub prefer_original_audio: bool,
    #[serde(deserialize_with = "deserialize_video_quality")]
    pub video_quality: u16,
    pub preferred_video_codec: Option<VideoFormat>,
    pub preferred_audio_codec: Option<VideoFormat>,
    pub chapters: bool,

    pub theme: Theme,
    pub key_bindings: KeyBindings,
}

impl Config {
    pub fn new() -> Result<Self> {
        let config_file = match CLAP_ARGS.get_one::<PathBuf>("config") {
            Some(path) => path.to_owned(),
            None => utils::get_config_dir()?.join(CONFIG_FILE),
        };

        let mut config = match fs::read_to_string(config_file) {
            Ok(config_str) if !CLAP_ARGS.get_flag("no_config") => {
                toml::from_str::<Self>(&config_str)?
            }
            _ => Self::default(),
        };

        config.override_with_clap_args();

        if config.database.as_os_str().is_empty() {
            config.database = utils::get_default_database_file()?;
        }

        if config.instances.as_os_str().is_empty() {
            config.instances = utils::get_default_instances_file()?;
        }

        Ok(config)
    }

    pub fn override_with_clap_args(&mut self) {
        if let Some(database) = CLAP_ARGS.get_one::<PathBuf>("database") {
            database.clone_into(&mut self.database);
        }

        if let Some(instances) = CLAP_ARGS.get_one::<PathBuf>("instances") {
            instances.clone_into(&mut self.instances);
        }

        if let Some(tick_rate) = CLAP_ARGS.get_one::<u64>("tick_rate") {
            self.tick_rate = *tick_rate;
        }

        if let Some(request_timeout) = CLAP_ARGS.get_one::<u64>("request_timeout") {
            self.request_timeout = *request_timeout;
        }

        if let Some(highlight_symbol) = CLAP_ARGS.get_one::<String>("highlight_symbol") {
            highlight_symbol.clone_into(&mut self.highlight_symbol);
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database: PathBuf::default(),
            instances: PathBuf::default(),
            mode: Mode::default(),
            tabs: EnabledTabs::VIDEOS,
            hide_disabled_tabs: true,
            api: ApiBackend::Local,
            refresh_threshold: 600,
            refresh_on_launch: true,
            rss_threshold: 9999,
            tick_rate: 10,
            request_timeout: 5,
            highlight_symbol: String::new(),
            video_player_for_stream_formats: VideoPlayer::Mpv,
            mpv_path: PathBuf::from("mpv"),
            vlc_path: PathBuf::from("vlc"),
            hide_watched: false,
            hide_members_only: false,
            show_thumbnails: true,
            video_info_position: VideoInfoPosition::Top,
            always_show_video_info: true,
            subtitle_languages: Vec::new(),
            prefer_dash_formats: true,
            prefer_original_titles: true,
            prefer_original_audio: true,
            video_quality: u16::MAX,
            preferred_video_codec: None,
            preferred_audio_codec: None,
            chapters: true,

            theme: Theme::default(),
            key_bindings: KeyBindings::default(),
        }
    }
}

fn deserialize_tabs<'de, D>(deserializer: D) -> Result<EnabledTabs, D::Error>
where
    D: de::Deserializer<'de>,
{
    let tabs: Vec<ChannelTab> = de::Deserialize::deserialize(deserializer)?;

    let mut enabled = EnabledTabs::empty();

    if tabs.contains(&ChannelTab::Videos) {
        enabled.insert(EnabledTabs::VIDEOS);
    }

    if tabs.contains(&ChannelTab::Shorts) {
        enabled.insert(EnabledTabs::SHORTS);
    }

    if tabs.contains(&ChannelTab::Streams) {
        enabled.insert(EnabledTabs::STREAMS);
    }

    Ok(enabled)
}

fn deserialize_video_quality<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: de::Deserializer<'de>,
{
    use serde::de::Error;

    let quality_str: String = de::Deserialize::deserialize(deserializer)?;

    let quality = if quality_str.to_lowercase() == "best" {
        u16::MAX
    } else if let Some(Ok(quality)) = quality_str.strip_suffix('p').map(str::parse::<u16>) {
        quality
    } else if let Ok(quality) = quality_str.parse::<u16>() {
        quality
    } else {
        return Err(Error::custom(format!(
            "\"{quality_str}\" is not a valid quality"
        )));
    };

    Ok(quality)
}
