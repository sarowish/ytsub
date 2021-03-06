use crate::app::{App, Mode, Selected, State, StatefulList};
use crate::channel::VideoType;
use crate::help::HelpWindowState;
use crate::input::InputMode;
use crate::message::MessageType;
use crate::search::SearchDirection;
use crate::{utils, HELP, OPTIONS, THEME};
use tui::backend::Backend;
use tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tui::style::{Color, Style};
use tui::text::{Span, Spans};
use tui::widgets::{
    Block, BorderType, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, Wrap,
};
use tui::Frame;

pub fn draw<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let (main_layout, footer) = if app.is_footer_active() {
        let chunks = Layout::default()
            .constraints([Constraint::Min(1), Constraint::Length(1)].as_ref())
            .direction(Direction::Vertical)
            .split(f.size());
        (chunks[0], Some(chunks[1]))
    } else {
        (f.size(), None)
    };
    match app.mode {
        Mode::Subscriptions => draw_subscriptions(f, app, main_layout),
        Mode::LatestVideos => draw_videos(f, app, main_layout),
    }
    if let Some(footer) = footer {
        draw_footer(f, app, footer);
    }

    if let InputMode::Confirmation = app.input_mode {
        draw_confirmation_window(f, app);
    }

    if app.help_window_state.show {
        draw_help(f, &mut app.help_window_state);
    }
}

fn draw_subscriptions<B: Backend>(f: &mut Frame<B>, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
        .direction(Direction::Horizontal)
        .split(area);
    draw_channels(f, app, chunks[0]);
    draw_videos(f, app, chunks[1]);
}

fn draw_channels<B: Backend>(f: &mut Frame<B>, app: &mut App, area: Rect) {
    let channels = app
        .channels
        .items
        .iter()
        .map(|ch| ch.to_string())
        .map(Span::raw)
        .map(ListItem::new)
        .collect::<Vec<ListItem>>();
    let channels = List::new(channels)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(gen_title(
                    "Channels".into(),
                    false,
                    &app.channels,
                    area.width as usize,
                ))
                .border_style(match app.selected {
                    Selected::Channels => THEME.selected_block,
                    Selected::Videos => Style::default(),
                }),
        )
        .highlight_symbol(&OPTIONS.highlight_symbol)
        .highlight_style(match app.selected {
            Selected::Channels => THEME.focused,
            Selected::Videos => THEME.selected,
        });
    f.render_stateful_widget(channels, area, &mut app.channels.state);
}

fn draw_videos<B: Backend>(f: &mut Frame<B>, app: &mut App, area: Rect) {
    let (video_area, video_info_area) = if (matches!(app.mode, Mode::LatestVideos if area.width < 140)
        || matches!(app.mode, Mode::Subscriptions if area.width < 117))
        && app.get_current_video().is_some()
    {
        let chunks = Layout::default()
            .constraints([Constraint::Min(10), Constraint::Length(6)])
            .direction(Direction::Vertical)
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };
    let videos = app
        .videos
        .items
        .iter()
        .map(|video| {
            let mut columns = Vec::new();

            if let Some(VideoType::LatestVideos(channel_name)) = &video.video_type {
                columns.push(Cell::from(Span::raw(channel_name)))
            }

            columns.extend([
                Cell::from(Span::raw(format!(
                    "{} {}",
                    video.title,
                    if video.new { "[N]" } else { "" }
                ))),
                Cell::from(Span::raw(utils::as_hhmmss(video.length))),
                Cell::from(Span::raw(&video.published_text)),
            ]);

            Row::new(columns).style(if video.watched {
                THEME.watched
            } else {
                Style::default()
            })
        })
        .collect::<Vec<Row>>();
    let videos = Table::new(videos)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(if let Mode::LatestVideos = app.mode {
                    gen_title(
                        "Latest Videos".into(),
                        app.hide_watched,
                        &app.videos,
                        video_area.width.into(),
                    )
                } else if let Some(channel) = app.get_current_channel() {
                    gen_title(
                        channel.channel_name.clone(),
                        app.hide_watched,
                        &app.videos,
                        video_area.width.into(),
                    )
                } else {
                    Default::default()
                })
                .border_style(match app.selected {
                    Selected::Channels => Style::default(),
                    Selected::Videos => THEME.selected_block,
                }),
        )
        .header(
            Row::new(match app.mode {
                Mode::Subscriptions => vec!["Title", "Length", "Date"],
                Mode::LatestVideos => vec!["Channel", "Title", "Length", "Date"],
            })
            .style(THEME.header),
        )
        .column_spacing(2)
        .widths(match app.mode {
            Mode::Subscriptions => &[
                Constraint::Min(90),
                Constraint::Min(20),
                Constraint::Min(30),
            ],
            Mode::LatestVideos => &[
                Constraint::Percentage(15),
                Constraint::Min(90),
                Constraint::Min(20),
                Constraint::Min(30),
            ],
        })
        .highlight_symbol(&OPTIONS.highlight_symbol)
        .highlight_style({
            let mut style = match app.selected {
                Selected::Channels => THEME.selected,
                Selected::Videos => THEME.focused,
            };
            if let Some(video) = app.get_current_video() {
                if video.watched {
                    let overriding_style = match app.selected {
                        Selected::Channels => THEME.selected_watched,
                        Selected::Videos => THEME.focused_watched,
                    };
                    style = style.patch(overriding_style);
                    style.add_modifier = overriding_style.add_modifier;
                    style.sub_modifier = overriding_style.sub_modifier;
                }
            }
            style
        });
    f.render_stateful_widget(videos, video_area, &mut app.videos.state);
    if let Some(area) = video_info_area {
        draw_video_info(f, app, area);
    }
}

fn draw_video_info<B: Backend>(f: &mut Frame<B>, app: &mut App, area: Rect) {
    let current_video = app.get_current_video().unwrap();
    let video_info = Paragraph::new(vec![
        Spans::from(format!(
            "channel: {}",
            match &current_video.video_type {
                Some(VideoType::LatestVideos(channel_name)) => channel_name,
                Some(VideoType::Subscriptions) => &app.get_current_channel().unwrap().channel_name,
                None => "",
            }
        )),
        Spans::from(format!("title: {}", current_video.title)),
        Spans::from(format!(
            "length: {}",
            utils::as_hhmmss(current_video.length)
        )),
        Spans::from(format!("date: {}", current_video.published_text)),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(Span::styled("Video Info", THEME.title)),
    );
    f.render_widget(video_info, area);
}

fn draw_footer<B: Backend>(f: &mut Frame<B>, app: &mut App, area: Rect) {
    let text = match app.input_mode {
        InputMode::Search => Paragraph::new(Spans::from(vec![
            Span::raw(match app.search_direction() {
                SearchDirection::Forward => "/",
                SearchDirection::Backward => "?",
            }),
            Span::styled(
                &app.input,
                if app.no_search_pattern_match() {
                    THEME.error
                } else {
                    Style::default()
                },
            ),
        ])),
        InputMode::Subscribe => Paragraph::new(Spans::from(vec![
            Span::raw("Enter channel id or url: "),
            Span::raw(&app.input),
        ])),
        _ => Paragraph::new(match app.message.message_type {
            MessageType::Normal => Span::raw(&*app.message),
            MessageType::Error => Span::styled(&*app.message, THEME.error),
        }),
    };
    f.render_widget(text, area);
}

fn draw_confirmation_window<B: Backend>(f: &mut Frame<B>, app: &App) {
    let window = popup_window(50, 15, f.size());
    f.render_widget(Clear, window);
    f.render_widget(Block::default().borders(Borders::ALL), window);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(80), Constraint::Min(1)])
        .margin(1)
        .split(window);

    let (yes_area, no_area) = {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[1]);
        (chunks[0], chunks[1])
    };

    let channel_name = &app.get_current_channel().unwrap().channel_name;
    let mut text = Paragraph::new(Spans::from(format!(
        "Are you sure you want to unsubscribe from '{}'?",
        channel_name
    )))
    .alignment(Alignment::Center);
    // program crashes if width is 0 and wrap is enabled
    if chunks[0].width > 0 {
        text = text.wrap(Wrap { trim: true });
    }

    let yes = Paragraph::new(Spans::from(vec![
        Span::styled("Y", Style::default().fg(Color::Green)),
        Span::raw("es"),
    ]))
    .alignment(Alignment::Center);
    let no = Paragraph::new(Spans::from(vec![
        Span::styled("N", Style::default().fg(Color::Red)),
        Span::raw("o"),
    ]))
    .alignment(Alignment::Center);

    f.render_widget(text, chunks[0]);
    f.render_widget(yes, yes_area);
    f.render_widget(no, no_area);
}

fn draw_help<B: Backend>(f: &mut Frame<B>, help_window_state: &mut HelpWindowState) {
    let window = popup_window(80, 70, f.size());
    f.render_widget(Clear, window);

    let width = std::cmp::max(window.width.saturating_sub(2), 1);

    let help_entries = HELP
        .iter()
        .map(|(key, desc)| Spans::from(vec![Span::styled(key, THEME.help), Span::raw(*desc)]))
        .collect::<Vec<Spans>>();

    help_window_state.max_scroll = help_entries
        .iter()
        .map(|entry| 1 + entry.width().saturating_sub(1) as u16 / width)
        .sum::<u16>()
        .saturating_sub(window.height - 2);

    if help_window_state.max_scroll < help_window_state.scroll {
        help_window_state.scroll = help_window_state.max_scroll;
    }

    let mut help_text = Paragraph::new(help_entries)
        .scroll((help_window_state.scroll, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled("Help", THEME.title)),
        );

    if window.width > 0 {
        help_text = help_text.wrap(Wrap { trim: false });
    }

    f.render_widget(help_text, window);
}

fn popup_window(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}

fn gen_title<'a, T, S: State>(
    title: String,
    hide_flag: bool,
    list: &StatefulList<T, S>,
    area_width: usize,
) -> Vec<Span<'a>> {
    let title = Span::styled(title, THEME.title);

    let position = Span::styled(
        format!(
            "{}/{}",
            if let Some(index) = list.state.selected() {
                index + 1
            } else {
                0
            },
            list.items.len()
        ),
        THEME.title,
    );

    let border_symbol = BorderType::line_symbols(BorderType::Plain).horizontal;
    const MIN_GAP: usize = 2;
    let mut required_space = title.width() + position.width() + 2 + MIN_GAP;

    let mut title_sections = Vec::with_capacity(5);
    title_sections.push(title);

    if hide_flag {
        title_sections.push(Span::raw(border_symbol));
        title_sections.push(Span::styled("[H]", THEME.title));
        required_space += 4;
    }

    if let Some(p_gap_width) = area_width.checked_sub(required_space) {
        let fill = Span::raw(border_symbol.repeat(p_gap_width + MIN_GAP));
        title_sections.push(fill);
        title_sections.push(position);
    }

    title_sections
}
