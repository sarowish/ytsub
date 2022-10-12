mod app;
mod channel;
mod cli;
mod commands;
mod config;
mod database;
mod help;
mod import;
mod input;
mod invidious;
mod message;
mod search;
mod ui;
mod utils;

use crate::config::keys::KeyBindings;
use crate::config::options::Options;
use crate::config::theme::Theme;
use crate::config::Config;
use anyhow::Result;
use app::App;
use channel::RefreshState;
use clap::ArgMatches;
use crossterm::event;
use crossterm::event::Event;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures_util::StreamExt;
use help::Help;
use input::InputMode;
use std::io;
use std::panic;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::Instant;
use tui::backend::{Backend, CrosstermBackend};
use tui::Terminal;
use ui::draw;

lazy_static::lazy_static! {
    static ref CLAP_ARGS: ArgMatches = cli::get_matches();
    static ref CONFIG: Config = match Config::new() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{:?}", e);
            std::process::exit(1);
        }
    };
    static ref OPTIONS: &'static Options = &CONFIG.options;
    static ref THEME: &'static Theme = &CONFIG.theme;
    static ref KEY_BINDINGS: &'static KeyBindings = &CONFIG.key_bindings;
    static ref HELP: Help<'static> = Help::new();
}

fn main() -> Result<()> {
    if CLAP_ARGS.is_present("gen_instances_list") {
        utils::generate_instances_file()?;
        return Ok(());
    }

    let (sync_io_tx, sync_io_rx) = std::sync::mpsc::channel();

    let app = Arc::new(Mutex::new(App::new(sync_io_tx)?));

    let cloned_app = app.clone();
    std::thread::spawn(move || -> Result<()> {
        async_io_loop(sync_io_rx, cloned_app)?;
        Ok(())
    });

    match CLAP_ARGS.subcommand() {
        Some(("import", matches)) => app.lock().unwrap().select_channels_to_import(
            PathBuf::from(matches.value_of("source").unwrap()),
            matches.value_of("format").unwrap_or_default().into(),
        )?,
        Some(("export", matches)) => {
            return app.lock().unwrap().export_subscriptions(
                PathBuf::from(matches.value_of("target").unwrap()),
                matches.value_of("format").unwrap_or_default().into(),
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

    let res = run_tui(&mut terminal, app);

    reset_terminal()?;

    if let Err(e) = res {
        eprintln!("{:?}", e);
    }

    Ok(())
}

fn run_tui<B: Backend>(terminal: &mut Terminal<B>, app: Arc<Mutex<App>>) -> Result<()> {
    let tick_rate = Duration::from_millis(OPTIONS.tick_rate);

    let mut last_tick = Instant::now();
    loop {
        terminal.draw(|f| draw(f, &mut app.lock().unwrap()))?;

        const SEARCH_MODE_CURSOR_OFFSET: u16 = 1;
        const SUBSCRIBE_MODE_CURSOR_OFFSET: u16 = 25;
        const TAG_CREATION_MODE_CURSOR_OFFSET: u16 = 10;

        let cursor_position = app.lock().unwrap().cursor_position;
        match &app.lock().unwrap().input_mode {
            mode @ (InputMode::Subscribe
            | InputMode::Search
            | InputMode::TagCreation
            | InputMode::TagRenaming) => {
                let offset = match mode {
                    InputMode::Search => SEARCH_MODE_CURSOR_OFFSET,
                    InputMode::Subscribe => SUBSCRIBE_MODE_CURSOR_OFFSET,
                    InputMode::TagCreation | InputMode::TagRenaming => {
                        TAG_CREATION_MODE_CURSOR_OFFSET
                    }
                    _ => 0,
                };
                terminal.set_cursor(cursor_position + offset, terminal.size()?.height - 1)?;
                terminal.show_cursor()?;
            }
            _ => terminal.hide_cursor()?,
        }

        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if input::handle_event(key, &mut app.lock().unwrap()) {
                    break;
                }
            }
        }
        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }

    Ok(())
}

fn reset_terminal() -> Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;

    Ok(())
}

pub enum IoEvent {
    SubscribeToChannel(String),
    SubscribeToChannels,
    RefreshChannel(String),
    RefreshChannels(bool),
    ClearMessage(u64),
}

#[tokio::main]
async fn async_io_loop(
    io_rx: std::sync::mpsc::Receiver<IoEvent>,
    app: Arc<Mutex<App>>,
) -> Result<()> {
    while let Ok(io_event) = io_rx.recv() {
        match io_event {
            IoEvent::SubscribeToChannel(channel_id) => subscribe_to_channel(&app, channel_id).await,
            IoEvent::SubscribeToChannels => subscribe_to_channels(&app).await?,
            IoEvent::RefreshChannel(channel_id) => refresh_channel(&app, channel_id).await,
            IoEvent::RefreshChannels(refresh_failed) => {
                refresh_channels(&app, refresh_failed).await?
            }
            IoEvent::ClearMessage(duration_seconds) => clear_message(&app, duration_seconds).await,
        }
    }
    Ok(())
}

async fn clear_message(app: &Arc<Mutex<App>>, duration_seconds: u64) {
    let app = app.clone();
    let cloned_token = app.lock().unwrap().message.clone_token();
    tokio::task::spawn(async move {
        tokio::select! {
            _ = cloned_token.cancelled() => {}
            _ = tokio::time::sleep(std::time::Duration::from_secs(duration_seconds)) => {
                app.lock().unwrap().message.clear_message();
            }
        }
    });
}

async fn subscribe_to_channel(app: &Arc<Mutex<App>>, channel_id: String) {
    if app
        .lock()
        .unwrap()
        .channels
        .get_mut_by_id(&channel_id)
        .is_some()
    {
        app.lock()
            .unwrap()
            .set_warning_message("Already subscribed to the channel");
        return;
    }

    let instance = app.lock().unwrap().instance();
    app.lock().unwrap().set_message("Subscribing to channel");
    let app = app.clone();
    tokio::task::spawn(async move {
        let channel_feed = instance.get_videos_of_channel(&channel_id);
        match channel_feed {
            Ok(channel_feed) => {
                app.lock().unwrap().message.clear_message();
                app.lock().unwrap().add_channel(channel_feed);
            }
            Err(e) => {
                app.lock()
                    .unwrap()
                    .set_error_message(&format!("Failed to subscribe: {:?}", e));
            }
        }
    });
}

async fn subscribe_to_channels(app: &Arc<Mutex<App>>) -> Result<()> {
    let mut channel_ids = Vec::new();

    for channel in &mut app.lock().unwrap().import_state.items {
        channel_ids.push(channel.channel_id.clone());
        channel.sub_state = RefreshState::ToBeRefreshed;
    }

    let now = std::time::Instant::now();

    let count = Arc::new(Mutex::new(0));
    let total = channel_ids.len();
    app.lock().unwrap().set_message(&format!(
        "Subscribing to channels: {}/{}",
        count.lock().unwrap(),
        total
    ));
    let instance = app.lock().unwrap().instance();
    let streams = futures_util::stream::iter(channel_ids).map(|channel_id| {
        let instance = instance.clone();
        app.lock()
            .unwrap()
            .import_state
            .get_mut_by_id(&channel_id)
            .unwrap()
            .sub_state = RefreshState::Refreshing;
        let app = app.clone();
        let count = count.clone();
        tokio::task::spawn(async move {
            let channel_feed = if total > 125 {
                instance.get_rss_feed_of_channel(&channel_id)
            } else {
                instance.get_videos_of_channel(&channel_id)
            };
            match channel_feed {
                Ok(channel_feed) => {
                    app.lock().unwrap().add_channel(channel_feed);
                    {
                        let mut app = app.lock().unwrap();
                        let idx = app.import_state.find_by_id(&channel_id).unwrap();
                        app.import_state.items[idx].sub_state = RefreshState::Completed;
                        app.import_state.items.remove(idx);
                    }
                    *count.lock().unwrap() += 1;
                    app.lock().unwrap().set_message(&format!(
                        "Subscribing to channels: {}/{}",
                        count.lock().unwrap(),
                        total
                    ));
                }
                Err(_) => {
                    app.lock()
                        .unwrap()
                        .import_state
                        .get_mut_by_id(&channel_id)
                        .unwrap()
                        .sub_state = RefreshState::Failed
                }
            }
        })
    });
    let mut buffered = streams.buffer_unordered(num_cpus::get());
    while buffered.next().await.is_some() {}

    let elapsed = now.elapsed();
    app.lock()
        .unwrap()
        .set_message_with_default_duration(&format!(
            "Subscribed to {} out of {} channels in {:?}",
            count.lock().unwrap(),
            total,
            elapsed
        ));

    if *count.lock().unwrap() == total {
        app.lock().unwrap().input_mode = InputMode::Normal;
    } else {
        let mut app = app.lock().unwrap();

        let list_len = app.import_state.items.len();
        if let Some(idx) = app.import_state.state.selected() {
            if list_len < idx {
                app.import_state.state.select(Some(list_len - 1));
            }
        } else {
            app.import_state.state.select(Some(0));
        }

        if let Err(e) = app.change_instance() {
            app.set_error_message(&format!("Couldn't change instance: {}", e));
        }

        for channel in &mut app.import_state.items {
            channel.sub_state = RefreshState::Completed;
        }
    }

    Ok(())
}
async fn refresh_channel(app: &Arc<Mutex<App>>, channel_id: String) {
    let now = std::time::Instant::now();
    let instance = app.lock().unwrap().instance();
    app.lock()
        .unwrap()
        .channels
        .get_mut_by_id(&channel_id)
        .unwrap()
        .refresh_state = RefreshState::Refreshing;
    app.lock().unwrap().set_message("Refreshing channel");
    let app = app.clone();
    tokio::task::spawn(async move {
        let channel_feed = match instance.get_latest_videos_of_channel(&channel_id) {
            Ok(channel_feed) => channel_feed,
            Err(_) => {
                app.lock()
                    .unwrap()
                    .channels
                    .get_mut_by_id(&channel_id)
                    .unwrap()
                    .refresh_state = RefreshState::Failed;
                app.lock()
                    .unwrap()
                    .set_error_message("failed to refresh channel");
                return;
            }
        };
        {
            let mut app = app.lock().unwrap();
            app.add_videos(channel_feed);
            app.channels
                .get_mut_by_id(&channel_id)
                .unwrap()
                .on_refresh_completed();
            let elapsed = now.elapsed();
            app.set_message_with_default_duration(&format!("Refreshed in {:?}", elapsed));
        }
    });
}

async fn refresh_channels(app: &Arc<Mutex<App>>, refresh_failed: bool) -> Result<()> {
    let now = std::time::Instant::now();

    let mut channel_ids = Vec::new();
    for channel in &mut app.lock().unwrap().channels.items {
        if refresh_failed && !matches!(channel.refresh_state, RefreshState::Failed)
            || matches!(channel.last_refreshed,
                        Some(time) if time.elapsed() < Duration::from_secs(600))
        {
            continue;
        }
        channel.set_to_be_refreshed();
        channel_ids.push(channel.channel_id.clone());
    }

    if channel_ids.is_empty() {
        {
            let mut app = app.lock().unwrap();
            if !app.channels.items.is_empty() {
                app.set_warning_message(if refresh_failed {
                    "There are no channels to retry refreshing"
                } else {
                    "All the channels have been recently refreshed"
                });
            }
        }
        return Ok(());
    }

    let count = Arc::new(Mutex::new(0));
    let total = channel_ids.len();
    app.lock().unwrap().set_message(&format!(
        "Refreshing Channels: {}/{}",
        count.lock().unwrap(),
        total
    ));
    let instance = app.lock().unwrap().instance();
    let streams = futures_util::stream::iter(channel_ids).map(|channel_id| {
        let instance = instance.clone();
        app.lock()
            .unwrap()
            .channels
            .get_mut_by_id(&channel_id)
            .unwrap()
            .refresh_state = RefreshState::Refreshing;
        let app = app.clone();
        let count = count.clone();
        tokio::task::spawn(async move {
            let channel_feed = if total > 125 {
                instance.get_rss_feed_of_channel(&channel_id)
            } else {
                instance.get_latest_videos_of_channel(&channel_id)
            };
            match channel_feed {
                Ok(channel_feed) => {
                    let mut app = app.lock().unwrap();
                    app.add_videos(channel_feed);
                    app.channels
                        .get_mut_by_id(&channel_id)
                        .unwrap()
                        .on_refresh_completed();
                    *count.lock().unwrap() += 1;
                    app.set_message(&format!(
                        "Refreshing Channels: {}/{}",
                        count.lock().unwrap(),
                        total
                    ));
                }
                Err(_) => {
                    app.lock()
                        .unwrap()
                        .channels
                        .get_mut_by_id(&channel_id)
                        .unwrap()
                        .refresh_state = RefreshState::Failed
                }
            }
        })
    });
    let mut buffered = streams.buffer_unordered(num_cpus::get());
    while buffered.next().await.is_some() {}
    let elapsed = now.elapsed();
    match *count.lock().unwrap() {
        0 => app
            .lock()
            .unwrap()
            .set_error_message("Failed to refresh channels"),
        count => app
            .lock()
            .unwrap()
            .set_message_with_default_duration(&format!(
                "Refreshed {} out of {} channels in {:?}",
                count, total, elapsed
            )),
    }
    Ok(())
}
