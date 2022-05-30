# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Added
- Make theme, key bindings and options configurable through a configuration file.
- `ctrl+h` opens a help window that shows the key bindings.

### Changed
- `o` key binding opens the channel page in browser if the channels is the selected block.

### Fixed
- Reset the terminal properly when the app panics or an error is propagated

## [0.1.1] - 2022-05-13
### Fixed
- Enable foreign key constraints in case they are disabled by default. [`029dc0c`](https://github.com/sarowish/ytsub/commit/029dc0c)
