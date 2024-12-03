use crate::{
    api::{ApiBackend, ChannelTab, PreferredVideoFormat},
    app::VideoPlayer,
    CLAP_ARGS,
};
use serde::{de, Deserialize};
use std::path::PathBuf;

#[derive(Deserialize)]
pub struct UserOptions {
    database: Option<PathBuf>,
    instances: Option<PathBuf>,
    tabs: Option<Vec<ChannelTab>>,
    api: Option<ApiBackend>,
    refresh_threshold: Option<u64>,
    rss_threshold: Option<usize>,
    tick_rate: Option<u64>,
    request_timeout: Option<u64>,
    highlight_symbol: Option<String>,
    video_player_for_stream_formats: Option<VideoPlayer>,
    #[serde(alias = "video_player")]
    mpv_path: Option<PathBuf>,
    vlc_path: Option<PathBuf>,
    hide_watched: Option<bool>,
    subtitle_languages: Option<Vec<String>>,
    prefer_dash_formats: Option<bool>,
    #[serde(deserialize_with = "deserialize_video_quality")]
    video_quality: Option<u16>,
    preferred_video_codec: Option<PreferredVideoFormat>,
    preferred_audio_codec: Option<PreferredVideoFormat>,
}

pub struct Options {
    pub database: PathBuf,
    pub instances: PathBuf,
    pub videos_tab: bool,
    pub shorts_tab: bool,
    pub streams_tab: bool,
    pub api: ApiBackend,
    pub refresh_threshold: u64,
    pub rss_threshold: usize,
    pub tick_rate: u64,
    pub request_timeout: u64,
    pub highlight_symbol: String,
    pub video_player_for_stream_formats: VideoPlayer,
    pub mpv_path: PathBuf,
    pub vlc_path: PathBuf,
    pub hide_watched: bool,
    pub subtitle_languages: Vec<String>,
    pub prefer_dash_formats: bool,
    pub video_quality: u16,
    pub preferred_video_codec: PreferredVideoFormat,
    pub preferred_audio_codec: PreferredVideoFormat,
}

impl Options {
    pub fn override_with_clap_args(&mut self) {
        if let Some(database) = CLAP_ARGS.get_one::<PathBuf>("database") {
            self.database = database.to_owned();
        }

        if let Some(instances) = CLAP_ARGS.get_one::<PathBuf>("instances") {
            self.instances = instances.to_owned();
        }

        if let Some(tick_rate) = CLAP_ARGS.get_one::<u64>("tick_rate") {
            self.tick_rate = *tick_rate;
        }

        if let Some(request_timeout) = CLAP_ARGS.get_one::<u64>("request_timeout") {
            self.request_timeout = *request_timeout;
        }

        if let Some(highlight_symbol) = CLAP_ARGS.get_one::<String>("highlight_symbol") {
            self.highlight_symbol = highlight_symbol.to_owned();
        }

        // Deprecated
        if let Some(video_player) = CLAP_ARGS.get_one::<PathBuf>("video_player") {
            self.mpv_path = video_player.to_owned()
        }
    }
}

impl Default for Options {
    fn default() -> Self {
        Options {
            database: PathBuf::default(),
            instances: PathBuf::default(),
            videos_tab: true,
            shorts_tab: false,
            streams_tab: false,
            api: ApiBackend::Invidious,
            refresh_threshold: 600,
            rss_threshold: 125,
            tick_rate: 200,
            request_timeout: 5,
            highlight_symbol: String::new(),
            video_player_for_stream_formats: VideoPlayer::Mpv,
            mpv_path: PathBuf::from("mpv"),
            vlc_path: PathBuf::from("vlc"),
            hide_watched: false,
            subtitle_languages: Vec::new(),
            prefer_dash_formats: true,
            video_quality: u16::MAX,
            preferred_video_codec: PreferredVideoFormat::Mp4,
            preferred_audio_codec: PreferredVideoFormat::Mp4,
        }
    }
}

impl From<UserOptions> for Options {
    fn from(user_options: UserOptions) -> Self {
        let mut options = Options::default();

        macro_rules! set_options_field {
            ($name: ident) => {
                if let Some(option) = user_options.$name {
                    options.$name = option;
                }
            };
        }

        if let Some(tabs) = user_options.tabs {
            options.videos_tab = tabs.contains(&ChannelTab::Videos);
            options.shorts_tab = tabs.contains(&ChannelTab::Shorts);
            options.streams_tab = tabs.contains(&ChannelTab::Streams);
        }

        set_options_field!(database);
        set_options_field!(instances);
        set_options_field!(api);
        set_options_field!(refresh_threshold);
        set_options_field!(rss_threshold);
        set_options_field!(tick_rate);
        set_options_field!(request_timeout);
        set_options_field!(highlight_symbol);
        set_options_field!(hide_watched);
        set_options_field!(video_player_for_stream_formats);
        set_options_field!(mpv_path);
        set_options_field!(vlc_path);
        set_options_field!(subtitle_languages);
        set_options_field!(prefer_dash_formats);
        set_options_field!(video_quality);
        set_options_field!(preferred_video_codec);
        set_options_field!(preferred_audio_codec);

        options
    }
}

fn deserialize_video_quality<'de, D>(deserializer: D) -> Result<Option<u16>, D::Error>
where
    D: de::Deserializer<'de>,
{
    use serde::de::Error;

    let Some(quality_str): Option<String> = de::Deserialize::deserialize(deserializer)? else {
        return Ok(None);
    };

    Ok(Some(if quality_str.to_lowercase() == "best" {
        u16::MAX
    } else if let Some(Ok(quality)) = quality_str
        .strip_suffix('p')
        .map(|number| number.parse::<u16>())
    {
        quality
    } else if let Ok(quality) = quality_str.parse::<u16>() {
        quality
    } else {
        return Err(Error::custom(format!(
            "\"{quality_str}\" is not a valid quality"
        )));
    }))
}
