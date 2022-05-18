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
    -c, --config <FILE>                        Path to configuration file
    -d, --database <FILE>                      Path to database file
    -g, --gen-instance-list                    Generate invidious instances file
    -h, --help                                 Print help information
        --highlight-symbol <SYMBOL>            Symbol to highlight selected items
    -i, --instances <FILE>                     Path to instances file
    -n, --no-config                            Ignore configuration file
    -r, --request-timeout <REQUEST_TIMEOUT>    Timeout in secs
    -t, --tick-rate <TICK_RATE>                Tick rate in milliseconds
    -V, --version                              Print version information
        --video-player <PATH>                  Path to the video player
```

## Configuration

Default directory of the configuration files is `$HOME/.config/ytsub`.

| File          | Description                                   |
|---------------|-----------------------------------------------|
| `config.toml` | option, key binding and theme configurations  |
| `instances`   | list of invidious instances                   |

### `config.toml`

Path to the configuration file can be specified with the `-c` flag.

#### Example `config.toml` with default values

```toml
# Options

database = "/home/username/.local/share/ytsub/videos.db" # Path to database file
instances = "/home/username/.config/ytsub/instances" # Path to instances file
tick_rate = 200 # Tick rate in milliseconds
request_timeout = 5 # Request timeout in seconds
highlight_symbol = "" # Symbol to highlight selected items
video_player = "mpv" # Path to video player
hide_watched = false # Hide watched videos by default

# Theme

# fg and bg fields can be set with rgb (ex: "255, 255, 255"),
# hex (ex: "#ffffff") or named colors.
# Valid color names are Black, Red, Green, Yellow, Blue, Magenta
# Cyan, Gray, DarkGray, LightRed, LightGreen, LightGreen,
# LightYellow, LightBlue, LightMagenta, LightCyan, White and Reset.

# Valid modifiers are bold, dim, italic, underlined,
# slow_blink, rapid_blink, reversed, hidden and crossed_out.

# Example:
# [title]
# fg = "#123456"
# bg = "10, 250, 99"
# modifiers = "bold reversed italic"

# Block titles
[title]
fg = "Cyan"
modifiers = "bold"
# Channel, Title, Length and Date headers
[header]
fg = "Yellow"
modifiers = "bold"
# Selected item in inactive block
[selected]
fg = "Blue"
modifiers = "bold"
# Selected item in active block
[focused]
fg = "Magenta"
modifiers = "bold"
# Watched videos
[watched]
fg = "DarkGray"
# Selected watched video in inactive block
# Overrides the modifiers of [selected]. If fg and bg are set, they are patched to [selected]
[selected_watched]
# Selected watched video in active block
# Overrides the modifiers of [focused]. If fg and bg are set, they are patched to [focused]
[focused_watched]
# Border of selected block
[selected_block]
fg = "Magenta"
# Error message
[error]
fg = "Red"

# Key Bindings

# Valid key codes are backspace, enter, left, right, up, down, home, end
# pageup, pagedown, tab, backtab, del, delete, insert, esc, escape and characters.

# Valid modifiers are ctrl, shift and alt.

# Multiple key bindings can be set in a single line.
# Example: "escape q ctrl-c" = "quit"

# A default binding can be removed by setting it to an empty string.
# Example: "q" = ""

[key_bindings]
"1" = "set_mode_subs" # Switch to subscriptions mode
"2" = "set_mode_latest_videos" # Switch to latest videos mode
"j down" = "on_down" # Go one line downward
"k up" = "on_up" # Go one line upward
"h left" = "on_left" # Switch to channels block
"l right" = "on_right" # Switch to videos block
"g" = "select_first" # Jump to the first line in the list
"G" = "select_last" # Jump to the last line in the list
"c" = "jump_to_channel" # Jump to the channel of the selected video from latest videos mode
"t" = "toggle_hide" # Hide/unhide watched videos
"i" = "subscribe" # Enter editing mode to enter channel id or url
"d" = "unsubscribe" # Open confirmation window to unsubcribe from the selected channel
"/" = "search_forward" # Enter editing mode to make a forward search
"?" = "search_backward" # Enter editing mode to make a backward search
"n" = "repeat_last_search" # Search with the latest pattern and direction
"N" = "repeat_last_search_opposite" # Search with the latest pattern and opposite direction
"r" = "refresh_channel" # Refresh videos of the selected channel
"R" = "refresh_channels" # Refresh videos of every channel
"F" = "refresh_failed_channels" # Refresh videos of channels which their latest refresh was a failure
"o" = "open_in_browser" # Open channel or video on browser
"p" = "play_video" # Play selected video in a video player (default: mpv)
"m" = "toggle_watched" # Mark/unmark selected video as watched
"q ctrl-c" = "quit" # Quit application
```

### `instances`

A custom path to the `instances` file can be specified with the `-i` flag.
The file can either be manually created by adding instances that have api enabled or
automatically generated from the instances in https://api.invidious.io by running the app with `-g` flag.
Every instance entry is separated by a line.
If the `instances` file doesn't exist, every time the app is launched,
instances list will be built from https://api.invidious.io.

#### Example `instances` file

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
| `o`                  | open channel or video in browser             |
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
