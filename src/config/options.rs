use crate::{
    api::{ApiBackend, ChannelTab},
    CLAP_ARGS,
};
use serde::Deserialize;
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
    video_player: Option<String>,
    hide_watched: Option<bool>,
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
    pub video_player: String,
    pub hide_watched: bool,
}

impl Options {
    pub fn override_with_clap_args(&mut self) {
        if let Some(database) = CLAP_ARGS.get_one::<String>("database") {
            self.database = PathBuf::from(database);
        }

        if let Some(instances) = CLAP_ARGS.get_one::<String>("instances") {
            self.instances = PathBuf::from(instances);
        }

        if let Some(tick_rate) = CLAP_ARGS.get_one::<u64>("tick_rate") {
            self.tick_rate = *tick_rate;
        }

        if let Some(request_timeout) = CLAP_ARGS.get_one::<u64>("request_timeout") {
            self.request_timeout = *request_timeout;
        }

        if let Some(highlight_symbol) = CLAP_ARGS.get_one::<String>("highlight_symbol") {
            self.highlight_symbol = highlight_symbol.to_string();
        }

        if let Some(video_player) = CLAP_ARGS.get_one::<String>("video_player") {
            self.video_player = video_player.to_string();
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
            video_player: String::from("mpv"),
            hide_watched: false,
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
        set_options_field!(video_player);
        set_options_field!(hide_watched);

        options
    }
}
