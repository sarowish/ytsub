use crate::app::{App, Mode, Selected, State, StatefulList};
use crate::help::HelpWindowState;
use crate::input::InputMode;
use crate::message::MessageType;
use crate::search::SearchDirection;
use crate::stream_formats::Formats;
use crate::{utils, HELP, OPTIONS, THEME};
use std::fmt::Display;
use tui::backend::Backend;
use tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tui::style::{Color, Style};
use tui::text::{Span, Spans};
use tui::widgets::{
    Block, BorderType, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table,
    Tabs, Wrap,
};
use tui::Frame;
use unicode_width::UnicodeWidthStr;

struct TitleBuilder<'a, T, S: State> {
    title: String,
    hide_flag: bool,
    list: Option<&'a StatefulList<T, S>>,
    tags: Option<Vec<&'a String>>,
    available_width: usize,
}

impl<'a, T, S: State> TitleBuilder<'a, T, S> {
    fn new(available_width: usize) -> Self {
        Self {
            title: String::new(),
            hide_flag: false,
            list: None,
            tags: None,
            available_width: available_width.saturating_sub(2),
        }
    }

    fn title(mut self, title: String) -> Self {
        self.title = title;
        self
    }

    fn hide_flag(mut self, hide: bool) -> Self {
        self.hide_flag = hide;
        self
    }

    fn list(mut self, list: &'a StatefulList<T, S>) -> Self {
        self.list = Some(list);
        self
    }

    fn tags(mut self, tags: Vec<&'a String>) -> Self {
        if !tags.is_empty() {
            self.tags = Some(tags);
        }

        self
    }

    fn build_title<'b>(mut self) -> Vec<Span<'b>> {
        const MIN_GAP: usize = 2;

        let mut title_sections = Vec::with_capacity(7);
        let border_symbol = BorderType::line_symbols(BorderType::Plain).horizontal;

        if !self.title.is_empty() {
            let title = Span::styled(self.title, THEME.title);
            self.available_width = self.available_width.saturating_sub(title.width());

            title_sections.push(title);
        }

        if self.hide_flag {
            self.available_width = self.available_width.saturating_sub(4);
        }

        let position = if let Some(list) = self.list {
            Span::styled(
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
            )
        } else {
            Span::raw("")
        };

        let required_width_for_position = if self.list.is_some() {
            position.width() + MIN_GAP
        } else {
            0
        };

        if let Some(tags) = self.tags {
            let mut available_width = self
                .available_width
                .saturating_sub(required_width_for_position + 3);

            let mut shown_tags = Vec::new();

            for tag in tags {
                if tag.len() > available_width {
                    if 2 > available_width {
                        shown_tags.pop();
                    }

                    shown_tags.push("..".to_string());
                    break;
                }

                shown_tags.push(tag.to_string());
                available_width = available_width.saturating_sub(tag.width() + 2);
            }

            let tag_text = format!("[{}]", shown_tags.join(", "));
            self.available_width = self.available_width.saturating_sub(tag_text.width() + 1);

            title_sections.push(Span::raw(border_symbol));
            title_sections.push(Span::styled(tag_text, THEME.title));
        }

        if self.hide_flag {
            title_sections.push(Span::raw(border_symbol));
            title_sections.push(Span::styled("[H]", THEME.title));
        }

        if let Some(p_gap_width) = self
            .available_width
            .checked_sub(required_width_for_position)
        {
            let fill = Span::raw(border_symbol.repeat(p_gap_width + MIN_GAP));
            title_sections.push(fill);
            title_sections.push(position);
        }

        title_sections
    }
}

fn filter_columns(constraints: &[(Constraint, u16)], mut available_width: u16) -> Vec<Constraint> {
    constraints
        .iter()
        .filter_map(|(constraint, min_width)| match constraint {
            Constraint::Percentage(perc) => {
                available_width = available_width.saturating_sub((available_width * perc) / 100);
                Some(*constraint)
            }
            Constraint::Min(width) => {
                if *min_width >= available_width {
                    None
                } else {
                    available_width = available_width.saturating_sub(*width);
                    Some(*constraint)
                }
            }
            _ => panic!(),
        })
        .collect()
}

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
            draw_list_with_help(f, "Import".to_string(), &mut app.import_state, &HELP.import)
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

fn draw_videos<B: Backend>(f: &mut Frame<B>, app: &mut App, area: Rect) {
    const COLUMN_SPACING: u16 = 2;
    const COLUMN_CONSTRAINTS: &[(Constraint, u16); 4] = &[
        (Constraint::Percentage(15), 0),
        (Constraint::Min(90), 0),
        (Constraint::Min(20), 4),
        (Constraint::Min(30), 11),
    ];

    let column_constraints = match app.mode {
        Mode::LatestVideos => &COLUMN_CONSTRAINTS[0..],
        Mode::Subscriptions => &COLUMN_CONSTRAINTS[1..],
    };
    let shown_column_constraints = filter_columns(
        column_constraints,
        area.width
            .saturating_sub((column_constraints.len() as u16 - 1) * COLUMN_SPACING),
    );

    let (video_area, video_info_area) = if shown_column_constraints.len() < column_constraints.len()
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

            if let Some(channel_name) = &video.channel_name {
                columns.push(Cell::from(Span::raw(channel_name)))
            }

            columns.extend([
                Cell::from(Span::raw(format!(
                    "{} {}",
                    video.title,
                    if video.new { "[N]" } else { "" }
                ))),
                Cell::from(Span::raw(if let Some(length) = video.length {
                    utils::length_as_hhmmss(length)
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

    let title = TitleBuilder::new(video_area.width.into())
        .hide_flag(app.hide_watched)
        .list(&app.videos);

    let title = if let Mode::LatestVideos = app.mode {
        let selected_tags = app.tags.get_selected_items();
        title
            .title("Latest Videos".into())
            .tags(selected_tags)
            .build_title()
    } else if let Some(channel) = app.get_current_channel() {
        title.title(channel.channel_name.clone()).build_title()
    } else {
        Default::default()
    };

    let videos = Table::new(videos)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
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
        .widths(&shown_column_constraints)
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
            match &current_video.channel_name {
                Some(channel_name) => channel_name,
                None => &app.get_current_channel().unwrap().channel_name,
            }
        )),
        Spans::from(format!("title: {}", current_video.title)),
        Spans::from(format!(
            "length: {}",
            if let Some(length) = current_video.length {
                utils::length_as_hhmmss(length)
            } else {
                String::new()
            }
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
        InputMode::TagCreation | InputMode::TagRenaming => Paragraph::new(Spans::from(vec![
            Span::raw("Tag name: "),
            Span::raw(&app.input),
        ])),
        InputMode::Subscribe => Paragraph::new(Spans::from(vec![
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

fn draw_confirmation_window<B: Backend>(f: &mut Frame<B>, app: &App) {
    let window = popup_window_from_percentage(50, 15, f.size());
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
        "Are you sure you want to unsubscribe from '{channel_name}'?"
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
    let window = popup_window_from_percentage(80, 70, f.size());
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

fn draw_format_selection<B: Backend>(f: &mut Frame<B>, stream_formats: &mut Formats) {
    let tabs = Tabs::new(vec![
        Spans::from("Video"),
        Spans::from(Span::styled(
            "Audio",
            if stream_formats.use_adaptive_streams {
                Style::default()
            } else {
                THEME.watched
            },
        )),
        Spans::from(Span::styled(
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
    )
}

fn draw_list_with_help<T: Display, B: Backend>(
    f: &mut Frame<B>,
    title: String,
    list: &mut StatefulList<T, ListState>,
    help_entries: &[(String, &str)],
) {
    draw_list_with_help_tabs(f, title, None, list, help_entries)
}

fn draw_list_with_help_tabs<T: Display, B: Backend>(
    f: &mut Frame<B>,
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
        .map(|entry| entry.to_string())
        .map(Span::raw)
        .collect();

    let mut spans = Vec::new();

    for entry in help_entries {
        spans.push(Span::styled(entry.0.clone(), THEME.help));
        spans.push(Span::raw(entry.1));
    }

    let help_text = Spans::from(spans);

    let help_text_width = help_text.width();
    let help_text_height = 1 + help_text_width as u16 / f.size().width;

    let max_width = item_texts
        .iter()
        .map(|text| text.width())
        .max()
        .unwrap_or(0)
        .max(help_text_width) as u16
        + RIGHT_PADDING;

    let frame_height = f.size().height;
    let tabs_height = if tabs.is_some() { 1 } else { 0 };

    let mut max_height = item_texts.len() as u16 + help_text_height + tabs_height + 2;
    max_height = if frame_height <= max_height + VER_MARGIN {
        frame_height.saturating_sub(VER_MARGIN)
    } else {
        max_height
    }
    .max(10);

    let window = popup_window_from_dimensions(max_height, max_width, f.size());
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
