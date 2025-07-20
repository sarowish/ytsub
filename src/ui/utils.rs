use crate::{
    THEME,
    app::{State, StatefulList},
};
use ratatui::{layout::Constraint, text::Span, widgets::BorderType};
use unicode_width::UnicodeWidthStr;

pub struct TitleBuilder<'a, T, S: State> {
    title: String,
    hide_flag: bool,
    list: Option<&'a StatefulList<T, S>>,
    tags: Option<Vec<&'a String>>,
    available_width: usize,
}

impl<'a, T, S: State> TitleBuilder<'a, T, S> {
    pub fn new(available_width: usize) -> Self {
        Self {
            title: String::new(),
            hide_flag: false,
            list: None,
            tags: None,
            available_width: available_width.saturating_sub(2),
        }
    }

    pub fn title(mut self, title: String) -> Self {
        self.title = title;
        self
    }

    pub fn hide_flag(mut self, hide: bool) -> Self {
        self.hide_flag = hide;
        self
    }

    pub fn list(mut self, list: &'a StatefulList<T, S>) -> Self {
        self.list = Some(list);
        self
    }

    pub fn tags(mut self, tags: Vec<&'a String>) -> Self {
        if !tags.is_empty() {
            self.tags = Some(tags);
        }

        self
    }

    pub fn build_title<'b>(mut self) -> Vec<Span<'b>> {
        const MIN_GAP: usize = 2;

        let mut title_sections = Vec::with_capacity(7);
        let border_symbol = BorderType::border_symbols(BorderType::Plain).horizontal_top;

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

// This is horrible and isn't suitable for every case but works well enough for what it is used for
pub fn filter_columns<'a>(
    columns: &[(&'a str, Constraint, i16)],
    mut available_width: i16,
    spacing: i16,
) -> Vec<(&'a str, Constraint)> {
    let fill_count = columns
        .iter()
        .filter(|(_, constraint, _)| matches!(constraint, Constraint::Fill(_)))
        .count() as i16;

    available_width -= (columns.len() as i16 - 1) * spacing;
    let mut possible_spacing_save = fill_count * spacing;

    columns
        .iter()
        .filter(|(_, constraint, min_width)| match constraint {
            Constraint::Min(width) => (*min_width <= available_width + possible_spacing_save)
                .then(|| available_width -= *width as i16)
                .or_else(|| {
                    available_width += spacing;
                    None
                })
                .is_some(),
            _ => true,
        })
        .collect::<Vec<&(&'a str, Constraint, i16)>>()
        .into_iter()
        .filter(|(_, constraint, min_width)| match constraint {
            Constraint::Length(width) => (*min_width <= available_width + possible_spacing_save)
                .then(|| available_width -= *width as i16)
                .or_else(|| {
                    available_width += spacing;
                    None
                })
                .is_some(),
            _ => true,
        })
        .collect::<Vec<&(&'a str, Constraint, i16)>>()
        .into_iter()
        .filter(|(_, constraint, min_width)| match constraint {
            Constraint::Fill(v) => (*min_width
                <= available_width + possible_spacing_save - spacing)
                .then(|| {
                    possible_spacing_save -= spacing;
                    available_width -=
                        (available_width as f32 * *v as f32 / fill_count as f32).ceil() as i16
                })
                .or_else(|| {
                    available_width += spacing;
                    None
                })
                .is_some(),
            _ => true,
        })
        .map(|c| (c.0, c.1))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::filter_columns;
    use ratatui::layout::Constraint;

    #[test]
    fn filter_length_and_min_constraints() {
        let constraints = [
            ("a", Constraint::Length(5), 2),
            ("b", Constraint::Min(10), 0),
        ];

        assert_eq!(
            filter_columns(&constraints, 13, 0),
            vec![("a", Constraint::Length(5)), ("b", Constraint::Min(10))]
        );
        assert_eq!(
            filter_columns(&constraints, 11, 0),
            vec![("b", Constraint::Min(10))]
        );
    }

    #[test]
    fn filter_columns_with_fill() {
        let constraints = [
            ("a", Constraint::Length(5), 2),
            ("b", Constraint::Fill(1), 2),
            ("c", Constraint::Min(10), 0),
        ];

        assert_eq!(
            filter_columns(&constraints, 20, 0),
            vec![
                ("a", Constraint::Length(5)),
                ("b", Constraint::Fill(1)),
                ("c", Constraint::Min(10)),
            ]
        );
        assert_eq!(
            filter_columns(&constraints, 16, 0),
            vec![("a", Constraint::Length(5)), ("c", Constraint::Min(10)),]
        );
        assert_eq!(
            filter_columns(&constraints, 11, 0),
            vec![("c", Constraint::Min(10)),]
        );
    }

    #[test]
    fn filter_columns_with_spacing() {
        const SPACING: i16 = 2;

        let constraints = [
            ("a", Constraint::Length(45), 2),
            ("b", Constraint::Min(90), 0),
            ("c", Constraint::Fill(1), 5),
            ("d", Constraint::Fill(1), 11),
        ];

        let four = vec![
            ("a", Constraint::Length(45)),
            ("b", Constraint::Min(90)),
            ("c", Constraint::Fill(1)),
            ("d", Constraint::Fill(1)),
        ];
        assert_eq!(filter_columns(&constraints, 163, SPACING), four);

        let three = vec![
            ("a", Constraint::Length(45)),
            ("b", Constraint::Min(90)),
            ("c", Constraint::Fill(1)),
        ];
        assert_eq!(filter_columns(&constraints, 162, SPACING), three);
        assert_eq!(filter_columns(&constraints, 144, SPACING), three);

        let two = vec![("a", Constraint::Length(45)), ("b", Constraint::Min(90))];
        assert_eq!(filter_columns(&constraints, 143, SPACING), two);
        assert_eq!(filter_columns(&constraints, 94, SPACING), two);

        let one = vec![("b", Constraint::Min(90))];
        assert_eq!(filter_columns(&constraints, 93, SPACING), one);
    }
}
