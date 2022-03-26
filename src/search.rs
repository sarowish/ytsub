use crate::app::{State, StatefulList};
use std::fmt::Display;

pub enum SearchState {
    NotSearching,
    PoppedKey,
    PushedKey,
}

impl Default for SearchState {
    fn default() -> Self {
        SearchState::NotSearching
    }
}

pub enum SearchDirection {
    Forward,
    Backward,
}

impl Default for SearchDirection {
    fn default() -> Self {
        SearchDirection::Forward
    }
}

type Match = (usize, String);

#[derive(Default)]
pub struct Search {
    pub matches: Vec<Match>,
    pub state: SearchState,
    pub direction: SearchDirection,
    recovery_index: Option<usize>,
    pub previous_matches: Vec<Match>,
}

impl Search {
    pub fn search<T: Display, S: State>(&mut self, list: &mut StatefulList<T, S>, pattern: &str) {
        let pattern = pattern.to_lowercase();
        match self.state {
            SearchState::NotSearching | SearchState::PoppedKey => {
                if let SearchState::NotSearching = self.state {
                    self.recovery_index = list.state.selected();
                }
                self.matches = list
                    .items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| (i, item.to_string().to_lowercase()))
                    .filter(|(_, item)| item.contains(&pattern))
                    .collect();
                self.state = SearchState::PushedKey;
            }
            SearchState::PushedKey => {
                self.matches = self
                    .matches
                    .drain(..)
                    .filter(|(_, text)| text.contains(&pattern))
                    .collect();
            }
        }
        match list.state.selected() {
            Some(current_index) if self.indices().contains(&current_index) => (),
            _ => {
                if self.any_matches() {
                    match self.direction {
                        SearchDirection::Forward => self.next_match(list),
                        SearchDirection::Backward => self.prev_match(list),
                    }
                } else {
                    self.recover_item(list);
                }
            }
        }
    }

    fn indices(&self) -> Vec<usize> {
        self.matches.iter().map(|m| m.0).collect()
    }

    pub fn any_matches(&self) -> bool {
        !self.matches.is_empty()
    }

    pub fn complete_search(&mut self, abort: bool) {
        self.state = SearchState::NotSearching;
        self.recovery_index = None;
        if self.matches.is_empty() || abort {
            self.matches = self.previous_matches.drain(..).collect();
        }
    }

    pub fn recover_item<T, S: State>(&mut self, list: &mut StatefulList<T, S>) {
        list.state.select(if self.recovery_index.is_some() {
            self.recovery_index
        } else {
            list.state.selected()
        });
    }

    fn jump_to_match<T, S: State>(
        &mut self,
        list: &mut StatefulList<T, S>,
        match_index: Option<usize>,
    ) {
        if match_index.is_some() {
            list.state.select(match_index);
        }
    }

    pub fn next_match<T, S: State>(&mut self, list: &mut StatefulList<T, S>) {
        let indices = self.indices();
        let match_index = if let Some(current_index) = list.state.selected() {
            indices
                .iter()
                .find(|index| **index > current_index)
                .or_else(|| indices.first())
        } else {
            indices.first()
        }
        .copied();
        self.jump_to_match(list, match_index);
    }

    pub fn prev_match<T, S: State>(&mut self, list: &mut StatefulList<T, S>) {
        let indices = self.indices();
        let match_index = if let Some(current_index) = list.state.selected() {
            indices
                .iter()
                .rev()
                .find(|index| **index < current_index)
                .or_else(|| indices.last())
        } else {
            indices.last()
        }
        .copied();
        self.jump_to_match(list, match_index);
    }
}
