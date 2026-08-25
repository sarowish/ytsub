mod api;
mod app;
mod backend;
mod channel;
mod cli;
mod client;
mod clipboard;
mod commands;
mod config;
mod database;
mod emulator;
mod help;
mod http;
mod import;
mod input;
mod list;
mod message;
mod mpv;
mod player;
mod process;
mod progress;
mod protobuf;
mod ro_cell;
mod search;
mod stream_formats;
mod thumbnail;
mod ui;
mod utils;
mod video;

use crate::client::IoEvent;
use crate::config::Config;
use crate::config::keys::KeyBindings;
use crate::config::theme::Theme;
use crate::emulator::Emulator;
use crate::mpv::PlaybackPhase;
use anyhow::Result;
use app::App;
use backend::CrosstermBackend;
use channel::RefreshState;
use clap::ArgMatches;
use client::ClientRequest;
use client::TX;
use crossterm::event::Event;
use crossterm::event::EventStream;
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use futures_util::StreamExt;
use help::Help;
use input::InputMode;
use ratatui::Terminal;
use std::io;
use std::panic;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedReceiver;
use ui::draw;

static CLAP_ARGS: LazyLock<ArgMatches> = LazyLock::new(cli::get_matches);
static CONFIG: LazyLock<Config> = LazyLock::new(|| match Config::new() {
    Ok(config) => config,
    Err(e) => {
        eprintln!("{e:?}");
        std::process::exit(1);
    }
});
static KEY_BINDINGS: LazyLock<&KeyBindings> = LazyLock::new(|| &CONFIG.key_bindings);
static THEME: LazyLock<&Theme> = LazyLock::new(|| &CONFIG.theme);
static HELP: LazyLock<Help> = LazyLock::new(Help::new);

type AppTerminal = Terminal<CrosstermBackend<io::Stdout>>;

#[tokio::main]
async fn main() -> Result<()> {
    if CLAP_ARGS.get_flag("gen_instances_list") {
        utils::generate_instances_file().await?;
        return Ok(());
    }

    let subcommand = CLAP_ARGS.subcommand();

    if let Some(("database", database_matches)) = subcommand
        && let Some(("downgrade", downgrade_matches)) = database_matches.subcommand()
    {
        let target_version = downgrade_matches.get_one::<u8>("target").copied();

        match database::downgrade_database(&CONFIG.database, target_version)? {
            database::DowngradeOutcome::Downgraded {
                from,
                to,
                backup_path,
            } => {
                println!("Downgraded database schema from {from} to {to}.");
                println!("Backup: {}", backup_path.display());

                let removed_data = downgrade_removed_data(from, to);
                if !removed_data.is_empty() {
                    println!(
                        "Removed during downgrade: {}. The original data remains in the backup.",
                        removed_data.join(", ")
                    );
                }
            }
            database::DowngradeOutcome::AlreadyAtTarget { version } => {
                println!("Database is already at schema version {version}.");
            }
        }

        return Ok(());
    }

    let (io_tx, io_rx) = mpsc::unbounded_channel();

    let mut app = App::new(io_tx)?;

    match subcommand {
        Some(("import", matches)) => app.select_channels_to_import(
            matches.get_one::<PathBuf>("source").unwrap(),
            matches
                .get_one::<String>("format")
                .map(String::as_str)
                .unwrap()
                .into(),
        )?,
        Some(("export", matches)) => {
            return app.export_subscriptions(
                matches.get_one::<PathBuf>("target").unwrap(),
                matches
                    .get_one::<String>("format")
                    .map(String::as_str)
                    .unwrap()
                    .into(),
            );
        }
        _ => (),
    }

    let default_hook = panic::take_hook();

    panic::set_hook(Box::new(move |info| {
        reset_terminal().unwrap();
        default_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let res = run_tui(&mut terminal, io_rx, app).await;

    reset_terminal()?;

    if let Err(e) = res {
        eprintln!("{e:?}");
    }

    Ok(())
}

fn downgrade_removed_data(from: u8, to: u8) -> Vec<&'static str> {
    let mut removed = Vec::new();

    if from >= 4 && to < 4 {
        removed.push("saved playback positions");
    }
    if from >= 3 && to < 3 {
        removed.push("video tab and members-only state");
    }
    if from >= 2 && to < 2 {
        removed.push("channel refresh timestamps");
    }

    removed
}

fn render(app: &mut App, terminal: &mut AppTerminal) -> Result<()> {
    let prev_covered_area = app.thumbnail.as_ref().and_then(|t| t.covered_area);

    terminal.draw(|f| draw(f, app))?;

    if let Some(e) = &app.emulator
        && let Some(t) = &app.thumbnail
        && t.needs_rerender(prev_covered_area, e.graphics_protocol)
    {
        terminal.swap_buffers();
        terminal.draw(|f| draw(f, app))?;
    }

    let cursor_position = app.input.cursor_position();
    match &app.input_mode {
        InputMode::Subscribe
        | InputMode::Search
        | InputMode::TagCreation
        | InputMode::TagRenaming => {
            terminal.set_cursor_position((cursor_position, terminal.size()?.height - 1))?;
            terminal.show_cursor()?;
        }
        _ => terminal.hide_cursor()?,
    }

    Ok(())
}

async fn sleep_if_timeout(timeout: &mut Option<Duration>) -> bool {
    let Some(t) = timeout.take() else {
        return false;
    };

    tokio::time::sleep(t).await;
    true
}

async fn run_tui(
    terminal: &mut AppTerminal,
    rx: UnboundedReceiver<IoEvent>,
    mut app: App,
) -> Result<()> {
    let mut term_events = EventStream::new();

    let (req_tx, mut req_rx) = mpsc::unbounded_channel();
    TX.init(req_tx);

    let (player, mut playback_update, _player_task) = mpv::PlayerHandle::spawn();
    let mut client = client::Client::new(rx, player).await?;
    tokio::spawn(async move { client.run().await });

    if CONFIG.show_thumbnails {
        app.emulator = Emulator::new().await.ok();
        app.on_change_video();
    }

    render(&mut app, terminal)?;

    let (mut timeout, mut last_render) = (None, Instant::now());
    let mut playback_update_open = true;

    loop {
        tokio::select! {
            true = sleep_if_timeout(&mut timeout) => {
                render(&mut app, terminal)?;
                last_render = Instant::now();
            }
            Some(Ok(term_event)) = term_events.next() => {
                if let Event::Key(key) = term_event
                    && input::handle_event(key, &mut app)
                {
                    break;
                }

                render(&mut app, terminal)?;
                last_render = Instant::now();
            },
            Some(event) = req_rx.recv() => {
                handle_event(event, &mut app);

                timeout = Duration::from_millis(CONFIG.tick_rate).checked_sub(last_render.elapsed());

                if timeout.is_none() {
                    render(&mut app, terminal)?;
                    last_render = Instant::now();
                }
            }
            result = playback_update.recv(), if playback_update_open => {
                match result {
                    Some(update) => {
                        if let PlaybackPhase::Error(error) = &update.state.phase {
                            app.set_error_message(&format!("Playback failed: {error}"));
                        }

                        app.handle_playback_update(update);

                        timeout = None;
                        render(&mut app, terminal)?;
                        last_render = Instant::now();
                    }
                    None => {
                        playback_update_open = false;
                        app.set_error_message("Player stopped unexpectedly");
                        timeout = None;
                        render(&mut app, terminal)?;
                        last_render = Instant::now();
                    }
                }
            }
        }
    }

    Ok(())
}

fn reset_terminal() -> Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;

    Ok(())
}

fn handle_event(event: ClientRequest, app: &mut App) {
    match event {
        ClientRequest::SetRefreshState(id, state) => app.set_channel_refresh_state(&id, state),
        ClientRequest::SetImportState(id, state) => {
            let idx = app.import_state.find_by_id(&id).unwrap();
            app.import_state.items[idx].sub_state = state;

            if matches!(state, RefreshState::Completed) {
                app.import_state.items.remove(idx);
            }
        }
        ClientRequest::AddChannel(feed) => app.add_channel(feed),
        ClientRequest::CheckChannel(id, tx) => {
            tx.send(app.channels.get_mut_by_id(&id).is_some()).unwrap();
        }
        ClientRequest::FinalizeImport(imported_all) => {
            if imported_all {
                app.input_mode = InputMode::Normal;
            } else {
                for channel in &mut app.import_state.items {
                    channel.sub_state = RefreshState::Completed;
                }
            }
        }
        ClientRequest::UpdateChannel(feed) => app.add_tabs(feed),
        ClientRequest::UpdateTitle(video_id, title) => {
            if database::update_title(&app.conn, &video_id, &title).is_ok() {
                app.load_videos(true);
            }
        }
        ClientRequest::SetThumbnail(video_id, data) => {
            let is_current_video = app
                .get_current_video()
                .is_some_and(|video| video.video_id == video_id);

            if is_current_video {
                app.thumbnail = data;
            }
        }
        ClientRequest::EnterFormatSelection(formats) => {
            app.input_mode = InputMode::FormatSelection;
            app.stream_formats = *formats;
        }
        ClientRequest::SetWatched(video_id, is_watched) => {
            if CONFIG.auto_mark_watched {
                app.set_watched(&video_id, is_watched)
            }
        }
        ClientRequest::SetMessage(msg, message_type, duration) => {
            app.message.set_message(&msg);
            app.message.message_type = message_type;
            if let Some(duration) = duration {
                app.clear_message_after_duration(duration);
            }
        }
        ClientRequest::ClearMessage => app.message.clear_message(),
    }
}
