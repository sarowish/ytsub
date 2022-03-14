use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
pub struct Options {
    /// path to database file
    #[clap(short, long, parse(from_os_str), value_name = "FILE")]
    pub database_path: Option<PathBuf>,
    /// path to subscriptions file
    #[clap(short, long, parse(from_os_str), value_name = "FILE")]
    pub subs_path: Option<PathBuf>,
    /// generate invidious instances file
    #[clap(short, long)]
    pub gen_instance_list: bool,
    /// tick rate in milliseconds
    #[clap(short, long, default_value_t = 200)]
    pub tick_rate: u64,
    /// timeout in secs
    #[clap(short, long, default_value_t = 5)]
    pub request_timeout: u64,
}
