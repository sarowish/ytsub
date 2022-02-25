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
}
