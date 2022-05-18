use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[clap(about, version)]
pub struct Args {
    /// Path to configuration file
    #[clap(short, long, parse(from_os_str), value_name = "FILE")]
    pub config: Option<PathBuf>,
    /// Ignore configuration file
    #[clap(short, long)]
    pub no_config: bool,
    /// Path to database file
    #[clap(short, long, parse(from_os_str), value_name = "FILE")]
    pub database: Option<PathBuf>,
    /// Path to instances file
    #[clap(short, long, parse(from_os_str), value_name = "FILE")]
    pub instances: Option<PathBuf>,
    /// Generate invidious instances file
    #[clap(short, long)]
    pub gen_instance_list: bool,
    /// Tick rate in milliseconds
    #[clap(short, long)]
    pub tick_rate: Option<u64>,
    /// Timeout in secs
    #[clap(short, long)]
    pub request_timeout: Option<u64>,
    /// Symbol to highlight selected items
    #[clap(long, value_name = "SYMBOL")]
    pub highlight_symbol: Option<String>,
    /// Path to the video player
    #[clap(long, value_name = "PATH")]
    pub video_player: Option<String>,
}
