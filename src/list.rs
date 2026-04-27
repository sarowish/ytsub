use ratatui::widgets::{ListState, TableState};
use std::{
    fmt::Display,
    ops::{Deref, DerefMut},
};

pub trait ListItem {
    fn id(&self) -> &str;
}

impl ListItem for String {
    fn id(&self) -> &str {
        self
    }
}

pub trait State {
    fn select(&mut self, index: Option<usize>);
    fn selected(&self) -> Option<usize>;
    fn selected_mut(&mut self) -> &mut Option<usize>;
    fn offset(&self) -> usize;
    fn offset_mut(&mut self) -> &mut usize;
}

impl State for ListState {
    fn select(&mut self, index: Option<usize>) {
        self.select(index);
    }

    fn selected(&self) -> Option<usize> {
        self.selected()
    }

    fn selected_mut(&mut self) -> &mut Option<usize> {
        self.selected_mut()
    }

    fn offset(&self) -> usize {
        self.offset()
    }

    fn offset_mut(&mut self) -> &mut usize {
        self.offset_mut()
    }
}

impl State for TableState {
    fn select(&mut self, index: Option<usize>) {
        self.select(index);
    }

    fn selected(&self) -> Option<usize> {
        self.selected()
    }

    fn selected_mut(&mut self) -> &mut Option<usize> {
        self.selected_mut()
    }

    fn offset(&self) -> usize {
        self.offset()
    }

    fn offset_mut(&mut self) -> &mut usize {
        self.offset_mut()
    }
}

pub trait Scrollable {
    fn len(&self) -> usize;
    fn offset(&self) -> usize;
    fn offset_mut(&mut self) -> &mut usize;
    fn visible_lines(&self) -> usize;

    fn scroll_top(&mut self) {
        *self.offset_mut() = 0;
    }

    fn scroll_bottom(&mut self) {
        *self.offset_mut() = self.len().saturating_sub(self.visible_lines());
    }

    fn scroll_up(&mut self, by: usize) {
        let len = self.len();

        if len == 0 {
            return;
        }

        let offset = self.offset_mut();
        *offset = offset.saturating_sub(by);
    }

    fn scroll_down(&mut self, by: usize) {
        let len = self.len();

        if len == 0 {
            return;
        }

        let max_offset = len.saturating_sub(self.visible_lines());
        let offset = self.offset_mut();
        *offset = (*offset + by).min(max_offset);
    }

    fn scroll_up_half(&mut self) {
        let by = self.visible_lines() / 2;
        self.scroll_up(by);
    }

    fn scroll_down_half(&mut self) {
        let by = self.visible_lines() / 2;
        self.scroll_down(by);
    }

    fn scroll_up_full(&mut self) {
        let by = self.visible_lines();
        self.scroll_up(by);
    }

    fn scroll_down_full(&mut self) {
        let by = self.visible_lines();
        self.scroll_down(by);
    }
}

pub trait Selectable: Scrollable {
    fn selected(&self) -> Option<usize>;
    fn selected_mut(&mut self) -> &mut Option<usize>;

    fn select_with_index(&mut self, index: usize) {
        let is_empty = self.len() == 0;
        *self.selected_mut() = if is_empty { None } else { Some(index) };
    }

    fn next(&mut self) {
        let i = self
            .selected()
            .map_or(0, |i| if i >= self.len() - 1 { 0 } else { i + 1 });

        self.select_with_index(i);
    }

    fn previous(&mut self) {
        let i = self
            .selected()
            .map_or(0, |i| if i == 0 { self.len() - 1 } else { i - 1 });

        self.select_with_index(i);
    }

    fn select_first(&mut self) {
        self.select_with_index(0);
    }

    fn select_last(&mut self) {
        self.select_with_index(self.len().saturating_sub(1));
    }

    fn move_selection_up(&mut self, by: usize) {
        self.scroll_up(by);

        let Some(selected) = self.selected_mut() else {
            return;
        };

        *selected = selected.saturating_sub(by);
    }

    fn move_selection_down(&mut self, by: usize) {
        self.scroll_down(by);

        let len = self.len();

        let Some(selected) = self.selected_mut() else {
            return;
        };

        *selected = (*selected + by).min(len - 1);
    }

    fn page_up(&mut self) {
        let by = self.visible_lines();
        self.move_selection_up(by);
    }

    fn page_down(&mut self) {
        let by = self.visible_lines();
        self.move_selection_down(by);
    }

    fn half_page_up(&mut self) {
        let by = self.visible_lines() / 2;
        self.move_selection_up(by);
    }

    fn half_page_down(&mut self) {
        let by = self.visible_lines() / 2;
        self.move_selection_down(by);
    }
}

pub struct StatefulList<T, S: State> {
    pub state: S,
    pub items: Vec<T>,
    pub visible_lines: u16,
}

impl<T, S: State + Default> Default for StatefulList<T, S> {
    fn default() -> Self {
        Self {
            state: Default::default(),
            items: Vec::default(),
            visible_lines: 0,
        }
    }
}

impl<T, S: State + Default> StatefulList<T, S> {
    pub fn with_items(items: Vec<T>) -> Self {
        let mut stateful_list = Self {
            items,
            ..Default::default()
        };

        stateful_list.select_first();

        stateful_list
    }
}

impl<T, S: State> StatefulList<T, S> {
    pub fn reset_state(&mut self) {
        self.state
            .select(if self.items.is_empty() { None } else { Some(0) });
    }

    pub fn get_selected(&self) -> Option<&T> {
        self.state.selected().and_then(|idx| self.items.get(idx))
    }

    pub fn get_mut_selected(&mut self) -> Option<&mut T> {
        self.state
            .selected()
            .and_then(|idx| self.items.get_mut(idx))
    }

    pub fn check_bounds(&mut self) {
        if let Some(idx) = self.state.selected() {
            if self.items.is_empty() {
                self.state.select(None);
            } else if idx >= self.items.len() {
                self.select_last();
            }
        }
    }
}

impl<T, S: State> Scrollable for StatefulList<T, S> {
    fn len(&self) -> usize {
        self.items.len()
    }

    fn offset(&self) -> usize {
        self.state.offset()
    }

    fn offset_mut(&mut self) -> &mut usize {
        self.state.offset_mut()
    }

    fn visible_lines(&self) -> usize {
        self.visible_lines as usize
    }
}

impl<T, S: State> Selectable for StatefulList<T, S> {
    fn selected(&self) -> Option<usize> {
        self.state.selected()
    }

    fn selected_mut(&mut self) -> &mut Option<usize> {
        self.state.selected_mut()
    }
}

impl<T: ListItem, S: State> StatefulList<T, S> {
    pub fn find_by_id(&self, id: &str) -> Option<usize> {
        self.items.iter().position(|item| item.id() == id)
    }

    pub fn get_mut_by_id(&mut self, id: &str) -> Option<&mut T> {
        self.find_by_id(id).map(|index| &mut self.items[index])
    }
}

impl<T, S: State + Default> From<Vec<T>> for StatefulList<T, S> {
    fn from(v: Vec<T>) -> Self {
        Self::with_items(v)
    }
}

pub struct SelectionItem<T> {
    pub selected: bool,
    pub item: T,
}

impl<T> SelectionItem<T> {
    pub const fn new(item: T) -> Self {
        Self {
            selected: false,
            item,
        }
    }

    pub const fn toggle(&mut self) {
        self.selected = !self.selected;
    }
}

impl<T: Display> Display for SelectionItem<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] {}",
            if self.selected { "*" } else { " " },
            self.item
        )
    }
}

impl<T: ListItem> ListItem for SelectionItem<T> {
    fn id(&self) -> &str {
        self.item.id()
    }
}

impl<T> Deref for SelectionItem<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.item
    }
}

impl<T> DerefMut for SelectionItem<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.item
    }
}

pub struct SelectionList<T: ListItem>(StatefulList<SelectionItem<T>, ListState>);

impl<T: ListItem> SelectionList<T> {
    pub fn new(items: Vec<T>) -> Self {
        let items = items.into_iter().map(SelectionItem::new).collect();

        Self(StatefulList::with_items(items))
    }

    pub fn toggle_selected(&mut self) {
        if let Some(item) = self.get_mut_selected() {
            item.toggle();
        }
    }

    pub fn select(&mut self) {
        if let Some(item) = self.items.iter_mut().find(|item| item.selected) {
            item.selected = false;
        }

        self.toggle_selected();
    }

    pub fn select_all(&mut self) {
        self.items.iter_mut().for_each(|item| item.selected = true);
    }

    pub fn deselect_all(&mut self) {
        self.items.iter_mut().for_each(|item| item.selected = false);
    }

    pub fn get_selected_items(&self) -> Vec<&T> {
        self.items
            .iter()
            .filter(|item| item.selected)
            .map(|item| &item.item)
            .collect()
    }

    pub fn get_selected_item(&self) -> &T {
        self.items.iter().find(|item| item.selected).unwrap()
    }
}

impl<T: ListItem> Deref for SelectionList<T> {
    type Target = StatefulList<SelectionItem<T>, ListState>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: ListItem> DerefMut for SelectionList<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: ListItem> Default for SelectionList<T> {
    fn default() -> Self {
        Self(StatefulList::with_items(Vec::default()))
    }
}
