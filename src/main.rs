mod api;
mod app;
mod channel;
mod cli;
mod commands;
mod config;
mod database;
mod help;
mod import;
mod input;
mod message;
mod search;
mod stream_formats;
mod ui;
mod utils;

use crate::config::keys::KeyBindings;
use crate::config::options::Options;
use crate::config::theme::Theme;
use crate::config::Config;
use anyhow::Result;
use api::ApiBackend;
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
use parking_lot::Mutex;
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::Terminal;
use std::io;
use std::panic;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use ui::draw;

lazy_static::lazy_static! {
    static ref CLAP_ARGS: ArgMatches = cli::get_matches();
    static ref CONFIG: Config = match Config::new() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{e:?}");
            std::process::exit(1);
        }
    };
    static ref OPTIONS: &'static Options = &CONFIG.options;
    static ref THEME: &'static Theme = &CONFIG.theme;
    static ref KEY_BINDINGS: &'static KeyBindings = &CONFIG.key_bindings;
    static ref HELP: Help<'static> = Help::new();
}

fn main() -> Result<()> {
    if CLAP_ARGS.get_flag("gen_instances_list") {
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
        Some(("import", matches)) => app.lock().select_channels_to_import(
            PathBuf::from(matches.get_one::<String>("source").unwrap()),
            matches
                .get_one::<String>("format")
                .map(String::as_str)
                .unwrap()
                .into(),
        )?,
        Some(("export", matches)) => {
            return app.lock().export_subscriptions(
                PathBuf::from(matches.get_one::<String>("target").unwrap()),
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

    let res = run_tui(&mut terminal, app);

    reset_terminal()?;

    if let Err(e) = res {
        eprintln!("{e:?}");
    }

    Ok(())
}

fn run_tui<B: Backend>(terminal: &mut Terminal<B>, app: Arc<Mutex<App>>) -> Result<()> {
    let tick_rate = Duration::from_millis(OPTIONS.tick_rate);

    let mut last_tick = Instant::now();
    loop {
        terminal.draw(|f| draw(f, &mut app.lock()))?;

        const SEARCH_MODE_CURSOR_OFFSET: u16 = 1;
        const SUBSCRIBE_MODE_CURSOR_OFFSET: u16 = 25;
        const TAG_CREATION_MODE_CURSOR_OFFSET: u16 = 10;

        let cursor_position = app.lock().cursor_position;
        match &app.lock().input_mode {
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
                terminal
                    .set_cursor_position((cursor_position + offset, terminal.size()?.height - 1))?;
                terminal.show_cursor()?;
            }
            _ => terminal.hide_cursor()?,
        }

        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if input::handle_event(key, &mut app.lock()) {
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
    FetchInstances,
    ClearMessage(u64),
}

#[tokio::main]
async fn async_io_loop(
    io_rx: std::sync::mpsc::Receiver<IoEvent>,
    app: Arc<Mutex<App>>,
) -> Result<()> {
    while let Ok(io_event) = io_rx.recv() {
        match io_event {
            IoEvent::SubscribeToChannel(channel_id) => subscribe_to_channel(&app, &channel_id),
            IoEvent::SubscribeToChannels => subscribe_to_channels(&app).await?,
            IoEvent::RefreshChannel(channel_id) => refresh_channel(&app, channel_id),
            IoEvent::RefreshChannels(refresh_failed) => {
                refresh_channels(&app, refresh_failed).await?;
            }
            IoEvent::FetchInstances => {
                app.lock().set_message("Fetching instances");

                if let Ok(instances) = utils::fetch_invidious_instances() {
                    app.lock().invidious_instances = Some(instances);
                    app.lock().message.clear_message();
                    app.lock().set_instance();
                } else {
                    app.lock().selected_api = ApiBackend::Local;
                    app.lock()
                        .set_error_message("Failed to fetch instances. Falling back to Local API.");
                }
            }
            IoEvent::ClearMessage(duration_seconds) => clear_message(&app, duration_seconds),
        }
    }
    Ok(())
}

fn clear_message(app: &Arc<Mutex<App>>, duration_seconds: u64) {
    let app = app.clone();
    let cloned_token = app.lock().message.clone_token();
    tokio::task::spawn(async move {
        tokio::select! {
            () = cloned_token.cancelled() => {}
            () = tokio::time::sleep(std::time::Duration::from_secs(duration_seconds)) => {
                app.lock().message.clear_message();
            }
        }
    });
}

fn subscribe_to_channel(app: &Arc<Mutex<App>>, input: &str) {
    let channel_id = app.lock().instance().resolve_channel_id(input).unwrap();

    if app.lock().channels.get_mut_by_id(&channel_id).is_some() {
        app.lock()
            .set_warning_message("Already subscribed to the channel");
        return;
    }

    let mut instance = app.lock().instance();
    app.lock().set_message("Subscribing to channel");
    let app = app.clone();
    tokio::task::spawn(async move {
        let channel_feed = instance.get_videos_for_the_first_time(&channel_id);

        match channel_feed {
            Ok(channel_feed) => {
                app.lock().message.clear_message();
                app.lock().add_channel(channel_feed);
            }
            Err(e) => {
                app.lock()
                    .set_error_message(&format!("Failed to subscribe: {e:?}"));
            }
        }
    });
}

async fn subscribe_to_channels(app: &Arc<Mutex<App>>) -> Result<()> {
    let mut channel_ids = Vec::new();

    for channel in &mut app.lock().import_state.items {
        channel_ids.push(channel.channel_id.clone());
        channel.sub_state = RefreshState::ToBeRefreshed;
    }

    let now = std::time::Instant::now();

    let count = Arc::new(Mutex::new(0));
    let total = channel_ids.len();
    app.lock().set_message(&format!(
        "Subscribing to channels: {}/{}",
        count.lock(),
        total
    ));
    let instance = app.lock().instance();
    let streams = futures_util::stream::iter(channel_ids).map(|channel_id| {
        let mut instance = dyn_clone::clone_box(&*instance);
        app.lock()
            .import_state
            .get_mut_by_id(&channel_id)
            .unwrap()
            .sub_state = RefreshState::Refreshing;
        let app = app.clone();
        let count = count.clone();
        tokio::task::spawn(async move {
            let channel_feed = if total > OPTIONS.rss_threshold {
                instance.get_rss_feed_of_channel(&channel_id)
            } else {
                instance.get_videos_for_the_first_time(&channel_id)
            };

            match channel_feed {
                Ok(channel_feed) => {
                    app.lock().add_channel(channel_feed);
                    {
                        let mut app = app.lock();
                        let idx = app.import_state.find_by_id(&channel_id).unwrap();
                        app.import_state.items[idx].sub_state = RefreshState::Completed;
                        app.import_state.items.remove(idx);
                    }
                    *count.lock() += 1;
                    app.lock().set_message(&format!(
                        "Subscribing to channels: {}/{}",
                        count.lock(),
                        total
                    ));
                }
                Err(_) => {
                    app.lock()
                        .import_state
                        .get_mut_by_id(&channel_id)
                        .unwrap()
                        .sub_state = RefreshState::Failed;
                }
            }
        })
    });
    let mut buffered = streams.buffer_unordered(num_cpus::get());
    while buffered.next().await.is_some() {}

    let elapsed = now.elapsed().as_secs_f64();
    app.lock().set_message_with_default_duration(&format!(
        "Subscribed to {} out of {} channels in {:.2}s",
        count.lock(),
        total,
        elapsed
    ));

    if *count.lock() == total {
        app.lock().input_mode = InputMode::Normal;
    } else {
        let mut app = app.lock();

        let list_len = app.import_state.items.len();
        if let Some(idx) = app.import_state.state.selected() {
            if list_len < idx {
                app.import_state.state.select(Some(list_len - 1));
            }
        } else {
            app.import_state.state.select(Some(0));
        }

        if let ApiBackend::Invidious = app.selected_api {
            app.set_instance();
        }

        for channel in &mut app.import_state.items {
            channel.sub_state = RefreshState::Completed;
        }
    }

    Ok(())
}
fn refresh_channel(app: &Arc<Mutex<App>>, channel_id: String) {
    let now = std::time::Instant::now();
    let mut instance = app.lock().instance();
    app.lock()
        .set_channel_refresh_state(&channel_id, RefreshState::Refreshing);
    app.lock().set_message("Refreshing channel");
    let app = app.clone();
    tokio::task::spawn(async move {
        let Ok(channel_feed) = instance.get_videos_of_channel(&channel_id) else {
            app.lock()
                .set_channel_refresh_state(&channel_id, RefreshState::Failed);
            app.lock().set_error_message("failed to refresh channel");
            return;
        };
        {
            let mut app = app.lock();
            app.add_videos(channel_feed);
            app.set_channel_refresh_state(&channel_id, RefreshState::Completed);
            let elapsed = now.elapsed().as_secs_f64();
            app.set_message_with_default_duration(&format!("Refreshed in {elapsed:.2}s"));
        }
    });
}

async fn refresh_channels(app: &Arc<Mutex<App>>, refresh_failed: bool) -> Result<()> {
    let now = std::time::Instant::now();

    let mut channel_ids = Vec::new();
    for channel in &mut app.lock().channels.items {
        if refresh_failed && !matches!(channel.refresh_state, RefreshState::Failed)
            || !refresh_failed
                && matches!(
                    channel.last_refreshed,
                    Some(time) if utils::time_passed(time)? < OPTIONS.refresh_threshold
                )
        {
            continue;
        }
        channel.set_to_be_refreshed();
        channel_ids.push(channel.channel_id.clone());
    }

    if channel_ids.is_empty() {
        {
            let mut app = app.lock();
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
    app.lock()
        .set_message(&format!("Refreshing Channels: {}/{}", count.lock(), total));
    let instance = app.lock().instance();
    let streams = futures_util::stream::iter(channel_ids).map(|channel_id| {
        let mut instance = dyn_clone::clone_box(&*instance);
        app.lock()
            .set_channel_refresh_state(&channel_id, RefreshState::Refreshing);
        let app = app.clone();
        let count = count.clone();
        tokio::task::spawn(async move {
            let channel_feed = if total > OPTIONS.rss_threshold {
                instance.get_rss_feed_of_channel(&channel_id)
            } else {
                instance.get_videos_of_channel(&channel_id)
            };

            match channel_feed {
                Ok(channel_feed) => {
                    let mut app = app.lock();
                    app.add_videos(channel_feed);
                    app.set_channel_refresh_state(&channel_id, RefreshState::Completed);
                    *count.lock() += 1;
                    app.set_message(&format!("Refreshing Channels: {}/{}", count.lock(), total));
                }
                Err(_) => {
                    app.lock()
                        .set_channel_refresh_state(&channel_id, RefreshState::Failed);
                }
            }
        })
    });
    let mut buffered = streams.buffer_unordered(num_cpus::get());
    while buffered.next().await.is_some() {}
    let elapsed = now.elapsed().as_secs_f64();
    match *count.lock() {
        0 => app.lock().set_error_message("Failed to refresh channels"),
        count => app.lock().set_message_with_default_duration(&format!(
            "Refreshed {count} out of {total} channels in {elapsed:.2}s"
        )),
    }
    Ok(())
}
