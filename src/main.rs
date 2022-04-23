mod app;
mod channel;
mod cli;
mod database;
mod input;
mod search;
mod ui;
mod utils;

use anyhow::{Context, Result};
use app::App;
use clap::Parser;
use cli::Options;
use crossterm::event;
use crossterm::event::{Event, KeyCode, KeyModifiers};
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

fn main() -> Result<()> {
    let options = Options::parse();
    if options.gen_instance_list {
        utils::generate_instances_file()?;
    }

    let (sync_io_tx, sync_io_rx) = std::sync::mpsc::channel();

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    let tick_rate = Duration::from_millis(options.tick_rate);

    let app = Arc::new(Mutex::new(App::new(options, sync_io_tx)?));

    let cloned_app = app.clone();
    std::thread::spawn(move || -> Result<()> {
        async_io_loop(sync_io_rx, cloned_app)?;
        Ok(())
    });

    let mut last_tick = Instant::now();
    loop {
        terminal.draw(|f| draw(f, &mut app.lock().unwrap()))?;

        const SEARCH_MODE_CURSOR_OFFSET: u16 = 1;
        const SUBSCRIBE_MODE_CURSOR_OFFSET: u16 = 25;

        if !matches!(
            app.lock().unwrap().input_mode,
            InputMode::Normal | InputMode::Confirmation
        ) {
            terminal.show_cursor()?;
        } else {
            terminal.hide_cursor()?;
        }
        let offset = match app.lock().unwrap().input_mode {
            InputMode::Search => SEARCH_MODE_CURSOR_OFFSET,
            InputMode::Subscribe => SUBSCRIBE_MODE_CURSOR_OFFSET,
            _ => 0,
        };
        terminal.set_cursor(
            app.lock().unwrap().cursor_position + offset,
            terminal.size()?.height - 1,
        )?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                let input_mode = app.lock().unwrap().input_mode.clone();
                match input_mode {
                    InputMode::Normal => match (key.code, key.modifiers) {
                        (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                            break
                        }
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
    RefreshChannels,
}

#[tokio::main]
async fn async_io_loop(
    io_rx: std::sync::mpsc::Receiver<IoEvent>,
    app: Arc<Mutex<App>>,
) -> Result<()> {
    while let Ok(io_event) = io_rx.recv() {
        match io_event {
            IoEvent::SubscribeToChannel(channel_id) => {
                subscribe_to_channel(&app, channel_id).await?
            }
            IoEvent::RefreshChannel(channel_id) => refresh_channel(&app, channel_id).await?,
            IoEvent::RefreshChannels => refresh_channels(&app).await?,
        }
    }
    Ok(())
}

async fn clear_message(app: &Arc<Mutex<App>>, duration_seconds: u64) {
    let app = app.clone();
    tokio::task::spawn(async move {
        tokio::time::sleep(Duration::from_secs(duration_seconds)).await;
        app.lock().unwrap().clear_message();
    });
}

async fn subscribe_to_channel(app: &Arc<Mutex<App>>, channel_id: String) -> Result<()> {
    let instance = app.lock().unwrap().instance();
    app.lock().unwrap().set_message("Subscribing to channel");
    let cloned_app = app.clone();
    match tokio::task::spawn_blocking(move || -> Result<()> {
        let videos_json = instance.get_videos_of_channel(&channel_id)?;
        cloned_app.lock().unwrap().add_channel(videos_json);
        Ok(())
    })
    .await?
    {
        Ok(_) => app.lock().unwrap().clear_message(),
        Err(e) => {
            app.lock()
                .unwrap()
                .set_message(&format!("Failed to subscribe: {:?}", e));
            clear_message(app, 5).await;
            return Ok(());
        }
    }
    Ok(())
}

async fn refresh_channel(app: &Arc<Mutex<App>>, channel_id: String) -> Result<()> {
    let now = std::time::Instant::now();
    let instance = app.lock().unwrap().instance();
    app.lock().unwrap().start_refreshing_channel(&channel_id);
    app.lock().unwrap().set_message("Refreshing channel");
    let cloned_app = app.clone();
    match tokio::task::spawn_blocking(move || -> Result<()> {
        let cloned_id = channel_id.clone();
        let videos_json = instance
            .get_latest_videos_of_channel(&channel_id)
            .with_context(|| cloned_id)?;
        cloned_app
            .lock()
            .unwrap()
            .add_videos(videos_json, &channel_id);
        cloned_app
            .lock()
            .unwrap()
            .complete_refreshing_channel(&channel_id);
        Ok(())
    })
    .await?
    {
        Ok(_) => (),
        Err(e) => {
            app.lock().unwrap().refresh_failed(&e.to_string());
            app.lock().unwrap().set_message("failed to refresh channel");
            return Ok(());
        }
    };
    let elapsed = now.elapsed();
    app.lock()
        .unwrap()
        .set_message(&format!("Refreshed in {:?}", elapsed));
    clear_message(app, 5).await;
    Ok(())
}

async fn refresh_channels(app: &Arc<Mutex<App>>) -> Result<()> {
    let now = std::time::Instant::now();
    app.lock()
        .unwrap()
        .channels
        .items
        .iter_mut()
        .for_each(|channel| channel.set_to_be_refreshed());
    let channel_ids = database::get_channel_ids(&app.lock().unwrap().conn)?;
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
        tokio::task::spawn_blocking(move || -> Result<()> {
            let cloned_id = channel_id.clone();
            let videos_json = instance
                .get_latest_videos_of_channel(&channel_id)
                .with_context(|| cloned_id)?;
            app.lock().unwrap().add_videos(videos_json, &channel_id);
            app.lock().unwrap().complete_refreshing_channel(&channel_id);
            *count.lock().unwrap() += 1;
            app.lock().unwrap().set_message(&format!(
                "Refreshing Channels: {}/{}",
                count.lock().unwrap(),
                total
            ));
            Ok(())
        })
    });
    let mut buffered = streams.buffer_unordered(num_cpus::get());
    while let Some(result) = buffered.next().await {
        if let Err(e) = result? {
            app.lock().unwrap().refresh_failed(&e.to_string());
        }
    }
    let elapsed = now.elapsed();
    app.lock().unwrap().set_message(&format!(
        "Refreshed {} out of {} channels in {:?}",
        count.lock().unwrap(),
        total,
        elapsed
    ));
    clear_message(app, 5).await;
    Ok(())
}
