use clap::{Arg, ArgMatches, Command};

pub fn get_matches() -> ArgMatches {
    Command::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .help("Path to configuration file")
                .value_name("FILE"),
        )
        .arg(
            Arg::new("no_config")
                .short('n')
                .long("no-config")
                .help("Ignore configuration file")
                .conflicts_with("config"),
        )
        .arg(
            Arg::new("database")
                .short('d')
                .long("database")
                .help("Path to database file")
                .value_name("FILE"),
        )
        .arg(
            Arg::new("instances")
                .short('s')
                .long("instances")
                .help("Path to instances file")
                .value_name("FILE"),
        )
        .arg(
            Arg::new("gen_instances_list")
                .short('g')
                .long("gen-instances")
                .help("Generate Invidious instances file"),
        )
        .arg(
            Arg::new("tick_rate")
                .short('t')
                .long("tick-rate")
                .help("Tick rate in milliseconds")
                .value_name("TICK RATE")
                .validator(|s| s.parse::<u64>()),
        )
        .arg(
            Arg::new("request_timeout")
                .short('r')
                .long("request-timeout")
                .help("Timeout in seconds")
                .value_name("TIMEOUT")
                .validator(|s| s.parse::<u64>()),
        )
        .arg(
            Arg::new("highlight_symbol")
                .long("highlight-symbol")
                .help("Symbol to highlight selected items")
                .value_name("SYMBOL"),
        )
        .arg(
            Arg::new("video_player")
                .long("video-player")
                .help("Path to video player")
                .value_name("VIDEO PLAYER"),
        )
        .get_matches()
}
