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
}

impl State for ListState {
    fn select(&mut self, index: Option<usize>) {
        self.select(index);
    }

    fn selected(&self) -> Option<usize> {
        self.selected()
    }
}

impl State for TableState {
    fn select(&mut self, index: Option<usize>) {
        self.select(index);
    }

    fn selected(&self) -> Option<usize> {
        self.selected()
    }
}

pub struct StatefulList<T, S: State> {
    pub state: S,
    pub items: Vec<T>,
}

impl<T, S: State + Default> Default for StatefulList<T, S> {
    fn default() -> Self {
        Self {
            state: Default::default(),
            items: Vec::default(),
        }
    }
}

impl<T, S: State + Default> StatefulList<T, S> {
    pub fn with_items(items: Vec<T>) -> StatefulList<T, S> {
        let mut stateful_list = StatefulList {
            state: Default::default(),
            items,
        };

        stateful_list.select_first();

        stateful_list
    }
}

impl<T, S: State> StatefulList<T, S> {
    pub fn select_with_index(&mut self, index: usize) {
        self.state.select(if self.items.is_empty() {
            None
        } else {
            Some(index)
        });
    }

    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.select_with_index(i);
    }

    pub fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.select_with_index(i);
    }

    pub fn select_first(&mut self) {
        self.select_with_index(0);
    }

    pub fn select_last(&mut self) {
        self.select_with_index(self.items.len().saturating_sub(1));
    }

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
        StatefulList::with_items(v)
    }
}

pub struct SelectionItem<T> {
    pub selected: bool,
    pub item: T,
}

impl<T> SelectionItem<T> {
    pub fn new(item: T) -> Self {
        Self {
            selected: false,
            item,
        }
    }

    pub fn toggle(&mut self) {
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
