# ytsub

ytsub is a subscriptions only tui youtube client.

![demo](https://github.com/user-attachments/assets/64c17027-8a6d-4f46-8c55-742a6ad9c5c8)

## Dependencies

`mpv` and `yt-dlp` are optional dependencies for video playback. `yt-dlp` is not
needed for playback when using the `play_from_formats` command.

`ueberzugpp` and `chafa` are optional dependencies for thumbnail
support when the terminal does not provide a native graphics protocol.

`xclip`, `xsel`, `wl-clipboard` and `wayclip` are optional dependencies for
clipboard support on Linux.

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

[ytsub](https://aur.archlinux.org/packages/ytsub),
[ytsub-bin](https://aur.archlinux.org/packages/ytsub-bin) and
[ytsub-git](https://aur.archlinux.org/packages/ytsub-git) packages are available in the AUR.
You can install one of them using your preferred AUR helper.

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

### Managing subscriptions

#### Subscribing

Pressing `i` prompts the user to enter a channel id or url.

##### Example inputs

- `UCsXVk37bltHxD1rDPwtNM8Q`
- `https://youtube.com/channel/UCsXVk37bltHxD1rDPwtNM8Q`
- `<INVIDIOUS_INSTANCE>/channel/UCsXVk37bltHxD1rDPwtNM8Q`
- `@kurzgesagt`
- `https://youtube.com/@kurzgesagt`

#### Importing subscriptions

Subscriptions can be imported with:

```bash
ytsub import [--format FORMAT] <FILE>
```

Supported formats are:

- `youtube_csv` (default) for YouTube subscription exports
- `newpipe` for NewPipe subscription exports

##### From YouTube / Google Takeout

1. Go to [Google Takeout](https://takeout.google.com/takeout/custom/youtube).
2. Sign in if prompted.
3. Under `Create a New Export`, make sure `YouTube and YouTube Music` is selected.
4. Click `All YouTube data included`.
5. Click `Deselect all`, then enable only `subscriptions` and click `OK`.
6. Click `Next step`, then `Create export`.
7. Download the generated `.zip` file when it is ready.
8. Extract the archive and locate `Takeout/YouTube and YouTube Music/subscriptions/subscriptions.csv`
   (the directory and file names may differ depending on your language settings).
9. Import the extracted CSV with:

```bash
ytsub import "Takeout/YouTube and YouTube Music/subscriptions/subscriptions.csv"
```

##### From NewPipe

1. In NewPipe, go to the subscriptions page.
2. Tap the three-dot menu in the top-right corner, then choose `Export to`, then `File`.
3. Transfer the exported file to the device where you use `ytsub`.
4. Import it with:

```bash
ytsub import --format newpipe <newpipe-subscriptions-file>
```

### Key Bindings

The table below lists the default general key bindings.

Press `ctrl-h` in the app to open the binding help window.
Window-specific bindings are shown as inline help in popup windows.

| Command                       | Description                                                         | Default keys  |
| ----------------------------- | ------------------------------------------------------------------- | ------------- |
| `set_mode_subs`               | Switch to subscriptions mode                                        | `1`           |
| `set_mode_latest_videos`      | Switch to latest videos mode                                        | `2`           |
| `on_down`                     | Move one line downward                                              | `j`, `down`   |
| `on_up`                       | Move one line upward                                                | `k`, `up`     |
| `on_left`                     | Switch to channels block                                            | `h`, `left`   |
| `on_right`                    | Switch to videos block                                              | `l`, `right`  |
| `select_first`                | Jump to the first line                                              | `g`           |
| `select_last`                 | Jump to the last line                                               | `G`           |
| `page_up`                     | Move up one page                                                    | `ctrl-b`      |
| `page_down`                   | Move down one page                                                  | `ctrl-f`      |
| `half_page_up`                | Move up half a page                                                 | `ctrl-u`      |
| `half_page_down`              | Move down half a page                                               | `ctrl-d`      |
| `next_tab`                    | Select next tab                                                     | `L`           |
| `previous_tab`                | Select previous tab                                                 | `H`           |
| `jump_to_channel`             | Jump to the channel of the selected video from latest videos mode   | `c`           |
| `toggle_hide`                 | Hide/unhide watched videos                                          | `t`           |
| `subscribe`                   | Subscribe                                                           | `i`           |
| `unsubscribe`                 | Unsubscribe                                                         | `d`           |
| `delete_video`                | Delete the selected video from database                             | `D`           |
| `search_forward`              | Search Forward                                                      | `/`           |
| `search_backward`             | Search backward                                                     | `?`           |
| `repeat_last_search`          | Repeat last search                                                  | `n`           |
| `repeat_last_search_opposite` | Repeat last search in the opposite direction                        | `N`           |
| `switch_api`                  | Switch API                                                          | `s`           |
| `refresh_channel`             | Refresh videos of the selected channel                              | `r`           |
| `refresh_channels`            | Refresh videos of every channel                                     | `R`           |
| `refresh_failed_channels`     | Refresh videos of channels which their latest refresh was a failure | `F`           |
| `load_more_videos`            | Load more videos                                                    | `J`           |
| `load_all_videos`             | Load all videos                                                     | `ctrl-j`      |
| `copy_youtube_link`           | Copy channel or video Youtube link to clipboard                     | `y`           |
| `copy_invidious_link`         | Copy channel or video Invidious link to clipboard                   | `Y`           |
| `open_in_youtube`             | Open channel or video Youtube page in browser                       | `o`           |
| `open_in_invidious`           | Open channel or video Invidious page in browser                     | `O`           |
| `play_from_formats`           | Play video in video player using stream formats                     | `p`           |
| `play_using_ytdlp`            | Play video in mpv using yt-dlp                                      | `P`           |
| `select_formats`              | Toggle format selection window                                      | `f`           |
| `toggle_watched`              | Mark/unmark video as watched                                        | `m`           |
| `toggle_help`                 | Toggle help window                                                  | `ctrl-h`      |
| `toggle_tag`                  | Toggle tag selection window                                         | `T`           |
| `quit`                        | Quit application                                                    | `q`, `ctrl-c` |

To configure the key bindings, see the
[key bindings section in the configuration documentation](docs/configuration.md#key-bindings).

## Thumbnails

Video thumbnails can be displayed inside the video info area. The rendering
method is selected automatically based on terminal support and the availability
of external tools.

Supported native graphics protocols are
[Sixel](https://www.vt100.net/docs/vt3xx-gp/chapter14.html),
[Inline Images Protocol](https://iterm2.com/documentation-images.html) and
[Kitty Graphics Protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol).
Support is detected using terminal queries, while environment variables are
also used as hints when choosing a protocol.

Terminal cell size is detected from terminal responses when available and
otherwise derived from the window size. If no native graphics protocol is
available, rendering falls back to
[`ueberzugpp`](https://github.com/jstkdng/ueberzugpp), then
[`chafa`](https://hpjansson.org/chafa/), and finally an internal half-block
renderer that uses Unicode half-block symbols.

Downloaded thumbnails are cached under the ytsub cache directory in
`thumbnail/` (for example `~/.cache/ytsub/thumbnail` on Linux).

### Tested Terminal Emulators

| Terminal         | Protocol                  | Works | Notes                                                             |
| ---------------- | ------------------------- | :---: | ----------------------------------------------------------------- |
| kitty            | `Kitty Graphics Protocol` |  ✔️   | -                                                                 |
| Ghostty          | `Kitty Graphics Protocol` |  ✔️   | -                                                                 |
| foot             | `Sixel`                   |  ✔️   | -                                                                 |
| Contour          | `Sixel`                   |  ✔️   | -                                                                 |
| xterm            | `Sixel`                   |  ✔️   | Launch with `-ti 340` to make sure sixel support is enabled.      |
| BlackBox         | `Sixel`                   |  ✔️   | Requires `Sixel support` at compilation and in preferences.       |
| Windows Terminal | `Sixel`                   |  ✔️   | -                                                                 |
| WezTerm          | `Inline Images Protocol`  |  ✔️   | Also supports `Sixel`, but images seem to be misplaced sometimes. |
| Rio              | `Inline Images Protocol`  |  ✔️   | Also supports `Sixel`.                                            |
| Warp             | `Inline Images Protocol`  |  ✔️   | -                                                                 |
| mlterm           | `Inline Images Protocol`  |  ✔️   | Also supports `Sixel`.                                            |
| Tabby            | `Inline Images Protocol`  |  ✔️   | Also supports `Sixel`.                                            |
| Bobcat           | `Inline Images Protocol`  |  ✔️   | Also supports `Sixel`. `Inline Images` option should be enabled.  |
| Konsole          | `Sixel`                   |  ❌   | Also supports `iip`. Images aren't cleared properly.              |
| VSCode           | `Sixel`                   |  ❌   | Also supports `iip`. Images aren't cleared properly.              |
| ctx              | `Sixel`                   |  ❌   | -                                                                 |

### tmux

To make thumbnail rendering work correctly in `tmux`, add the following options
to your `tmux.conf`:

```tmux
set -g allow-passthrough on
set -ga update-environment TERM
set -ga update-environment TERM_PROGRAM
```

Then restart `tmux`:

```bash
tmux kill-server && tmux || tmux
```

## Clipboard

For clipboard support, one of the following providers is used if available:

- Wayland: `wl-clipboard` or `wayclip`
- X11: `xclip` or `xsel`
- macOS: `pbcopy`
- Windows: native clipboard API

If no clipboard provider is available, or if invoking the clipboard provider
fails, ytsub falls back to OSC52 for copying.

> [!WARNING]
> OSC52 capability is queried through DA1 and XTGETTCAP, but some terminals do
> not report it despite supporting it. Because of this, OSC52 sequences are
> sent regardless of detection.
>
> The status message after pressing `y` or `Y` is one of:
>
> - `Copied: {link}`
> - `OSC52 copy sent: {link}`
>
> The second message is shown when support could not be confirmed.
> This means an OSC52 copy sequence was sent, but actual copying may not occur.

## Configuration

By default, `config.toml` and the `instances` file are read from
`$HOME/.config/ytsub`.

You can change the `config.toml` path with the `-c` flag and the `instances`
file path with the `-s` flag.

If no `config.toml` is found, `ytsub` starts with built-in defaults.

See the [configuration documentation](docs/configuration.md) for details.
