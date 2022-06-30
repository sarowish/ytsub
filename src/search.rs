use crate::app::{State, StatefulList};
use std::fmt::Display;

#[derive(Default)]
pub enum SearchState {
    #[default]
    NotSearching,
    PoppedKey,
    PushedKey,
}

#[derive(Default, PartialEq, Debug, Clone)]
pub enum SearchDirection {
    #[default]
    Forward,
    Backward,
}

impl SearchDirection {
    fn reverse(&self) -> SearchDirection {
        match self {
            SearchDirection::Forward => SearchDirection::Backward,
            SearchDirection::Backward => SearchDirection::Forward,
        }
    }
}

type Match = (usize, String);
type LastSearch = (String, SearchDirection);

#[derive(Default)]
pub struct Search {
    matches: Vec<Match>,
    pub pattern: String,
    pub state: SearchState,
    pub direction: SearchDirection,
    pub recovery_index: Option<usize>,
    last_search: Option<LastSearch>,
}

impl Search {
    pub fn search<T: Display, S: State>(&mut self, list: &mut StatefulList<T, S>, pattern: &str) {
        if pattern.is_empty() {
            self.recover_item(list);
            return;
        }
        self.pattern = pattern.to_lowercase();
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
                    .filter(|(_, item)| item.contains(&self.pattern))
                    .collect();
            }
            SearchState::PushedKey => {
                self.matches = self
                    .matches
                    .drain(..)
                    .filter(|(_, text)| text.contains(&self.pattern))
                    .collect();
            }
        }
        if self.any_matches() {
            match self.direction {
                SearchDirection::Forward => self.next_match(list),
                SearchDirection::Backward => self.prev_match(list),
            }
        } else {
            self.recover_item(list);
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
        self.matches.clear();

        let pattern = std::mem::take(&mut self.pattern);

        if !abort {
            self.last_search = Some((pattern, self.direction.clone()))
        }
    }

    pub fn recover_item<T, S: State>(&mut self, list: &mut StatefulList<T, S>) {
        if self.recovery_index.is_some() {
            list.state.select(self.recovery_index);
        }
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
        let match_index = if let Some(recovery_index) = self.recovery_index {
            indices
                .iter()
                .find(|index| **index > recovery_index)
                .or_else(|| indices.first())
        } else {
            indices.first()
        }
        .copied();
        self.jump_to_match(list, match_index);
    }

    pub fn prev_match<T, S: State>(&mut self, list: &mut StatefulList<T, S>) {
        let indices = self.indices();
        let match_index = if let Some(recovery_index) = self.recovery_index {
            indices
                .iter()
                .rev()
                .find(|index| **index < recovery_index)
                .or_else(|| indices.last())
        } else {
            indices.last()
        }
        .copied();
        self.jump_to_match(list, match_index);
    }

    pub fn repeat_last<T: Display, S: State>(
        &mut self,
        list: &mut StatefulList<T, S>,
        opposite_dir: bool,
    ) {
        if let Some((pattern, direction)) = &self.last_search {
            let pattern = pattern.clone();
            self.direction = if opposite_dir {
                direction.reverse()
            } else {
                direction.clone()
            };
            self.search(list, &pattern);
        }
    }
}
