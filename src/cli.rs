use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[clap(about, version)]
pub struct Options {
    /// Path to database file
    #[clap(short, long, parse(from_os_str), value_name = "FILE")]
    pub database_path: Option<PathBuf>,
    /// Generate invidious instances file
    #[clap(short, long)]
    pub gen_instance_list: bool,
    /// Tick rate in milliseconds
    #[clap(short, long, default_value_t = 200)]
    pub tick_rate: u64,
    /// Timeout in secs
    #[clap(short, long, default_value_t = 5)]
    pub request_timeout: u64,
    /// Symbol to highlight selected items
    #[clap(long, default_value = "", value_name = "SYMBOL")]
    pub highlight_symbol: String,
    /// Path to the video player
    #[clap(long, default_value = "mpv", value_name = "PATH")]
    pub video_player_path: String,
}
