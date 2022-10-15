use clap::{Arg, ArgAction, ArgMatches, Command};

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
                .conflicts_with("config")
                .action(ArgAction::SetTrue),
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
                .help("Generate Invidious instances file")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("tick_rate")
                .hide(true)
                .short('t')
                .long("tick-rate")
                .help("Tick rate in milliseconds")
                .value_name("TICK RATE")
                .value_parser(clap::value_parser!(u64)),
        )
        .arg(
            Arg::new("request_timeout")
                .hide(true)
                .short('r')
                .long("request-timeout")
                .help("Timeout in seconds")
                .value_name("TIMEOUT")
                .value_parser(clap::value_parser!(u64)),
        )
        .arg(
            Arg::new("highlight_symbol")
                .hide(true)
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
        .subcommand(
            Command::new("import")
                .about("Import subscriptions")
                .arg(
                    Arg::new("format")
                        .short('f')
                        .long("format")
                        .help("Format of the import file")
                        .value_name("FORMAT")
                        .default_value("youtube_csv")
                        .value_parser(["youtube_csv", "newpipe"]),
                )
                .arg(
                    Arg::new("source")
                        .help("Path to the import file")
                        .value_name("FILE")
                        .required(true),
                ),
        )
        .subcommand(
            Command::new("export")
                .about("Export subscriptions")
                .arg(
                    Arg::new("format")
                        .short('f')
                        .long("format")
                        .help("Format of the export file")
                        .value_name("FORMAT")
                        .default_value("youtube_csv")
                        .value_parser(["youtube_csv", "newpipe"]),
                )
                .arg(
                    Arg::new("target")
                        .help("Path to the export file")
                        .value_name("FILE")
                        .required(true),
                ),
        )
        .get_matches()
}
