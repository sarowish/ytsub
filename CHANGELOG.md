# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Fixed
- Handle short forms in published texts.
- Take help text into account while determining floating window height.
- Get channel name from a different field.

## [0.4.0] - 2023-02-06
### Added
- Fetch instances from api.invidious.io after starting the tui.
[`b6236d`](https://github.com/sarowish/ytsub/commit/b6236d)
- Hide barely visable columns.
[`0637d7`](https://github.com/sarowish/ytsub/commit/0637d7)
- `O` key binding opens the channel or video Youtube page in browser.
[`4ae403`](https://github.com/sarowish/ytsub/commit/4ae403)
- Add Innertube API as an alternative.
[`c53db8`](https://github.com/sarowish/ytsub/commit/c53db8)
- Make channel refresh threshold and RSS threshold configurable.
[`db3e1c`](https://github.com/sarowish/ytsub/commit/db3e1c)

### Changed
- Change `modify channels` help text to `pick channels`
[`f7d456`](https://github.com/sarowish/ytsub/commit/f7d456)

### Fixed
- No longer crashes if no tag is selected when trying to modify channels of a tag.
[`3b6276`](https://github.com/sarowish/ytsub/commit/3b6276)
- Reload channels when a tag is deleted.
[`4a63ae`](https://github.com/sarowish/ytsub/commit/4a63ae)
- Don't automatically create config directory unless generating instances file.
[`6ce6ec`](https://github.com/sarowish/ytsub/commit/6ce6ec)
- Handle changes in Invidious API.
[`d9ef23`](https://github.com/sarowish/ytsub/commit/d9ef23)
- Ignore `refresh_threshold` when refreshing failed channels.
[`bc6bbe`](https://github.com/sarowish/ytsub/commit/bc6bbe)

## [0.3.1] - 2022-10-16
### Fixed
- After a channel is successfully refreshed, set its `last_refreshed` field.
[`f6a056a`](https://github.com/sarowish/ytsub/commit/f6a056a)

## [0.3.0] - 2022-10-16
### Added
- `D` key binding deletes the selected video from database.
[`83238bc`](https://github.com/sarowish/ytsub/commit/83238bc)
- Add import and export subcommands.
[`4f3e073`](https://github.com/sarowish/ytsub/commit/4f3e073)
- Classify some messages as warning.
[`9d88336`](https://github.com/sarowish/ytsub/commit/9d88336)
- Skip recently refreshed channels.
[`89f8528`](https://github.com/sarowish/ytsub/commit/89f8528)
- Group channels using tags.
[`3e57063`](https://github.com/sarowish/ytsub/commit/3e57063)
- Show warning if trying to subscribe to an already subscribed channel.
[`ea83acc`](https://github.com/sarowish/ytsub/commit/ea83acc)

### Changed
- Ignore case when sorting channels list
[`4e08381`](https://github.com/sarowish/ytsub/commit/4e08381)
- Use rss if the number of channels exceeds 125.
[`0bbca1d`](https://github.com/sarowish/ytsub/commit/0bbca1d)

### Fixed
- Mark new videos correctly.
[`d91ce4e`](https://github.com/sarowish/ytsub/commit/d91ce4e)

### Deprecated
- Deprecate `--tick-rate`, `--request-timeout` and `--highlight-symbol` cli arguments.
[`1fd5678`](https://github.com/sarowish/ytsub/commit/1fd5678)

## [0.2.0] - 2022-05-31
### Added
- Make theme, key bindings and options configurable through a configuration file.
[`8688fb5`](https://github.com/sarowish/ytsub/commit/8688fb5)
- `ctrl+h` opens a help window that shows the key bindings.
[`c418969`](https://github.com/sarowish/ytsub/commit/c418969)

### Changed
- `o` key binding opens the channel page in browser if the channels is the selected block.
[`2dc1e7c`](https://github.com/sarowish/ytsub/commit/2dc1e7c)

### Fixed
- Reset the terminal properly when the app panics or an error is propagated
[`041aa75`](https://github.com/sarowish/ytsub/commit/041aa75)
- Don't show an incorrect error message after aborting from search and trying to repeat the latest search.
[`7e171cd`](https://github.com/sarowish/ytsub/commit/7e171cd)

## [0.1.1] - 2022-05-13
### Fixed
- Enable foreign key constraints in case they are disabled by default.
[`029dc0c`](https://github.com/sarowish/ytsub/commit/029dc0c)
