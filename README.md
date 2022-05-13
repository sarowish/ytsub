# ytsub

ytsub is a subscriptions only tui youtube client that uses the invidious api.

![subscriptions mode](screenshots/subscriptions.png)
![latest videos mode](screenshots/latest_videos.png)

## Dependencies

`mpv` and `yt-dlp` are optional dependencies for playing videos.

`sqlite` is a required dependency. If it is not available on the system,
it can be compiled and linked by enabling
the `bundled_sqlite` feature when building with cargo:

```
cargo build --features bundled_sqlite
```

## Installation

```bash
cargo install ytsub
# or
cargo install ytsub --features bundled_sqlite
```

## Usage

```
USAGE:
    ytsub [OPTIONS]

OPTIONS:
    -d, --database-path <FILE>                 Path to database file
    -g, --gen-instance-list                    Generate invidious instances file
    -h, --help                                 Print help information
        --highlight-symbol <SYMBOL>            Symbol to highlight selected items [default: ]
    -r, --request-timeout <REQUEST_TIMEOUT>    Timeout in secs [default: 5]
    -t, --tick-rate <TICK_RATE>                Tick rate in milliseconds [default: 200]
    -V, --version                              Print version information
        --video-player-path <PATH>             Path to the video player [default: mpv]
```

## Configuration

Default directory of the configuration files is `~/.config/ytsub`.

| File        | Description                             |
|-------------|-----------------------------------------|
| `instances` | includes the invidious instances        |

If the `instances` file doesn't exist, every time you open the app, instances list will be built from https://api.invidious.io/.
You can either manually create the file and add instances that have api enabled or
automatically generate it from the instances in https://api.invidious.io/ by running the app with `-g` flag.
Every entry is separated by a line.

#### Example `instances` file:

```
https://vid.puffyan.us
https://invidio.xamh.de
https://inv.riverside.rocks
https://yt.artemislena.eu
```

## Key Bindings

| Key Binding          | Action                                       |
| -------------------- | -------------------------------------------- |
| `h/l,left/right`     | switch to channels/videos block              |
| `k/j,up/down`        | go one line upward/downward                  |
| `g`                  | go to first line                             |
| `G`                  | go to last line                              |
| `c`                  | jump to channel from latest videos mode      |
| `1`                  | switch to subscriptions mode                 |
| `2`                  | switch to latest videos mode                 |
| `o`                  | open video in browser                        |
| `p`                  | play video in mpv                            |
| `t`                  | toggle hide                                  |
| `i`                  | subscribe                                    |
| `d`                  | unsubscribe                                  |
| `/`                  | search forward                               |
| `?`                  | search backward                              |
| `n`                  | repeat last search                           |
| `N`                  | repeat last search in the opposite direction |
| `r`                  | refresh selected channel                     |
| `R`                  | refresh all channels                         |
| `F`                  | retry refreshing failed channels             |
| `m`                  | toggle watched/unwatched                     |
| `q`,`ctrl+c`         | quit application                             |
