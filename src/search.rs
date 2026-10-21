use crate::list::{ListItem, State, StatefulList};
use std::fmt::Display;

pub enum SearchUpdate {
    Build,
    Filter,
}

#[derive(Default, PartialEq, Eq, Debug, Clone, Copy)]
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

struct Match {
    idx: usize,
    text: String,
}

#[derive(Clone)]
pub struct LastSearch {
    pattern: String,
    direction: SearchDirection,
}

pub struct RecoveryItem {
    idx: usize,
    pub id: String,
}

impl RecoveryItem {
    pub fn new<T: ListItem, S: State>(list: &StatefulList<T, S>, idx: usize) -> Option<Self> {
        Some(Self {
            idx,
            id: list.items.get(idx)?.id().to_owned(),
        })
    }

    fn from_selected<T: ListItem, S: State>(list: &StatefulList<T, S>) -> Option<Self> {
        Self::new(list, list.state.selected()?)
    }
}

pub trait Searchable: Display + ListItem {}
impl<T: Display + ListItem> Searchable for T {}

pub trait SearchList {
    fn begin_search(&mut self, direction: SearchDirection);
    fn set_search_pattern(&mut self, pattern: &str, update: SearchUpdate) -> bool;
    fn repeat_last(&mut self, last: &LastSearch, opposite: bool) -> bool;
    fn abort_search(&mut self) -> bool;
    fn finish_search(&mut self) -> Option<LastSearch>;
    fn pattern(&self) -> Option<&str>;
    fn has_matches(&self) -> bool;
}

#[derive(Default)]
pub struct SearchSession {
    matches: Vec<Match>,
    pub pattern: String,
    pub direction: SearchDirection,
    pub recovery: Option<RecoveryItem>,
}

impl SearchSession {
    pub fn new<T: ListItem, S: State>(list: &StatefulList<T, S>) -> Self {
        Self {
            recovery: RecoveryItem::from_selected(list),
            ..Self::default()
        }
    }

    pub fn search<T: Searchable, S: State>(
        &mut self,
        list: &mut StatefulList<T, S>,
        pattern: &str,
        update: SearchUpdate,
    ) {
        self.pattern = pattern.to_lowercase();

        if pattern.is_empty() {
            self.matches.clear();
            self.recover_item(list);
            return;
        }

        if matches!(update, SearchUpdate::Filter) {
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

    pub fn build_matches<T: Display>(&mut self, items: &[T]) {
        self.matches = items
            .iter()
            .enumerate()
            .map(|(idx, item)| Match {
                idx,
                text: item.to_string().to_lowercase(),
            })
            .filter(|item| item.text.contains(&self.pattern))
            .collect();
    }

    fn filter_matches(&mut self) {
        self.matches
            .retain(|item| item.text.contains(&self.pattern));
    }

    fn indices(&self) -> Vec<usize> {
        self.matches.iter().map(|m| m.idx).collect()
    }

    pub const fn any_matches(&self) -> bool {
        !self.matches.is_empty()
    }

    pub fn into_last_search(self) -> LastSearch {
        LastSearch {
            pattern: self.pattern,
            direction: self.direction,
        }
    }

    pub fn recover_item<T, S: State>(&self, list: &mut StatefulList<T, S>) {
        if let Some(recovery) = &self.recovery {
            list.state.select(Some(recovery.idx));
        }
    }

    fn jump_to_match<T, S: State>(list: &mut StatefulList<T, S>, match_index: Option<usize>) {
        if match_index.is_some() {
            list.state.select(match_index);
        }
    }

    pub fn follow_direction<T, S: State>(&self, list: &mut StatefulList<T, S>) {
        match self.direction {
            SearchDirection::Forward => self.next_match(list),
            SearchDirection::Backward => self.prev_match(list),
        }
    }

    pub fn next_match<T, S: State>(&self, list: &mut StatefulList<T, S>) {
        let indices = self.indices();
        let match_index = self
            .recovery
            .as_ref()
            .map_or_else(
                || indices.first(),
                |recovery| {
                    indices
                        .iter()
                        .find(|index| **index > recovery.idx)
                        .or_else(|| indices.first())
                },
            )
            .copied();

        Self::jump_to_match(list, match_index);
    }

    pub fn prev_match<T, S: State>(&self, list: &mut StatefulList<T, S>) {
        let indices = self.indices();
        let match_index = self
            .recovery
            .as_ref()
            .map_or_else(
                || indices.last(),
                |recovery| {
                    indices
                        .iter()
                        .rev()
                        .find(|index| **index < recovery.idx)
                        .or_else(|| indices.last())
                },
            )
            .copied();

        Self::jump_to_match(list, match_index);
    }

    pub fn repeat_last<T: Searchable, S: State>(
        &mut self,
        list: &mut StatefulList<T, S>,
        last_search: &LastSearch,
        opposite_dir: bool,
    ) {
        let pattern = last_search.pattern.clone();
        self.direction = if opposite_dir {
            last_search.direction.reverse()
        } else {
            last_search.direction
        };
        self.search(list, &pattern, SearchUpdate::Build);
    }
}

#[cfg(test)]
mod tests {
    use super::{SearchDirection, SearchList, SearchUpdate};
    use crate::list::{ListItem, SearchableList, Selectable, SelectionList, SelectionMode};
    use ratatui::widgets::ListState;
    use std::fmt::{self, Display};

    struct Item {
        id: String,
        text: String,
    }

    impl Item {
        fn new(id: &str, text: &str) -> Self {
            Self {
                id: id.to_owned(),
                text: text.to_owned(),
            }
        }
    }

    impl ListItem for Item {
        fn id(&self) -> &str {
            &self.id
        }
    }

    impl Display for Item {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.text)
        }
    }

    type List = SearchableList<Item, ListState>;

    fn list(items: &[(&str, &str)]) -> List {
        SearchableList::with_items(items.iter().map(|(id, text)| Item::new(id, text)).collect())
    }

    fn select_by_id(list: &mut List, id: &str) {
        let index = list.find_by_id(id).unwrap();
        list.select_with_index(index);
    }

    fn selected_id(list: &List) -> Option<&str> {
        list.get_selected().map(ListItem::id)
    }

    fn move_to_front(list: &mut List, id: &str) {
        let index = list.find_by_id(id).unwrap();
        list.update_items(|items| {
            let item = items.remove(index);
            items.insert(0, item);
        });
    }

    fn move_to_back(list: &mut List, id: &str) {
        let index = list.find_by_id(id).unwrap();
        list.update_items(|items| {
            let item = items.remove(index);
            items.push(item);
        });
    }

    #[test]
    fn forward_search_uses_current_selection_and_order_after_reordering() {
        let mut list = list(&[
            ("attract", "Attract"),
            ("admit", "Admit"),
            ("assume", "Assume"),
            ("aspect", "Aspect"),
        ]);
        select_by_id(&mut list, "attract");

        list.begin_search(SearchDirection::Forward);
        list.set_search_pattern("a", SearchUpdate::Build);
        assert_eq!(selected_id(&list), Some("admit"));

        move_to_front(&mut list, "assume");
        list.set_search_pattern("as", SearchUpdate::Filter);

        assert_eq!(selected_id(&list), Some("aspect"));
    }

    #[test]
    fn backward_search_uses_current_selection_and_order_after_reordering() {
        let mut list = list(&[
            ("aspect", "Aspect"),
            ("assume", "Assume"),
            ("admit", "Admit"),
            ("attract", "Attract"),
        ]);
        select_by_id(&mut list, "attract");

        list.begin_search(SearchDirection::Backward);
        list.set_search_pattern("a", SearchUpdate::Build);
        assert_eq!(selected_id(&list), Some("admit"));

        move_to_back(&mut list, "assume");
        list.set_search_pattern("as", SearchUpdate::Filter);

        assert_eq!(selected_id(&list), Some("aspect"));
    }

    #[test]
    fn abort_restores_the_original_item_after_reordering() {
        let mut list = list(&[
            ("aardvark", "Aardvark"),
            ("current", "Current"),
            ("alpine", "Alpine"),
            ("incoming", "Incoming"),
        ]);
        select_by_id(&mut list, "current");

        list.begin_search(SearchDirection::Forward);
        list.set_search_pattern("al", SearchUpdate::Build);
        move_to_front(&mut list, "incoming");
        list.abort_search();

        assert_eq!(selected_id(&list), Some("current"));
    }

    #[test]
    fn refining_search_includes_items_added_during_the_search() {
        let mut list = list(&[("alpha", "Alpha"), ("current", "Current")]);
        select_by_id(&mut list, "current");

        list.begin_search(SearchDirection::Forward);
        list.set_search_pattern("z", SearchUpdate::Build);
        assert_eq!(selected_id(&list), Some("current"));

        list.update_items(|items| items.insert(0, Item::new("zebra", "Zebra")));
        list.set_search_pattern("ze", SearchUpdate::Filter);

        assert_eq!(selected_id(&list), Some("zebra"));
    }

    #[test]
    fn abort_stays_in_place_if_the_original_item_was_removed() {
        let mut list = list(&[("alpha", "Alpha"), ("beta", "Beta"), ("current", "Current")]);
        select_by_id(&mut list, "current");

        list.begin_search(SearchDirection::Forward);
        list.set_search_pattern("alpha", SearchUpdate::Build);
        list.update_items(|items| items.retain(|item| item.id() != "current"));
        list.abort_search();

        assert_eq!(selected_id(&list), Some("alpha"));
    }

    #[test]
    fn clearing_the_query_restores_selection_and_clears_search_results() {
        let mut list = list(&[("current", "Current"), ("alpha", "Alpha")]);
        list.begin_search(SearchDirection::Forward);
        list.set_search_pattern("al", SearchUpdate::Build);
        assert_eq!(selected_id(&list), Some("alpha"));

        assert!(list.set_search_pattern("", SearchUpdate::Build));

        assert_eq!(selected_id(&list), Some("current"));
        assert_eq!(list.pattern(), Some(""));
        assert!(!list.has_matches());
    }

    #[test]
    fn typing_a_new_query_after_clearing_keeps_the_original_recovery_item() {
        let mut list = list(&[("current", "Current"), ("alpha", "Alpha"), ("beta", "Beta")]);
        list.begin_search(SearchDirection::Forward);
        list.set_search_pattern("al", SearchUpdate::Build);
        list.set_search_pattern("", SearchUpdate::Build);

        list.set_search_pattern("be", SearchUpdate::Build);
        assert_eq!(selected_id(&list), Some("beta"));

        assert!(list.abort_search());
        assert_eq!(selected_id(&list), Some("current"));
    }

    #[test]
    fn editing_items_before_typing_does_not_move_selection() {
        let mut list = list(&[("alpha", "Alpha"), ("current", "Current"), ("beta", "Beta")]);
        select_by_id(&mut list, "current");
        list.begin_search(SearchDirection::Forward);

        let changed = list.update_item("alpha", |item| item.text = "Alpine".to_owned());

        assert_eq!(selected_id(&list), Some("current"));
        assert!(!changed);
        assert!(!list.has_matches());
    }

    #[test]
    fn shortening_the_query_rebuilds_matches_excluded_by_the_longer_query() {
        let mut list = list(&[
            ("current", "Current"),
            ("alpha", "Alpha"),
            ("alpine", "Alpine"),
        ]);
        list.begin_search(SearchDirection::Forward);
        list.set_search_pattern("alpi", SearchUpdate::Build);
        assert_eq!(selected_id(&list), Some("alpine"));

        assert!(list.set_search_pattern("al", SearchUpdate::Build));

        assert_eq!(selected_id(&list), Some("alpha"));
    }

    #[test]
    fn adding_the_first_match_updates_selection_without_more_input() {
        let mut list = list(&[("alpha", "Alpha"), ("current", "Current")]);
        select_by_id(&mut list, "current");
        list.begin_search(SearchDirection::Forward);
        list.set_search_pattern("ze", SearchUpdate::Build);
        assert!(!list.has_matches());

        let changed = list.update_items(|items| items.insert(0, Item::new("zebra", "Zebra")));

        assert_eq!(selected_id(&list), Some("zebra"));
        assert!(list.has_matches());
        assert!(changed);
    }

    #[test]
    fn abort_after_removing_all_items_keeps_selection_empty() {
        let mut list = list(&[("current", "Current"), ("alpha", "Alpha")]);
        list.begin_search(SearchDirection::Forward);
        list.set_search_pattern("al", SearchUpdate::Build);

        assert!(list.update_items(Vec::clear));
        assert_eq!(list.selected(), None);
        assert!(!list.has_matches());

        assert!(!list.abort_search());
        assert_eq!(list.selected(), None);
        assert_eq!(list.pattern(), None);
    }

    #[test]
    fn abort_does_not_restore_an_unrelated_item_at_the_old_recovery_index() {
        let mut list = list(&[("alpha", "Alpha"), ("current", "Current"), ("beta", "Beta")]);
        select_by_id(&mut list, "current");
        list.begin_search(SearchDirection::Forward);
        list.set_search_pattern("al", SearchUpdate::Build);

        list.update_items(|items| items.retain(|item| item.id() != "current"));
        assert_eq!(selected_id(&list), Some("alpha"));

        assert!(!list.abort_search());
        assert_eq!(selected_id(&list), Some("alpha"));
    }

    #[test]
    fn renaming_another_item_preserves_a_valid_result_and_refreshes_the_cache() {
        let mut list = list(&[
            ("current", "Current"),
            ("other", "Other"),
            ("alpha", "Alpha"),
        ]);
        list.begin_search(SearchDirection::Forward);
        list.set_search_pattern("al", SearchUpdate::Build);
        assert_eq!(selected_id(&list), Some("alpha"));

        let changed = list.update_item("other", |item| item.text = "Alpine".to_owned());

        assert_eq!(selected_id(&list), Some("alpha"));
        assert!(!changed);

        assert!(list.set_search_pattern("alpi", SearchUpdate::Filter));
        assert_eq!(selected_id(&list), Some("other"));
    }

    #[test]
    fn editing_the_selected_text_through_update_items_selects_a_matching_result() {
        let mut list = list(&[
            ("current", "Current"),
            ("alpha", "Alpha"),
            ("alpine", "Alpine"),
        ]);
        list.begin_search(SearchDirection::Forward);
        list.set_search_pattern("al", SearchUpdate::Build);

        let changed = list.update_items(|items| items[1].text = "Renamed".to_owned());

        assert_eq!(selected_id(&list), Some("alpine"));
        assert!(changed);
    }

    #[test]
    fn editing_the_last_matching_item_at_an_index_recovers_the_original_selection() {
        let mut list = list(&[("alpha", "Alpha"), ("current", "Current")]);
        select_by_id(&mut list, "current");
        list.begin_search(SearchDirection::Backward);
        list.set_search_pattern("al", SearchUpdate::Build);

        let index = list.find_by_id("alpha").unwrap();
        let changed = list.update_item_at(index, |item| item.text = "Renamed".to_owned());

        assert_eq!(selected_id(&list), Some("current"));
        assert!(!list.has_matches());
        assert!(changed);
    }

    #[test]
    fn bulk_text_edits_report_no_selection_change_when_the_same_item_still_matches() {
        let mut list = list(&[
            ("current", "Current"),
            ("alpha", "Alpha"),
            ("alpine", "Alpine"),
        ]);
        list.begin_search(SearchDirection::Forward);
        list.set_search_pattern("al", SearchUpdate::Build);

        let changed = list.update_each(|item| item.text = item.text.to_uppercase());

        assert_eq!(selected_id(&list), Some("alpha"));
        assert!(!changed);
        assert!(list.has_matches());
    }

    #[test]
    fn replacing_items_preserves_search_and_selection_by_identity() {
        let mut list = list(&[
            ("current", "Current"),
            ("alpha", "Alpha"),
            ("alpine", "Alpine"),
        ]);
        list.begin_search(SearchDirection::Forward);
        list.set_search_pattern("al", SearchUpdate::Build);

        let changed = list.replace_items(vec![
            Item::new("incoming", "Incoming"),
            Item::new("alpine", "Alpine"),
            Item::new("alpha", "Alpha"),
            Item::new("current", "Current"),
        ]);

        assert_eq!(selected_id(&list), Some("alpha"));
        assert!(!changed);
        assert_eq!(list.pattern(), Some("al"));

        assert!(list.set_search_pattern("alpi", SearchUpdate::Filter));
        assert_eq!(selected_id(&list), Some("alpine"));

        assert!(list.abort_search());
        assert_eq!(selected_id(&list), Some("current"));
    }

    #[test]
    fn a_different_matching_item_at_the_same_index_triggers_reevaluation() {
        let mut list = list(&[
            ("current", "Current"),
            ("other", "Other"),
            ("alpha", "Alpha"),
        ]);
        list.begin_search(SearchDirection::Forward);
        list.set_search_pattern("al", SearchUpdate::Build);
        assert_eq!(selected_id(&list), Some("alpha"));

        let changed = list.replace_items(vec![
            Item::new("current", "Current"),
            Item::new("alpine", "Alpine"),
            Item::new("algebra", "Algebra"),
        ]);

        assert_eq!(selected_id(&list), Some("alpine"));
        assert!(changed);
    }

    #[test]
    fn repeating_in_the_opposite_direction_does_not_change_the_saved_direction() {
        for (direction, expected) in [
            (SearchDirection::Forward, "gamma"),
            (SearchDirection::Backward, "alpha"),
        ] {
            let mut list = list(&[("alpha", "Alpha"), ("beta", "Beta"), ("gamma", "Gamma")]);
            select_by_id(&mut list, "beta");
            list.begin_search(direction);
            list.set_search_pattern("a", SearchUpdate::Build);
            let last = list.finish_search().unwrap();
            assert_eq!(selected_id(&list), Some(expected), "{direction:?}");

            assert!(list.repeat_last(&last, true));
            list.finish_search();
            assert_eq!(selected_id(&list), Some("beta"), "{direction:?}");

            assert!(list.repeat_last(&last, false));
            list.finish_search();
            assert_eq!(selected_id(&list), Some(expected), "{direction:?}");
        }
    }

    #[test]
    fn repeating_a_completed_search_uses_current_item_text() {
        let mut list = list(&[("current", "Current"), ("alpha", "Alpha"), ("beta", "Beta")]);
        list.begin_search(SearchDirection::Forward);
        list.set_search_pattern("al", SearchUpdate::Build);
        let last = list.finish_search().unwrap();
        assert_eq!(list.pattern(), None);

        list.replace_items(vec![
            Item::new("current", "Current"),
            Item::new("alpha", "Renamed"),
            Item::new("beta", "Alpine"),
        ]);

        assert!(list.repeat_last(&last, false));
        assert_eq!(selected_id(&list), Some("beta"));

        list.finish_search();
        assert!(!list.abort_search());
        assert_eq!(selected_id(&list), Some("beta"));
    }

    #[test]
    fn toggling_a_checkbox_updates_searchable_text_and_recovers_when_no_match_remains() {
        let mut list = SelectionList::new(
            vec![Item::new("current", "Current"), Item::new("alpha", "Alpha")],
            SelectionMode::Single,
            false,
        );
        list.update_item("alpha", |item| item.checked = true);
        list.begin_search(SearchDirection::Forward);
        list.set_search_pattern("[*]", SearchUpdate::Build);
        assert_eq!(list.get_selected().map(ListItem::id), Some("alpha"));

        list.toggle_selected();

        assert!(!list.has_matches());
        assert_eq!(list.get_selected().map(ListItem::id), Some("current"));
        assert!(list.get_checked_items().is_empty());
    }

    #[test]
    fn bulk_checkbox_edits_refresh_the_cache_for_further_typing() {
        let mut list = SelectionList::new(
            vec![Item::new("alpha", "Alpha"), Item::new("current", "Current")],
            SelectionMode::Multiple,
            false,
        );
        list.select_with_index(1);
        list.begin_search(SearchDirection::Forward);
        list.set_search_pattern("[*]", SearchUpdate::Build);
        assert!(!list.has_matches());

        list.check_all();
        list.set_search_pattern("[*] al", SearchUpdate::Filter);
        assert_eq!(list.get_selected().map(ListItem::id), Some("alpha"));

        list.uncheck_all();
        assert!(!list.has_matches());
        assert_eq!(list.get_selected().map(ListItem::id), Some("current"));
    }
}
