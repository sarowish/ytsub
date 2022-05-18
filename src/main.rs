mod app;
mod channel;
mod cli;
mod commands;
mod config;
mod database;
mod input;
mod message;
mod search;
mod ui;
mod utils;

use crate::commands::Command;
use crate::config::keys::KeyBindings;
use crate::config::options::Options;
use crate::config::theme::Theme;
use crate::config::Config;
use anyhow::Result;
use app::App;
use channel::RefreshState;
use clap::Parser;
use cli::Args;
use crossterm::event;
use crossterm::event::Event;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures_util::StreamExt;
use input::InputMode;
use std::io::stdout;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::Instant;
use tui::backend::CrosstermBackend;
use tui::Terminal;
use ui::draw;

lazy_static::lazy_static! {
    static ref CLAP_ARGS: Args = Args::parse();
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
}

fn main() -> Result<()> {
    if CLAP_ARGS.gen_instance_list {
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

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    let tick_rate = Duration::from_millis(OPTIONS.tick_rate);

    let mut last_tick = Instant::now();
    loop {
        terminal.draw(|f| draw(f, &mut app.lock().unwrap()))?;

        const SEARCH_MODE_CURSOR_OFFSET: u16 = 1;
        const SUBSCRIBE_MODE_CURSOR_OFFSET: u16 = 25;

        let cursor_position = app.lock().unwrap().cursor_position;
        match &app.lock().unwrap().input_mode {
            mode @ InputMode::Subscribe | mode @ InputMode::Search => {
                let offset = match mode {
                    InputMode::Search => SEARCH_MODE_CURSOR_OFFSET,
                    InputMode::Subscribe => SUBSCRIBE_MODE_CURSOR_OFFSET,
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
                let input_mode = app.lock().unwrap().input_mode.clone();
                match input_mode {
                    InputMode::Normal => match KEY_BINDINGS.get(&key) {
                        Some(Command::Quit) => break,
                        _ => {
                            input::handle_key_normal_mode(key, &mut app.lock().unwrap());
                        }
                    },
                    InputMode::Confirmation => {
                        input::handle_key_confirmation_mode(key, &mut app.lock().unwrap())
                    }
                    _ => input::handle_key_editing_mode(key, &mut app.lock().unwrap()),
                }
            }
        }
        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

pub enum IoEvent {
    SubscribeToChannel(String),
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
    let instance = app.lock().unwrap().instance();
    app.lock().unwrap().set_message("Subscribing to channel");
    let app = app.clone();
    tokio::task::spawn(async move {
        let videos_json = instance.get_videos_of_channel(&channel_id);
        match videos_json {
            Ok(videos_json) => {
                app.lock().unwrap().message.clear_message();
                app.lock().unwrap().add_channel(videos_json);
            }
            Err(e) => {
                app.lock()
                    .unwrap()
                    .set_error_message(&format!("Failed to subscribe: {:?}", e));
            }
        }
    });
}

async fn refresh_channel(app: &Arc<Mutex<App>>, channel_id: String) {
    let now = std::time::Instant::now();
    let instance = app.lock().unwrap().instance();
    app.lock().unwrap().start_refreshing_channel(&channel_id);
    app.lock().unwrap().set_message("Refreshing channel");
    let app = app.clone();
    tokio::task::spawn(async move {
        let videos_json = match instance.get_latest_videos_of_channel(&channel_id) {
            Ok(videos) => videos,
            Err(_) => {
                app.lock().unwrap().refresh_failed(&channel_id);
                app.lock()
                    .unwrap()
                    .set_error_message("failed to refresh channel");
                return;
            }
        };
        app.lock().unwrap().add_videos(videos_json, &channel_id);
        app.lock().unwrap().complete_refreshing_channel(&channel_id);
        let elapsed = now.elapsed();
        app.lock()
            .unwrap()
            .set_message_with_default_duration(&format!("Refreshed in {:?}", elapsed));
    });
}

async fn refresh_channels(app: &Arc<Mutex<App>>, refresh_failed: bool) -> Result<()> {
    let now = std::time::Instant::now();

    let mut channel_ids = Vec::new();
    for channel in &mut app.lock().unwrap().channels.items {
        if refresh_failed && !matches!(channel.refresh_state, RefreshState::Failed) {
            continue;
        }
        channel.set_to_be_refreshed();
        channel_ids.push(channel.channel_id.clone());
    }

    if channel_ids.is_empty() {
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
        app.lock().unwrap().start_refreshing_channel(&channel_id);
        let app = app.clone();
        let count = count.clone();
        tokio::task::spawn(async move {
            let videos_json = instance.get_latest_videos_of_channel(&channel_id);
            match videos_json {
                Ok(videos_json) => {
                    app.lock().unwrap().add_videos(videos_json, &channel_id);
                    app.lock().unwrap().complete_refreshing_channel(&channel_id);
                    *count.lock().unwrap() += 1;
                    app.lock().unwrap().set_message(&format!(
                        "Refreshing Channels: {}/{}",
                        count.lock().unwrap(),
                        total
                    ));
                }
                Err(_) => app.lock().unwrap().refresh_failed(&channel_id),
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
