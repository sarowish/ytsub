mod app;
mod channel;
mod cli;
mod database;
mod input;
mod search;
mod ui;
mod utils;

use app::App;
use clap::Parser;
use cli::Options;
use crossterm::event;
use crossterm::event::{Event, KeyCode};
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

fn main() {
    let options = Options::parse();
    if options.gen_instance_list {
        utils::generate_instances_file();
    }

    let (sync_io_tx, sync_io_rx) = std::sync::mpsc::channel();

    enable_raw_mode().unwrap();
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).unwrap();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).unwrap();

    let app = Arc::new(Mutex::new(App::new(options, sync_io_tx)));

    let cloned_app = app.clone();
    std::thread::spawn(move || {
        async_io_loop(sync_io_rx, cloned_app);
    });

    {
        let mut app = app.lock().unwrap();
        database::initialize_db(&app.conn);
        app.set_mode_subs();
        app.load_channels();
        app.select_first();
        app.add_new_channels();
    }

    let tick_rate = Duration::from_millis(200);
    let mut last_tick = Instant::now();
    loop {
        terminal
            .draw(|f| draw(f, &mut app.lock().unwrap()))
            .unwrap();
        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));
        if crossterm::event::poll(timeout).unwrap() {
            if let Event::Key(key) = event::read().unwrap() {
                let input_mode = app.lock().unwrap().input_mode.clone();
                match input_mode {
                    InputMode::Normal => {
                        if let KeyCode::Char('q') = key.code {
                            break;
                        } else {
                            input::handle_key_normal_mode(key, &mut app.lock().unwrap());
                        }
                    }
                    InputMode::Editing => {
                        input::handle_key_input_mode(key, &mut app.lock().unwrap())
                    }
                }
            }
        }
        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }
    disable_raw_mode().unwrap();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).unwrap();
}

pub enum IoEvent {
    AddNewChannels,
    RefreshChannel(String),
    RefreshChannels,
}

#[tokio::main]
async fn async_io_loop(io_rx: std::sync::mpsc::Receiver<IoEvent>, app: Arc<Mutex<App>>) {
    while let Ok(io_event) = io_rx.recv() {
        match io_event {
            IoEvent::AddNewChannels => add_new_channels(&app).await,
            IoEvent::RefreshChannel(channel_id) => refresh_channel(&app, channel_id).await,
            IoEvent::RefreshChannels => refresh_channels(&app).await,
        }
        app.lock().unwrap().load_videos();
    }
}

async fn clear_message(app: &Arc<Mutex<App>>) {
    let app = app.clone();
    tokio::task::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        app.lock().unwrap().clear_message();
    });
}

async fn add_new_channels(app: &Arc<Mutex<App>>) {
    let channel_ids = app.lock().unwrap().get_new_channel_ids();
    let instance = app.lock().unwrap().instance();
    let streams = futures_util::stream::iter(channel_ids).map(|channel_id| {
        let instance = instance.clone();
        let app = app.clone();

        tokio::task::spawn_blocking(move || {
            let videos_json = instance.get_videos_of_channel(&channel_id);
            app.lock().unwrap().add_channel(videos_json);
        })
    });
    let mut buffered = streams.buffer_unordered(num_cpus::get());
    while buffered.next().await.is_some() {}
}

async fn refresh_channel(app: &Arc<Mutex<App>>, channel_id: String) {
    let now = std::time::Instant::now();
    let instance = app.lock().unwrap().instance();
    app.lock().unwrap().start_refreshing_channel(&channel_id);
    app.lock().unwrap().set_message("Refreshing channel");
    let cloned_app = app.clone();
    tokio::task::spawn_blocking(move || {
        let videos_json = instance.get_videos_of_channel(&channel_id);
        cloned_app
            .lock()
            .unwrap()
            .add_videos(videos_json, &channel_id);
        cloned_app
            .lock()
            .unwrap()
            .complete_refreshing_channel(&channel_id);
    })
    .await
    .unwrap();
    let elapsed = now.elapsed();
    app.lock()
        .unwrap()
        .set_message(&format!("Refreshed in {:?}", elapsed));
    clear_message(app).await;
}

async fn refresh_channels(app: &Arc<Mutex<App>>) {
    let now = std::time::Instant::now();
    app.lock()
        .unwrap()
        .channels
        .items
        .iter_mut()
        .for_each(|channel| channel.set_to_be_refreshed());
    let channel_ids = app.lock().unwrap().channel_ids.clone();
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
        tokio::task::spawn_blocking(move || {
            let videos_json = instance.get_videos_of_channel(&channel_id);
            app.lock().unwrap().add_videos(videos_json, &channel_id);
            app.lock().unwrap().complete_refreshing_channel(&channel_id);
            *count.lock().unwrap() += 1;
            app.lock().unwrap().set_message(&format!(
                "Refreshing Channels: {}/{}",
                count.lock().unwrap(),
                total
            ));
        })
    });
    let mut buffered = streams.buffer_unordered(num_cpus::get());
    while buffered.next().await.is_some() {}
    let elapsed = now.elapsed();
    app.lock()
        .unwrap()
        .set_message(&format!("Refreshed in {:?}", elapsed));
    clear_message(app).await;
}
