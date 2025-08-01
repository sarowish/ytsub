# ytsub

ytsub is a subscriptions only tui youtube client.

![demo](https://github.com/user-attachments/assets/974c8488-7963-4ed1-b300-2d7da8d460cc)

## Dependencies

`mpv` and `yt-dlp` are optional dependencies for video playback. `yt-dlp` is not
needed for playback when using the `play_from_formats` command.

`sqlite` is a required dependency. If it is not available on the system,
it can be compiled and linked by enabling
the `bundled_sqlite` feature when building with cargo:

```
cargo build --features bundled_sqlite
```

## Installation

### Cargo

```bash
cargo install ytsub

# or
cargo install ytsub --features bundled_sqlite
```

### Arch Linux

`ytsub` is available in the AUR as
[stable source package](https://aur.archlinux.org/packages/ytsub),
[stable binary package](https://aur.archlinux.org/packages/ytsub-bin) and
[latest development package](https://aur.archlinux.org/packages/ytsub-git).
You can install it using your preferred AUR helper.

```bash
# stable source package
paru -S ytsub

# stable binary package
paru -S ytsub-bin

# latest development package
paru -S ytsub-git
```

## Usage

```
Usage: ytsub [OPTIONS] [COMMAND]

Commands:
  import  Import subscriptions
  export  Export subscriptions
  help    Print this message or the help of the given subcommand(s)

Options:
  -c, --config <FILE>     Path to configuration file
  -n, --no-config         Ignore configuration file
  -d, --database <FILE>   Path to database file
  -s, --instances <FILE>  Path to instances file
  -g, --gen-instances     Generate Invidious instances file
  -h, --help              Print help
  -V, --version           Print version
```

For default key bindings, press `ctrl-h` in the app or refer to
the [example `config.toml` file](#example-configtoml-with-default-values).

### Subscribing
Pressing `i` prompts the user to enter a channel id or url. 

#### Example inputs

- `UCsXVk37bltHxD1rDPwtNM8Q`
- `https://youtube.com/channel/UCsXVk37bltHxD1rDPwtNM8Q`
- `<INVIDIOUS_INSTANCE>/channel/UCsXVk37bltHxD1rDPwtNM8Q`
- `@kurzgesagt`
- `https://youtube.com/@kurzgesagt`

## Configuration

Default directory of the configuration files is `$HOME/.config/ytsub`.

| File                         | Description                                   |
|------------------------------|-----------------------------------------------|
| [`config.toml`](#configtoml) | option, key binding and theme configurations  |
| [`instances`](#instances)    | list of invidious instances                   |

### `config.toml`

Path to the configuration file can be specified with the `-c` flag.

#### Example `config.toml` with default values

```toml
# Options

database = "/home/username/.local/share/ytsub/videos.db" # Path to database file
instances = "/home/username/.config/ytsub/instances" # Path to instances file
tabs = ["videos"] # Tabs to fetch videos from [possible values: videos, shorts, streams]
api = "local" # API to be used for fetching videos [possible values: invidious, local]
refresh_threshold = 600 # Time in seconds that needs to pass before refreshing a channel using the refresh_channels command
rss_threshold = 125 # Use RSS if the number of channels being refreshed or being subscribed to exceeds the specified amount
tick_rate = 10 # Tick rate in milliseconds
request_timeout = 5 # Request timeout in seconds
highlight_symbol = "" # Symbol to highlight selected items
hide_watched = false # Hide watched videos by default
video_player_for_stream_formats = "mpv" # Video player to use to play streams [possible values: mpv, vlc]
mpv_path = "mpv" # Path to mpv
vlc_path = "vlc" # Path to vlc

# the options below apply only while playing stream formats
subtitle_languages = [] # Subtitle languages to add
prefer_dash_formats = true # Use adaptive formats
video_quality = "best" # Preferred video quality
# the commented out options below are `None` by default
# preferred_video_codec = "webm" # [possible values: webm, mp4]
# preferred_audio_codec = "webm"
chapters = true # Get chapter information of videos

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

# Example with alternative syntax:
# title = { fg = "#123456", bg = "10, 250, 99", modifiers = "bold reversed italic" }

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
# New video indicator
[new_video_indicator]
fg = "Red"
modifiers = "italic"
# Border of selected block
[selected_block]
fg = "Magenta"
# Error message
[error]
fg = "Red"
# Warning message
[warning]
fg = "Yellow"
# Key bindings in the help window
[help]
fg = "Green"

# Key Bindings

# Valid key codes are backspace, space, enter, left, right, up, down, home, end
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
"D" = "delete_video" # Delete the selected video from database
"/" = "search_forward" # Enter editing mode to make a forward search
"?" = "search_backward" # Enter editing mode to make a backward search
"n" = "repeat_last_search" # Search with the latest pattern and direction
"N" = "repeat_last_search_opposite" # Search with the latest pattern and opposite direction
"s" = "switch_api" # Switch between the available APIs
"r" = "refresh_channel" # Refresh videos of the selected channel
"R" = "refresh_channels" # Refresh videos of every channel
"J" = "load_more_videos" # Load more videos for the selected channel
"F" = "refresh_failed_channels" # Refresh videos of channels which their latest refresh was a failure
"o" = "open_in_youtube" # Open channel or video Youtube page in browser
"O" = "open_in_invidious" # Open channel or video Invidious page in browser
"p" = "play_from_formats" # Play selected video in a video player using stream formats
"P" = "play_using_ytdlp" # Play selected video in mpv using yt-dlp
"f" = "select_formats" # Toggle format selection window
"m" = "toggle_watched" # Mark/unmark selected video as watched
"ctrl-h" = "toggle_help" # Toggle help window
"T" = "toggle_tag" # Toggle tag window
"q ctrl-c" = "quit" # Quit application

[key_bindings.help]
"ctrl-y" = "scroll_up" #general "on_up" binding also applies
"ctrl-e" = "scroll_down" # general "on_down" binding also applies
"g" = "go_to_top" # general "select_first" binding also applies
"G" = "go_to_bottom" # general "select_last" binding also applies
"escape" = "abort"

[key_bindings.import]
"space" = "toggle_selection" # Select/Unselect channel
"a" = "select_all" # Select all channels
"z" = "deselect_all" # Deselect all channels
"enter" = "import" # Import selected channels

[key_bindings.tag]
"i" = "create_tag"
"d" = "delete_tag"
"r" = "rename_tag"
"s" = "select_channels" # Pick channels for the tag
"space" = "toggle_selection" # Select/Unselect tag
"a" = "select_all" # Select all tags
"z" = "deselect_all" # Deselect all tags
"escape" = "abort" # Close window

[key_bindings.channel_selection]
"enter" = "confirm" # Confirm the selection of channels
"escape" = "abort" # Drop changes
"space" = "toggle_selection" # Select/Unselect channel
"a" = "select_all" # Select all channels
"z" = "deselect_all" # Deselect all channels

[key_bindings.format_selection]
"enter" = "play_video"
"escape" = "abort"
"space" = "select"
"l right" = "next_tab"
"h left" = "previous_tab"
"s" = "switch_format_type"
```

### `instances`

> [!NOTE]
> At the time of writing, there are no suitable instances available on https://api.invidious.io,
> and they've rarely been available for quite a while now.

A custom path to the `instances` file can be specified with the `-i` flag.
The file can either be manually created by adding instances that have API enabled or
automatically generated from the instances in https://api.invidious.io by running the app with `-g` flag.
Every instance entry is separated by a line.
If the `instances` file doesn't exist, instances list will be built from https://api.invidious.io when
Invidious is selected as the API (as opposed to the local API).

#### Example `instances` file

```
https://vid.puffyan.us
https://invidio.xamh.de
https://inv.riverside.rocks
https://yt.artemislena.eu
```
