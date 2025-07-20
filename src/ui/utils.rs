use crate::{
    THEME,
    app::{State, StatefulList},
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    text::Span,
    widgets::BorderType,
};
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

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Column<'a> {
    pub header: &'a str,
    pub constraint: Constraint,
    min_width: u16,
}

impl<'a> Column<'a> {
    pub fn new(text: &'a str, constraint: Constraint, min_width: u16) -> Self {
        Self {
            header: text,
            constraint,
            min_width,
        }
    }
}

pub fn filter_columns<'a>(
    columns: &'a [Column],
    available_width: u16,
    spacing: u16,
) -> Vec<Column<'a>> {
    let mut columns = Vec::from(columns);
    let area = Rect::new(0, 0, available_width, 0);

    while let Some(idx) = Layout::new(Direction::Horizontal, columns.iter().map(|c| c.constraint))
        .spacing(spacing)
        .split(area)
        .iter()
        .zip(columns.iter())
        .rev()
        .position(|(chunk, column)| chunk.width < column.min_width)
    {
        columns.remove(columns.len() - 1 - idx);
    }

    columns
}

#[cfg(test)]
mod tests {
    use super::filter_columns;
    use crate::ui::utils::Column;
    use ratatui::layout::Constraint;

    #[test]
    fn filter_length_and_min_constraints() {
        let a = Column::new("a", Constraint::Length(5), 2);
        let b = Column::new("b", Constraint::Min(10), 0);
        let constraints = [a, b];

        assert_eq!(filter_columns(&constraints, 13, 0), vec![a, b]);
        assert_eq!(filter_columns(&constraints, 11, 0), vec![b]);
    }

    #[test]
    fn filter_columns_with_fill() {
        let a = Column::new("a", Constraint::Length(5), 2);
        let b = Column::new("b", Constraint::Fill(1), 2);
        let c = Column::new("c", Constraint::Min(10), 0);
        let constraints = [a, b, c];

        assert_eq!(filter_columns(&constraints, 20, 0), vec![a, b, c]);
        assert_eq!(filter_columns(&constraints, 16, 0), vec![a, c]);
        assert_eq!(filter_columns(&constraints, 11, 0), vec![c]);
    }

    #[test]
    fn filter_columns_with_spacing() {
        const SPACING: u16 = 2;

        let a = Column::new("a", Constraint::Length(45), 2);
        let b = Column::new("b", Constraint::Min(90), 0);
        let c = Column::new("c", Constraint::Fill(1), 5);
        let d = Column::new("d", Constraint::Fill(1), 11);
        let constraints = [a, b, c, d];

        assert_eq!(filter_columns(&constraints, 163, SPACING), constraints);

        let three = vec![a, b, c];
        assert_eq!(filter_columns(&constraints, 162, SPACING), three);
        assert_eq!(filter_columns(&constraints, 144, SPACING), three);

        let two = vec![a, b];
        assert_eq!(filter_columns(&constraints, 143, SPACING), two);
        assert_eq!(filter_columns(&constraints, 94, SPACING), two);

        let one = vec![b];
        assert_eq!(filter_columns(&constraints, 93, SPACING), one);
    }
}
