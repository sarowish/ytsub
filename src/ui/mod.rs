use crate::app::{App, Mode, Selected, StatefulList};
use crate::channel::{HideVideos, tabs_to_be_loaded};
use crate::help::HelpWindowState;
use crate::input::InputMode;
use crate::message::MessageType;
use crate::search::SearchDirection;
use crate::stream_formats::Formats;
use crate::{HELP, OPTIONS, THEME};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, Tabs, Wrap,
};
use std::fmt::Display;
use unicode_width::UnicodeWidthStr;
use utils::{Column, TitleBuilder, filter_columns};

mod utils;

pub fn draw(f: &mut Frame, app: &mut App) {
    let (main_layout, footer) = if app.is_footer_active() {
        let chunks = Layout::default()
            .constraints([Constraint::Min(1), Constraint::Length(1)].as_ref())
            .direction(Direction::Vertical)
            .split(f.area());
        (chunks[0], Some(chunks[1]))
    } else {
        (f.area(), None)
    };
    match app.mode {
        Mode::Subscriptions => draw_subscriptions(f, app, main_layout),
        Mode::LatestVideos => draw_videos(f, app, main_layout),
    }
    if let Some(footer) = footer {
        draw_footer(f, app, footer);
    }

    let input_mode = if matches!(
        app.input_mode,
        InputMode::Search | InputMode::TagCreation | InputMode::TagRenaming
    ) {
        &app.prev_input_mode
    } else {
        &app.input_mode
    };

    match input_mode {
        InputMode::Normal if app.help_window_state.show => draw_help(f, &mut app.help_window_state),
        InputMode::Confirmation => draw_confirmation_window(f, app),
        InputMode::Import => {
            draw_list_with_help(f, "Import".to_string(), &mut app.import_state, &HELP.import);
        }
        InputMode::Tag => draw_list_with_help(f, "Tags".to_string(), &mut app.tags, &HELP.tag),
        InputMode::ChannelSelection => draw_list_with_help(
            f,
            app.tags.get_selected().unwrap().item.clone(),
            &mut app.channel_selection,
            &HELP.channel_selection,
        ),
        InputMode::FormatSelection => draw_format_selection(f, &mut app.stream_formats),
        _ => (),
    }
}

fn draw_subscriptions(f: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
        .direction(Direction::Horizontal)
        .split(area);
    draw_channels(f, app, chunks[0]);
    draw_videos(f, app, chunks[1]);
}

fn draw_channels(f: &mut Frame, app: &mut App, area: Rect) {
    let channels = app
        .channels
        .items
        .iter()
        .map(Line::from)
        .map(ListItem::new)
        .collect::<Vec<ListItem>>();

    let selected_tags = app.tags.get_selected_items();
    let title = TitleBuilder::new(area.width.into())
        .title("Channels".to_string())
        .list(&app.channels)
        .tags(selected_tags)
        .build_title();

    let channels = List::new(channels)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
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

fn draw_videos(f: &mut Frame, app: &mut App, area: Rect) {
    const COLUMN_SPACING: u16 = 2;
    let columns = [
        Column::new("Channel", Constraint::Length(45), 1),
        Column::new("Title", Constraint::Min(90), 0),
        Column::new("Length", Constraint::Fill(1), 4),
        Column::new("Date", Constraint::Fill(1), 10),
    ];

    let columns = match app.mode {
        Mode::LatestVideos => &columns[0..],
        Mode::Subscriptions => &columns[1..],
    };
    let shown_columns = filter_columns(
        columns,
        area.width - 2 - OPTIONS.highlight_symbol.width() as u16,
        COLUMN_SPACING,
    );
    let channel_header_present = shown_columns
        .first()
        .is_some_and(|item| item.header == "Channel");

    let (video_area, video_info_area) =
        if shown_columns.len() < columns.len() && app.get_current_video().is_some() {
            let chunks = Layout::default()
                .constraints([Constraint::Min(10), Constraint::Length(6)])
                .direction(Direction::Vertical)
                .split(area);
            (chunks[0], Some(chunks[1]))
        } else {
            (area, None)
        };

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(match app.selected {
            Selected::Channels => Style::default(),
            Selected::Videos => THEME.selected_block,
        });

    let mut title = TitleBuilder::new(video_area.width.into())
        .hide_flag(app.hide_videos.contains(HideVideos::WATCHED));

    if let Mode::LatestVideos = app.mode {
        let selected_tags = app.tags.get_selected_items();
        title = title.title("Latest Videos".into()).tags(selected_tags);
    } else if let Some(channel) = app.get_current_channel() {
        title = title.title(channel.channel_name.clone());
    }

    if let Some(tab) = app.tabs.get_selected() {
        title = title.list(&tab.videos);

        if tabs_to_be_loaded().count() > 1 {
            title = title.tabs(&app.tabs);
        }

        block = block.title(title.build_title());
    } else {
        f.render_widget(block.title(title.build_title()), video_area);
        return;
    }

    let Some(tab) = app.tabs.get_mut_selected() else {
        return;
    };

    let videos = tab
        .videos
        .items
        .iter()
        .map(|video| {
            let mut columns = Vec::new();

            if channel_header_present && let Some(channel_name) = &video.channel_name {
                columns.push(Cell::from(Span::raw(channel_name)));
            }

            columns.extend([
                Cell::from(Line::from(vec![
                    Span::raw(video.title.clone()),
                    Span::styled(
                        if video.members_only { " [M]" } else { "" },
                        THEME.members_only_indicator,
                    ),
                    Span::styled(
                        if video.new { " [N]" } else { "" },
                        THEME.new_video_indicator,
                    ),
                ])),
                Cell::from(Span::raw(if let Some(length) = video.length {
                    crate::utils::length_as_hhmmss(length)
                } else {
                    String::new()
                })),
                Cell::from(Span::raw(&video.published_text)),
            ]);

            Row::new(columns).style(if video.watched {
                THEME.watched
            } else {
                Style::default()
            })
        })
        .collect::<Vec<Row>>();

    let videos = Table::new(videos, shown_columns.iter().map(|c| c.constraint))
        .block(block)
        .header(Row::new(shown_columns.iter().map(|c| c.header)).style(THEME.header))
        .column_spacing(2)
        .highlight_symbol(&*OPTIONS.highlight_symbol)
        .row_highlight_style({
            let mut style = match app.selected {
                Selected::Channels => THEME.selected,
                Selected::Videos => THEME.focused,
            };
            if let Some(video) = tab.videos.get_selected()
                && video.watched
            {
                let overriding_style = match app.selected {
                    Selected::Channels => THEME.selected_watched,
                    Selected::Videos => THEME.focused_watched,
                };
                style = style.patch(overriding_style);
                style.add_modifier = overriding_style.add_modifier;
                style.sub_modifier = overriding_style.sub_modifier;
            }
            style
        });

    f.render_stateful_widget(videos, video_area, &mut tab.videos.state);

    if let Some(area) = video_info_area {
        draw_video_info(f, app, area);
    }
}

fn draw_video_info(f: &mut Frame, app: &mut App, area: Rect) {
    let current_video = app.get_current_video().unwrap();
    let video_info = Paragraph::new(vec![
        Line::from(format!(
            "channel: {}",
            match &current_video.channel_name {
                Some(channel_name) => channel_name,
                None => &app.get_current_channel().unwrap().channel_name,
            }
        )),
        Line::from(format!("title: {}", current_video.title)),
        Line::from(format!(
            "length: {}",
            if let Some(length) = current_video.length {
                crate::utils::length_as_hhmmss(length)
            } else {
                String::new()
            }
        )),
        Line::from(format!("date: {}", current_video.published_text)),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(Span::styled("Video Info", THEME.title)),
    );
    f.render_widget(video_info, area);
}

fn draw_footer(f: &mut Frame, app: &mut App, area: Rect) {
    let text = match app.input_mode {
        InputMode::Search => Paragraph::new(Line::from(vec![
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
        InputMode::TagCreation | InputMode::TagRenaming => Paragraph::new(Line::from(vec![
            Span::raw("Tag name: "),
            Span::raw(&app.input),
        ])),
        InputMode::Subscribe => Paragraph::new(Line::from(vec![
            Span::raw("Enter channel id or url: "),
            Span::raw(&app.input),
        ])),
        _ => Paragraph::new(match app.message.message_type {
            MessageType::Normal => Span::raw(&*app.message),
            MessageType::Error => Span::styled(&*app.message, THEME.error),
            MessageType::Warning => Span::styled(&*app.message, THEME.warning),
        }),
    };
    f.render_widget(text, area);
}

fn draw_confirmation_window(f: &mut Frame, app: &App) {
    let window = popup_window_from_percentage(50, 15, f.area());
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
    let mut text = Paragraph::new(Line::from(format!(
        "Are you sure you want to unsubscribe from '{channel_name}'?"
    )))
    .alignment(Alignment::Center);
    // program crashes if width is 0 and wrap is enabled
    if chunks[0].width > 0 {
        text = text.wrap(Wrap { trim: true });
    }

    let yes = Paragraph::new(Line::from(vec![
        Span::styled("Y", Style::default().fg(Color::Green)),
        Span::raw("es"),
    ]))
    .alignment(Alignment::Center);
    let no = Paragraph::new(Line::from(vec![
        Span::styled("N", Style::default().fg(Color::Red)),
        Span::raw("o"),
    ]))
    .alignment(Alignment::Center);

    f.render_widget(text, chunks[0]);
    f.render_widget(yes, yes_area);
    f.render_widget(no, no_area);
}

fn draw_help(f: &mut Frame, help_window_state: &mut HelpWindowState) {
    let window = popup_window_from_percentage(80, 70, f.area());
    f.render_widget(Clear, window);

    let width = std::cmp::max(window.width.saturating_sub(2), 1);

    let help_entries = HELP
        .iter()
        .map(|(key, desc)| Line::from(vec![Span::styled(key, THEME.help), Span::raw(*desc)]))
        .collect::<Vec<Line>>();

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

fn draw_format_selection(f: &mut Frame, stream_formats: &mut Formats) {
    let tabs = Tabs::new(vec![
        Line::from("Video"),
        Line::from(Span::styled(
            "Audio",
            if stream_formats.use_adaptive_streams {
                Style::default()
            } else {
                THEME.watched
            },
        )),
        Line::from(Span::styled(
            "Caption",
            if stream_formats.captions.items.is_empty() {
                THEME.watched
            } else {
                Style::default()
            },
        )),
    ])
    .select(stream_formats.selected_tab)
    .highlight_style(THEME.selected);

    draw_list_with_help_tabs(
        f,
        if stream_formats.use_adaptive_streams {
            "Adaptive Formats".to_string()
        } else {
            "Formats".to_string()
        },
        Some(tabs),
        stream_formats.get_mut_selected_tab(),
        &HELP.format_selection,
    );
}

fn draw_list_with_help<T: Display>(
    f: &mut Frame,
    title: String,
    list: &mut StatefulList<T, ListState>,
    help_entries: &[(String, &str)],
) {
    draw_list_with_help_tabs(f, title, None, list, help_entries);
}

fn draw_list_with_help_tabs<T: Display>(
    f: &mut Frame,
    title: String,
    tabs: Option<Tabs>,
    list: &mut StatefulList<T, ListState>,
    help_entries: &[(String, &str)],
) {
    const VER_MARGIN: u16 = 6;
    const RIGHT_PADDING: u16 = 4;

    let item_texts: Vec<Span> = list
        .items
        .iter()
        .map(ToString::to_string)
        .map(Span::raw)
        .collect();

    let mut spans = Vec::new();

    for entry in help_entries {
        spans.push(Span::styled(entry.0.clone(), THEME.help));
        spans.push(Span::raw(entry.1));
    }

    let help_text = Line::from(spans);

    let help_text_width = help_text.width();
    let help_text_height = 1 + help_text_width as u16 / f.area().width;

    let max_width = item_texts
        .iter()
        .map(Span::width)
        .max()
        .unwrap_or(0)
        .max(help_text_width) as u16
        + RIGHT_PADDING;

    let frame_height = f.area().height;
    let tabs_height = u16::from(tabs.is_some());

    let mut max_height = item_texts.len() as u16 + help_text_height + tabs_height + 2;
    max_height = if frame_height <= max_height + VER_MARGIN {
        frame_height.saturating_sub(VER_MARGIN)
    } else {
        max_height
    }
    .max(10);

    let window = popup_window_from_dimensions(max_height, max_width, f.area());
    f.render_widget(Clear, window);

    let title = TitleBuilder::new(window.width.into())
        .title(title)
        .list(list)
        .build_title();

    f.render_widget(Block::default().borders(Borders::ALL).title(title), window);

    let (entry_area, help_area) = {
        let layout = Layout::default().direction(Direction::Vertical).margin(1);
        let chunks;

        if let Some(tabs) = tabs {
            chunks = layout
                .constraints([
                    Constraint::Length(1),
                    Constraint::Min(1),
                    Constraint::Length(help_text_height),
                ])
                .split(window);

            f.render_widget(tabs, chunks[0]);
            (chunks[1], chunks[2])
        } else {
            chunks = layout
                .constraints([Constraint::Min(1), Constraint::Length(help_text_height)])
                .split(window);
            (chunks[0], chunks[1])
        }
    };

    let mut help_widget = Paragraph::new(help_text);
    if window.width > 0 {
        help_widget = help_widget.wrap(Wrap { trim: false });
    }

    let list_items = item_texts
        .into_iter()
        .map(ListItem::new)
        .collect::<Vec<ListItem>>();

    let w = List::new(list_items)
        .highlight_symbol(&OPTIONS.highlight_symbol)
        .highlight_style(THEME.focused);

    f.render_stateful_widget(w, entry_area, &mut list.state);
    f.render_widget(help_widget, help_area);
}

fn popup_window_from_dimensions(height: u16, width: u16, r: Rect) -> Rect {
    let hor = [
        Constraint::Length(r.width.saturating_sub(width) / 2),
        Constraint::Length(width),
        Constraint::Min(1),
    ];

    let ver = [
        Constraint::Length(r.height.saturating_sub(height) / 2),
        Constraint::Length(height),
        Constraint::Min(1),
    ];

    popup_window(&hor, &ver, r)
}

fn popup_window_from_percentage(hor_percent: u16, ver_percent: u16, r: Rect) -> Rect {
    let ver = [
        Constraint::Percentage((100 - ver_percent) / 2),
        Constraint::Percentage(ver_percent),
        Constraint::Percentage((100 - ver_percent) / 2),
    ];

    let hor = [
        Constraint::Percentage((100 - hor_percent) / 2),
        Constraint::Percentage(hor_percent),
        Constraint::Percentage((100 - hor_percent) / 2),
    ];

    popup_window(&hor, &ver, r)
}

fn popup_window(hor_constraints: &[Constraint], ver_constraints: &[Constraint], r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(ver_constraints)
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(hor_constraints)
        .split(popup_layout[1])[1]
}
