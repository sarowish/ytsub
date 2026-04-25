# Configuration

`ytsub` uses `config.toml` for application settings, theme, and key bindings,
and a separate `instances` file for Invidious instance configuration.

By default, both files live under `$HOME/.config/ytsub`. A different
`config.toml` path can be specified with the `-c` flag; the `instances` file path
can be set with the `instances` option in `config.toml` or with the `-s` flag.

If no `config.toml` is provided, `ytsub` is launched with default values.

An example configuration file with default values is available at [example/config.toml](../example/config.toml).
It is advisable to add only the parts you want to change instead of checking in the whole file.

## Table of Contents

- [Options](#options)
- [Theme](#theme)
- [Key Bindings](#key-bindings)
- [Instances File](#instances-file)

## Options

| Option                            | Description                                                                                                  | Default                                     |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------ | ------------------------------------------- |
| `database`                        | Path to database file.                                                                                       | `"/home/user/.local/share/ytsub/videos.db"` |
| `instances`                       | Path to instances file.                                                                                      | `"/home/user/.config/ytsub/instances"`      |
| `mode`                            | Default application mode: `subscriptions` (or `subs`) or `latest_videos`.                                    | `"subscriptions"`                           |
| `tabs`                            | Tabs to fetch videos from: `videos`, `shorts` or `streams`.                                                  | `["videos"]`                                |
| `hide_disabled_tabs`              | Hide tabs with locally available videos if they are not enabled in `tabs` option.                            | `true`                                      |
| `api`                             | Backend used for fetching videos: `local` or `invidious`.                                                    | `"local"`                                   |
| `refresh_threshold`               | Time in seconds that needs to pass before refreshing a channel with the `refresh_channels` command.          | `600`                                       |
| `refresh_on_launch`               | Refresh all channels on application launch.                                                                  | `true`                                      |
| `rss_threshold`                   | Use RSS if the number of channels being refreshed or subscribed to exceeds the threshold.                    | `9999`                                      |
| `tick_rate`                       | Tick rate in milliseconds. Determines how often the UI is redrawn for refresh status updates.                | `10`                                        |
| `request_timeout`                 | Network request timeout in seconds.                                                                          | `5`                                         |
| `highlight_symbol`                | Symbol used to highlight selected items.                                                                     | `""`                                        |
| `hide_watched`                    | Hide watched videos by default.                                                                              | `false`                                     |
| `hide_members_only`               | Hide members-only videos by default.                                                                         | `false`                                     |
| `show_thumbnails`                 | Show thumbnails in the video info area.                                                                      | `true`                                      |
| `video_info_position`             | Position of the video info area: `top` or `bottom`.                                                          | `"top"`                                     |
| `always_show_video_info`          | If `false`, shown only when there isn't enough space for all columns. Ignored when `show_thumbnails = true`. | `true`                                      |
| `video_player_for_stream_formats` | Video player used for stream formats: `mpv` or `vlc`.                                                        | `"mpv"`                                     |
| `mpv_path`                        | Path to `mpv`.                                                                                               | `"mpv"`                                     |
| `vlc_path`                        | Path to `vlc`.                                                                                               | `"vlc"`                                     |
| `subtitle_languages`              | Subtitle languages to add when available.                                                                    | `[]`                                        |
| `prefer_dash_formats`             | Prefer adaptive formats.                                                                                     | `true`                                      |
| `prefer_original_titles`          | Prefer video titles in their original language.                                                              | `true`                                      |
| `prefer_original_audio`           | Prefer original audio tracks.                                                                                | `true`                                      |
| `video_quality`                   | Preferred stream quality. Accepts `best`, `720p`, `720`, and similar values.                                 | `"best"`                                    |
| `preferred_video_codec`           | Preferred video container: `webm` or `mp4`.                                                                  | -                                           |
| `preferred_audio_codec`           | Preferred audio container: `webm` or `mp4`.                                                                  | -                                           |
| `chapters`                        | Extract chapter information when available.                                                                  | `true`                                      |

### Notes

- the options from `subtitle_languages` to `chapters` only apply when playing stream formats (`play_from_formats` command).

## Theme

Theme configuration is defined under the `[theme]` section.

A theme entry is a style object with these optional fields:

- `fg`: Foreground color
- `bg`: Background color
- `modifiers`: Space-separated style modifiers

**Color values**:

- Palette colors: `Black`, `Red`, `Green`, `Yellow`, `Blue`, `Magenta`, `Cyan`,
  `Gray`, `DarkGray`, `LightRed`, `LightGreen`, `LightYellow`, `LightBlue`,
  `LightMagenta`, `LightCyan`, `White`, `Reset`
- Hex colors: `#ffffff`
- RGB colors: `255, 255, 255`

**Available modifiers**: `bold`, `dim`, `italic`, `underlined`, `slow_blink`,
`rapid_blink`, `reversed`, `hidden`, `crossed_out`

### Theme Fields

`selected_watched` and `focused_watched` are patches over the normal selected
styles. Their modifiers replace the selected style modifiers, and any `fg` / `bg`
values are patched onto the selected style.

| Field                    | Used for                                                                              |
| ------------------------ | ------------------------------------------------------------------------------------- |
| `title`                  | Block and popup titles such as `Channels`, `Latest Videos`, `Video Info`, and `Help`. |
| `header`                 | Video table headers such as `Channel`, `Title`, `Length`, and `Date`.                 |
| `selected`               | Selected row in the inactive pane.                                                    |
| `focused`                | Selected row in the active pane.                                                      |
| `watched`                | Base style applied to watched videos.                                                 |
| `selected_watched`       | Patch applied when a watched video is selected in the inactive pane.                  |
| `focused_watched`        | Patch applied when a watched video is selected in the active pane.                    |
| `new_video_indicator`    | The `[N]` indicator shown for new videos and channels with new content.               |
| `members_only_indicator` | The `[M]` indicator shown for members-only videos.                                    |
| `selected_block`         | Border of the active pane.                                                            |
| `video_info`             | Field names in the `Video Info` panel.                                                |
| `error`                  | Error messages.                                                                       |
| `warning`                | Warning messages.                                                                     |
| `help`                   | Key names in help popups and inline help.                                             |

### Default Theme

```toml
[theme]
title = { fg = "Cyan", modifiers = "bold" }
header = { fg = "Yellow", modifiers = "bold" }
selected = { fg = "Blue", modifiers = "bold" }
focused = { fg = "Magenta", modifiers = "bold" }
watched = { fg = "DarkGray" }
selected_watched = {}
focused_watched = {}
new_video_indicator = { fg = "Red", modifiers = "italic" }
members_only_indicator = { fg = "Green", modifiers = "italic" }
selected_block = { fg = "Magenta" }
video_info = { fg = "Green" }
error = { fg = "Red" }
warning = { fg = "Yellow" }
help = { fg = "Green" }
```

## Key Bindings

Key bindings are defined in these sections:

- `[key_bindings]`
- `[key_bindings.help]`
- `[key_bindings.import]`
- `[key_bindings.tag]`
- `[key_bindings.channel_selection]`
- `[key_bindings.format_selection]`

Each section supports a different set of commands.

### Binding Syntax

Valid key codes are:

- Single characters: `e`, `r`, `2` etc.
- Special keys: `backspace`, `space`, `enter`, `left`, `right`, `up`, `down`,
  `home`, `end`, `pageup`, `pagedown`, `tab`, `backtab`, `del`, `delete`, `insert`, `esc`, `escape`,
- Bindings can use `ctrl`, `shift` and `alt` modifiers: `ctrl-h`, `ctrl-alt-d` etc.
  (For uppercase characters `shift` is not needed. Use the uppercase character itself.)
- Multiple keys can be assigned in a single line: `"q ctrl-c" = "quit"`
- A binding can be removed by assigning an empty string: `"q" = ""`

### General Commands

| Command                       | Description                                                         |
| ----------------------------- | ------------------------------------------------------------------- |
| `set_mode_subs`               | Switch to subscriptions mode                                        |
| `set_mode_latest_videos`      | Switch to latest videos mode                                        |
| `on_down`                     | Move one line downward                                              |
| `on_up`                       | Move one line upward                                                |
| `on_left`                     | Switch to channels block                                            |
| `on_right`                    | Switch to videos block                                              |
| `select_first`                | Jump to the first line                                              |
| `select_last`                 | Jump to the last line                                               |
| `page_up`                     | Move up one page                                                    |
| `page_down`                   | Move down one page                                                  |
| `half_page_up`                | Move up half a page                                                 |
| `half_page_down`              | Move down half a page                                               |
| `next_tab`                    | Select next tab                                                     |
| `previous_tab`                | Select previous tab                                                 |
| `jump_to_channel`             | Jump to the channel of the selected video from latest videos mode   |
| `toggle_hide`                 | Hide/unhide watched videos                                          |
| `subscribe`                   | Subscribe                                                           |
| `unsubscribe`                 | Unsubscribe                                                         |
| `delete_video`                | Delete the selected video from database                             |
| `search_forward`              | Search Forward                                                      |
| `search_backward`             | Search backward                                                     |
| `repeat_last_search`          | Repeat last search                                                  |
| `repeat_last_search_opposite` | Repeat last search in the opposite direction                        |
| `switch_api`                  | Switch API                                                          |
| `refresh_channel`             | Refresh videos of the selected channel                              |
| `refresh_channels`            | Refresh videos of every channel                                     |
| `refresh_failed_channels`     | Refresh videos of channels which their latest refresh was a failure |
| `load_more_videos`            | Load more videos                                                    |
| `load_all_videos`             | Load all videos                                                     |
| `copy_youtube_link`           | Copy channel or video Youtube link to clipboard                     |
| `copy_invidious_link`         | Copy channel or video Invidious link to clipboard                   |
| `open_in_youtube`             | Open channel or video Youtube page in browser                       |
| `open_in_invidious`           | Open channel or video Invidious page in browser                     |
| `play_from_formats`           | Play video in video player using stream formats                     |
| `play_using_ytdlp`            | Play video in mpv using yt-dlp                                      |
| `select_formats`              | Toggle format selection window                                      |
| `toggle_watched`              | Mark/unmark video as watched                                        |
| `toggle_help`                 | Toggle help window                                                  |
| `toggle_tag`                  | Toggle tag selection window                                         |
| `quit`                        | Quit application                                                    |

### Modal Commands

These sections configure key bindings for popup windows. Some general bindings,
such as movement or search, can also be used in these popups, but those are still
configured in `[key_bindings]`.

#### `[key_bindings.help]`

A help window for the current key bindings, opened with the `toggle_help` command.

| Command        | Description                     |
| -------------- | ------------------------------- |
| `scroll_up`    | Scroll upward.                  |
| `scroll_down`  | Scroll downward.                |
| `go_to_top`    | Jump to the top of the text.    |
| `go_to_bottom` | Jump to the bottom of the text. |
| `abort`        | Close the help window.          |

#### `[key_bindings.import]`

A picker for choosing which imported channels to subscribe to, shown after
running the `import` CLI subcommand.

| Command            | Description                      |
| ------------------ | -------------------------------- |
| `toggle_selection` | Toggle the selected import item. |
| `select_all`       | Select all import items.         |
| `deselect_all`     | Deselect all import items.       |
| `import`           | Import the selected items.       |

#### `[key_bindings.tag]`

A window for browsing, selecting, and managing tags, opened with the `toggle_tag` command.

| Command            | Description                                   |
| ------------------ | --------------------------------------------- |
| `create_tag`       | Start creating a tag.                         |
| `delete_tag`       | Delete the selected tag.                      |
| `rename_tag`       | Rename the selected tag.                      |
| `select_channels`  | Open the channel picker for the selected tag. |
| `toggle_selection` | Toggle the selected tag.                      |
| `select_all`       | Select all tags.                              |
| `deselect_all`     | Deselect all tags.                            |
| `abort`            | Close the tag window.                         |

#### `[key_bindings.channel_selection]`

A picker for assigning channels to the selected tag, opened from the tag window
with the `select_channels` command.

| Command            | Description                                      |
| ------------------ | ------------------------------------------------ |
| `confirm`          | Save the current channel selection for the tag.  |
| `abort`            | Return to the tag window without saving changes. |
| `toggle_selection` | Toggle the selected channel.                     |
| `select_all`       | Select all channels.                             |
| `deselect_all`     | Deselect all channels.                           |

#### `[key_bindings.format_selection]`

A window for choosing available video and audio formats and captions before
playing a video, opened with the `select_formats` command.

| Command              | Description                                              |
| -------------------- | -------------------------------------------------------- |
| `previous_tab`       | Move to the previous format tab.                         |
| `next_tab`           | Move to the next format tab.                             |
| `switch_format_type` | Switch between available format categories.              |
| `select`             | Select or toggle the current format entry.               |
| `play_video`         | Confirm the current format selection and play the video. |
| `abort`              | Close the format selection window.                       |

#### Fixed Prompt Keys

Not every key in `ytsub` is configurable through `[key_bindings]`:

- Unsubscribe confirmation: `y` confirms, `n` cancels
- Text-entry prompts:
  `left`, `right`, `ctrl-b`, `ctrl-f`, `ctrl-a`, `ctrl-e`, `ctrl-w`, `ctrl-u`,
  `ctrl-k`, `backspace`, `ctrl-h`, `enter`, and `esc`

## Instances File

The `instances` file is used when `ytsub` needs an Invidious instance, either
because `api = "invidious"` is configured or because you switch to the
Invidious backend at runtime.

The resolved path is chosen in this order:

- `-s`, `--instances <FILE>`
- `instances = "/path/to/instances"` in `config.toml`
- the default path: `$XDG_CONFIG_HOME/ytsub/instances`

### Generating the File

> [!NOTE]
> Suitable API-enabled instances on [api.invidious.io](https://api.invidious.io/)
> have often been scarce
> for a long time, and the API may return very few or no usable instances.

You can generate the file with `ytsub -g` or `ytsub --gen-instances`.

Generation uses the resolved `instances` path above, creates the parent
directory if needed, and writes API-enabled non-onion instances from
`https://api.invidious.io/instances.json`.

### File Format

The file is plain text, with one Invidious URL per line.

### Example File

```text
https://vid.puffyan.us
https://invidio.xamh.de
https://inv.riverside.rocks
https://yt.artemislena.eu
```
