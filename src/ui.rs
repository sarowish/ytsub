use crate::app::{App, Mode, Selected};
use crate::channel::RefreshState;
use tui::backend::Backend;
use tui::layout::{Constraint, Direction, Layout, Rect};
use tui::style::{Color, Modifier, Style};
use tui::text::Span;
use tui::widgets::{Block, BorderType, Borders, List, ListItem};
use tui::Frame;

pub fn draw<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    match app.mode {
        Mode::Subscriptions => draw_subscriptions(f, app),
        Mode::LatestVideos => draw_latest_videos(f, app),
    }
}

fn draw_subscriptions<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let chunks = Layout::default()
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
        .direction(Direction::Horizontal)
        .split(f.size());
    draw_channels(f, app, chunks[0]);
    draw_videos(f, app, chunks[1]);
}

fn draw_latest_videos<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let chunks = Layout::default()
        .constraints([Constraint::Percentage(100)].as_ref())
        .direction(Direction::Horizontal)
        .split(f.size());
    draw_videos(f, app, chunks[0]);
}

fn draw_channels<B: Backend>(f: &mut Frame<B>, app: &mut App, area: Rect) {
    let channels = app
        .channels
        .items
        .iter()
        .map(|ch| {
            let refresh_indicator = match ch.refresh_state {
                RefreshState::ToBeRefreshed => "□ ",
                RefreshState::Refreshing => "■ ",
                RefreshState::Completed => "",
            };
            let new_video_indicator = if ch.new_video { " [N]" } else { "" };
            format!(
                "{}{}{}",
                refresh_indicator,
                ch.channel_name.clone(),
                new_video_indicator
            )
        })
        .map(Span::raw)
        .map(ListItem::new)
        .collect::<Vec<ListItem>>();
    let channels = List::new(channels)
        .block(Block::default().borders(Borders::ALL).title(gen_title(
            app,
            "Channels".into(),
            Selected::Channels,
            area.width as usize,
        )))
        .highlight_style(match app.selected {
            Selected::Channels => Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
            Selected::Videos => Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        });
    f.render_stateful_widget(channels, area, &mut app.channels.state);
}

fn draw_videos<B: Backend>(f: &mut Frame<B>, app: &mut App, area: Rect) {
    let videos = app
        .videos
        .items
        .iter()
        .map(|video| {
            let title = video.title.clone();
            if video.watched {
                Span::styled(title, Style::default().fg(Color::DarkGray))
            } else {
                Span::raw(title)
            }
        })
        .map(ListItem::new)
        .collect::<Vec<ListItem>>();
    let videos =
        List::new(videos)
            .block(Block::default().borders(Borders::ALL).title(
                if let Mode::LatestVideos = app.mode {
                    gen_title(
                        app,
                        "Latest Videos".into(),
                        Selected::Videos,
                        area.width.into(),
                    )
                } else if let Some(channel) = app.get_current_channel() {
                    gen_title(
                        app,
                        channel.channel_name.clone(),
                        Selected::Videos,
                        area.width.into(),
                    )
                } else {
                    Default::default()
                },
            ))
            .highlight_style({
                let mut style = Style::default();
                style = match app.selected {
                    Selected::Channels => style.fg(Color::Blue),
                    Selected::Videos => style.fg(Color::Magenta),
                };
                if let Some(video) = app.get_current_video() {
                    if !video.watched {
                        style = style.add_modifier(Modifier::BOLD)
                    }
                }
                style
            });
    f.render_stateful_widget(videos, area, &mut app.videos.state);
}

fn gen_title<'a>(app: &App, title: String, selected: Selected, area_width: usize) -> Vec<Span<'a>> {
    let style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);

    let title = Span::styled(title, style);

    let (current_pos, len) = match selected {
        Selected::Channels => (
            if let Some(index) = app.channels.state.selected() {
                index + 1
            } else {
                0
            },
            app.channels.items.len(),
        ),
        Selected::Videos => (
            if let Some(index) = app.videos.state.selected() {
                index + 1
            } else {
                0
            },
            app.videos.items.len(),
        ),
    };
    let position = Span::styled(format!("{}/{}", current_pos, len), style);

    let border_symbol = BorderType::line_symbols(BorderType::Plain).horizontal;
    let fill = Span::raw(
        border_symbol.repeat(area_width as usize - (title.width() + position.width() + 2)),
    );

    let border_symbol = BorderType::line_symbols(BorderType::Plain).horizontal;
    const MIN_GAP: usize = 2;
    let required_space = title.width() + position.width() + 2 + MIN_GAP;
    if let Some(p_gap_width) = area_width.checked_sub(required_space) {
        let fill = Span::raw(border_symbol.repeat(p_gap_width + MIN_GAP));
        vec![title, fill, position]
    } else {
        vec![title]
    }
}
