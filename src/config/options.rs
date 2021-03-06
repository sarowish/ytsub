use crate::CLAP_ARGS;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
pub struct UserOptions {
    pub database: Option<PathBuf>,
    pub instances: Option<PathBuf>,
    pub tick_rate: Option<u64>,
    pub request_timeout: Option<u64>,
    pub highlight_symbol: Option<String>,
    pub video_player: Option<String>,
    pub hide_watched: Option<bool>,
}

pub struct Options {
    pub database: PathBuf,
    pub instances: PathBuf,
    pub tick_rate: u64,
    pub request_timeout: u64,
    pub highlight_symbol: String,
    pub video_player: String,
    pub hide_watched: bool,
}

impl Options {
    pub fn override_with_clap_args(&mut self) {
        if let Some(database) = CLAP_ARGS.value_of("database") {
            self.database = PathBuf::from(database);
        }

        if let Some(instances) = CLAP_ARGS.value_of("instances") {
            self.instances = PathBuf::from(instances);
        }

        if let Some(tick_rate) = CLAP_ARGS.value_of("tick_rate") {
            self.tick_rate = tick_rate.parse().unwrap();
        }

        if let Some(request_timeout) = CLAP_ARGS.value_of("instances") {
            self.request_timeout = request_timeout.parse().unwrap();
        }

        if let Some(highlight_symbol) = CLAP_ARGS.value_of("highlight_symbol") {
            self.highlight_symbol = highlight_symbol.to_string();
        }

        if let Some(video_player) = CLAP_ARGS.value_of("video_player") {
            self.video_player = video_player.to_string();
        }
    }
}

impl Default for Options {
    fn default() -> Self {
        Options {
            database: PathBuf::default(),
            instances: PathBuf::default(),
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

        set_options_field!(database);
        set_options_field!(instances);
        set_options_field!(tick_rate);
        set_options_field!(request_timeout);
        set_options_field!(highlight_symbol);
        set_options_field!(video_player);
        set_options_field!(hide_watched);

        options
    }
}
