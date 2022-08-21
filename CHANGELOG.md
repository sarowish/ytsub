# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Added
- `D` key binding deletes the selected video from database.
- Add import and export subcommands.
- Classify some messages as warning.
- Skip recently refreshed channels.

### Changed
- Use rss if the number of channels exceeds 125.

### Fixed
- Mark new videos correctly.

### Changed
- Ignore case when sorting channels list

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
