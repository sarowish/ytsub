use crate::list::{State, StatefulList};
use std::fmt::Display;

pub enum SearchUpdate {
    Build,
    Filter,
}

#[derive(Default, PartialEq, Eq, Debug, Clone)]
pub enum SearchDirection {
    #[default]
    Forward,
    Backward,
}

impl SearchDirection {
    const fn reverse(&self) -> Self {
        match self {
            Self::Forward => Self::Backward,
            Self::Backward => Self::Forward,
        }
    }
}

type Match = (usize, String);
type LastSearch = (String, SearchDirection);

#[derive(Default)]
pub struct Search {
    matches: Vec<Match>,
    matches_valid: bool,
    pub pattern: String,
    pub direction: SearchDirection,
    pub recovery_index: Option<usize>,
    last_search: Option<LastSearch>,
}

impl Search {
    pub fn search<T: Display, S: State>(
        &mut self,
        list: &mut StatefulList<T, S>,
        pattern: &str,
        update: SearchUpdate,
    ) {
        if pattern.is_empty() {
            self.matches_valid = false;
            self.recover_item(list);
            return;
        }

        if self.pattern.is_empty() {
            self.recovery_index = list.state.selected();
        }

        self.pattern = pattern.to_lowercase();

        if matches!(update, SearchUpdate::Filter) && self.matches_valid {
            self.filter_matches();
        } else {
            self.build_matches(&list.items);
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

    fn build_matches<T: Display>(&mut self, items: &[T]) {
        self.matches = items
            .iter()
            .enumerate()
            .map(|(i, item)| (i, item.to_string().to_lowercase()))
            .filter(|(_, item)| item.contains(&self.pattern))
            .collect();
        self.matches_valid = true;
    }

    fn filter_matches(&mut self) {
        self.matches
            .retain(|(_, text)| text.contains(&self.pattern));
    }

    fn indices(&self) -> Vec<usize> {
        self.matches.iter().map(|m| m.0).collect()
    }

    pub const fn any_matches(&self) -> bool {
        !self.matches.is_empty()
    }

    pub fn complete_search(&mut self, abort: bool) {
        self.matches_valid = false;
        self.recovery_index = None;
        self.matches.clear();

        let pattern = std::mem::take(&mut self.pattern);

        if !abort {
            self.last_search = Some((pattern, self.direction.clone()));
        }
    }

    pub fn recover_item<T, S: State>(&self, list: &mut StatefulList<T, S>) {
        if self.recovery_index.is_some() {
            list.state.select(self.recovery_index);
        }
    }

    fn jump_to_match<T, S: State>(list: &mut StatefulList<T, S>, match_index: Option<usize>) {
        if match_index.is_some() {
            list.state.select(match_index);
        }
    }

    pub fn next_match<T, S: State>(&self, list: &mut StatefulList<T, S>) {
        let indices = self.indices();
        let match_index = self
            .recovery_index
            .map_or_else(
                || indices.first(),
                |recovery_index| {
                    indices
                        .iter()
                        .find(|index| **index > recovery_index)
                        .or_else(|| indices.first())
                },
            )
            .copied();

        Self::jump_to_match(list, match_index);
    }

    pub fn prev_match<T, S: State>(&self, list: &mut StatefulList<T, S>) {
        let indices = self.indices();
        let match_index = self
            .recovery_index
            .map_or_else(
                || indices.last(),
                |recovery_index| {
                    indices
                        .iter()
                        .rev()
                        .find(|index| **index < recovery_index)
                        .or_else(|| indices.last())
                },
            )
            .copied();

        Self::jump_to_match(list, match_index);
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
            self.search(list, &pattern, SearchUpdate::Build);
        }
    }
}
