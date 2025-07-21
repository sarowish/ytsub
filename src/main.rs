mod api;
mod app;
mod channel;
mod cli;
mod client;
mod commands;
mod config;
mod database;
mod help;
mod import;
mod input;
mod message;
mod player;
mod ro_cell;
mod search;
mod stream_formats;
mod ui;
mod utils;

use crate::config::Config;
use crate::config::keys::KeyBindings;
use crate::config::options::Options;
use crate::config::theme::Theme;
use anyhow::Result;
use api::ApiBackend;
use app::App;
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
use ratatui::DefaultTerminal;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io;
use std::panic;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Duration;
use std::time::Instant;
use stream_formats::Formats;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_util::sync::CancellationToken;
use ui::draw;

static CLAP_ARGS: LazyLock<ArgMatches> = LazyLock::new(cli::get_matches);
static CONFIG: LazyLock<Config> = LazyLock::new(|| match Config::new() {
    Ok(config) => config,
    Err(e) => {
        eprintln!("{e:?}");
        std::process::exit(1);
    }
});
static OPTIONS: LazyLock<&Options> = LazyLock::new(|| &CONFIG.options);
static KEY_BINDINGS: LazyLock<&KeyBindings> = LazyLock::new(|| &CONFIG.key_bindings);
static THEME: LazyLock<&Theme> = LazyLock::new(|| &CONFIG.theme);
static HELP: LazyLock<Help> = LazyLock::new(Help::new);

#[tokio::main]
async fn main() -> Result<()> {
    if CLAP_ARGS.get_flag("gen_instances_list") {
        utils::generate_instances_file().await?;
        return Ok(());
    }

    let (io_tx, io_rx) = mpsc::unbounded_channel();

    let mut app = App::new(io_tx)?;

    match CLAP_ARGS.subcommand() {
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

fn render(app: &mut App, terminal: &mut DefaultTerminal) -> Result<()> {
    terminal.draw(|f| draw(f, app))?;

    const SEARCH_MODE_CURSOR_OFFSET: u16 = 1;
    const SUBSCRIBE_MODE_CURSOR_OFFSET: u16 = 25;
    const TAG_CREATION_MODE_CURSOR_OFFSET: u16 = 10;

    let cursor_position = app.cursor_position;
    match &app.input_mode {
        mode @ (InputMode::Subscribe
        | InputMode::Search
        | InputMode::TagCreation
        | InputMode::TagRenaming) => {
            let offset = match mode {
                InputMode::Search => SEARCH_MODE_CURSOR_OFFSET,
                InputMode::Subscribe => SUBSCRIBE_MODE_CURSOR_OFFSET,
                InputMode::TagCreation | InputMode::TagRenaming => TAG_CREATION_MODE_CURSOR_OFFSET,
                _ => 0,
            };
            terminal
                .set_cursor_position((cursor_position + offset, terminal.size()?.height - 1))?;
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
    terminal: &mut DefaultTerminal,
    rx: UnboundedReceiver<IoEvent>,
    mut app: App,
) -> Result<()> {
    let mut term_events = EventStream::new();

    let (req_tx, mut req_rx) = mpsc::unbounded_channel();
    TX.init(req_tx);

    let mut client = client::Client::new(rx).await?;
    tokio::spawn(async move { client.run().await });

    render(&mut app, terminal)?;

    let (mut timeout, mut last_render) = (None, Instant::now());

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

                timeout = Duration::from_millis(OPTIONS.tick_rate).checked_sub(last_render.elapsed());

                if timeout.is_none() {
                    render(&mut app, terminal)?;
                    last_render = Instant::now();
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

            if let RefreshState::Completed = state {
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
        ClientRequest::UpdateChannel(feed) => app.add_videos(feed),
        ClientRequest::EnterFormatSelection(formats) => {
            app.input_mode = InputMode::FormatSelection;
            app.stream_formats = *formats;
        }
        ClientRequest::MarkAsWatched(video_id) => app.set_watched(&video_id, true),
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

pub enum IoEvent {
    SubscribeToChannel(String),
    ImportChannels(Vec<String>),
    RefreshChannels(Vec<String>),
    FetchFormats(String, String, bool),
    PlayFromFormats(Box<Formats>),
    OpenInBrowser(String, ApiBackend),
    ClearMessage(CancellationToken, u64),
    SwitchApi,
}
