# ytsub

ytsub is a subscriptions only tui youtube client that uses the invidious api.

![subscriptions mode](screenshots/subscriptions.png)
![latest videos mode](screenshots/latest_videos.png)

## Installation

`mpv` and `yt-dlp` are optional dependencies to play videos.

```
cargo install ytsub --git https://codeberg.org/sarowish/ytsub
```

## Usage

```
ytsub

USAGE:
    ytsub [OPTIONS]

OPTIONS:
    -d, --database-path <FILE>                 path to database file
    -g, --gen-instance-list                    generate invidious instances file
    -h, --help                                 Print help information
        --highlight-symbol <SYMBOL>            symbol to highlight selected items [default: ]
    -r, --request-timeout <REQUEST_TIMEOUT>    timeout in secs [default: 5]
    -t, --tick-rate <TICK_RATE>                tick rate in milliseconds [default: 200]
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
| `m`                  | toggle watched/unwatched                     |
| `q`,`ctrl+c`         | quit application                             |
