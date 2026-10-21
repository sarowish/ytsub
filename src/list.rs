use ratatui::widgets::{ListState, TableState};
use std::{
    fmt::Display,
    ops::{Deref, DerefMut},
};

use crate::search::{
    LastSearch, RecoveryItem, SearchDirection, SearchList, SearchSession, SearchUpdate, Searchable,
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
    fn set_selected_index(&mut self, index: Option<usize>);

    fn select_with_index(&mut self, index: usize) {
        self.set_selected_index(Some(index));
    }

    fn unselect(&mut self) {
        self.set_selected_index(None);
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

    fn check_bounds(&mut self) {
        if let Some(idx) = self.selected() {
            if self.len() == 0 {
                self.unselect();
            } else if idx >= self.len() {
                self.select_last();
            }
        }
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
    pub fn get_selected(&self) -> Option<&T> {
        self.state.selected().and_then(|idx| self.items.get(idx))
    }

    pub fn get_mut_selected(&mut self) -> Option<&mut T> {
        self.state
            .selected()
            .and_then(|idx| self.items.get_mut(idx))
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

    fn set_selected_index(&mut self, index: Option<usize>) {
        self.state.select(index.filter(|_| self.len() != 0));
    }
}

impl<T: ListItem, S: State> StatefulList<T, S> {
    pub fn find_by_id(&self, id: &str) -> Option<usize> {
        self.items.iter().position(|item| item.id() == id)
    }
}

impl<T, S: State + Default> From<Vec<T>> for StatefulList<T, S> {
    fn from(v: Vec<T>) -> Self {
        Self::with_items(v)
    }
}

pub struct SearchableList<T, S: State> {
    list: StatefulList<T, S>,
    search_session: Option<SearchSession>,
}

impl<T, S: State + Default> Default for SearchableList<T, S> {
    fn default() -> Self {
        Self {
            list: StatefulList::default(),
            search_session: None,
        }
    }
}

impl<T, S: State> SearchableList<T, S> {
    pub fn get_selected(&self) -> Option<&T> {
        self.list.get_selected()
    }

    pub fn render_parts(&mut self, visible_lines: u16) -> (&[T], &mut S) {
        self.list.visible_lines = visible_lines;

        (&self.list.items, &mut self.list.state)
    }
}

impl<T: ListItem, S: State> SearchableList<T, S> {
    pub fn find_by_id(&self, id: &str) -> Option<usize> {
        self.list.find_by_id(id)
    }
}

impl<T: Searchable, S: State + Default> SearchableList<T, S> {
    pub fn with_items(items: Vec<T>) -> Self {
        Self {
            list: StatefulList::with_items(items),
            search_session: None,
        }
    }
}

impl<T: Searchable, S: State> SearchableList<T, S> {
    pub const fn list(&self) -> &StatefulList<T, S> {
        &self.list
    }

    pub fn items(&self) -> &[T] {
        &self.list.items
    }

    fn selected_id(&self) -> Option<&str> {
        self.list.get_selected().map(ListItem::id)
    }

    pub fn update_item(&mut self, id: &str, edit: impl FnOnce(&mut T)) -> bool {
        self.list
            .find_by_id(id)
            .is_some_and(|index| self.update_item_at(index, edit))
    }

    pub fn update_item_at(&mut self, index: usize, edit: impl FnOnce(&mut T)) -> bool {
        let selected_id = self.selected_id().map(str::to_owned);
        let compare_text = self.pattern().is_some_and(|pattern| !pattern.is_empty());

        let Some(item) = self.list.items.get_mut(index) else {
            return false;
        };

        let before = compare_text.then(|| item.to_string());

        edit(item);

        let content_changed = before.is_some_and(|before| before != item.to_string());
        let selected_changed = selected_id.as_deref() != self.selected_id();

        if content_changed || selected_changed {
            self.rebuild_search(selected_changed);
        }

        selected_id.as_deref() != self.selected_id()
    }

    pub fn update_each(&mut self, mut edit: impl FnMut(&mut T)) -> bool {
        let mut changed = false;
        let selected_id = self.selected_id().map(str::to_owned);
        let compare_text = self.pattern().is_some_and(|pattern| !pattern.is_empty());

        for item in &mut self.list.items {
            let before = (compare_text && !changed).then(|| item.to_string());

            edit(item);

            if before.is_some_and(|before| before != item.to_string()) {
                changed = true;
            }
        }

        let selected_changed = selected_id.as_deref() != self.selected_id();

        if changed || selected_changed {
            self.rebuild_search(selected_changed);
        }

        selected_id.as_deref() != self.selected_id()
    }

    pub fn update_items(&mut self, edit: impl FnOnce(&mut Vec<T>)) -> bool {
        let recovery_id = self
            .search_session
            .as_ref()
            .and_then(|session| session.recovery.as_ref())
            .map(|recovery| recovery.id.clone());
        let selected_id = self.selected_id().map(ToOwned::to_owned);
        let was_empty = self.list.items.is_empty();

        edit(&mut self.list.items);

        self.restore_selections(selected_id.as_deref(), recovery_id.as_deref());
        self.list.check_bounds();

        if was_empty && self.list.state.selected().is_none() {
            self.select_first();
        }

        let selected_changed = selected_id.as_deref() != self.selected_id();
        self.rebuild_search(selected_changed);

        selected_id.as_deref() != self.selected_id()
    }

    pub fn replace_items(&mut self, items: Vec<T>) -> bool {
        self.update_items(|current| *current = items)
    }

    fn restore_selections(&mut self, selected_id: Option<&str>, recovery_id: Option<&str>) {
        if let Some(id) = selected_id
            && let Some(index) = self.list.find_by_id(id)
        {
            self.list.select_with_index(index);
        }

        let recovery = recovery_id
            .and_then(|id| self.list.find_by_id(id))
            .and_then(|index| RecoveryItem::new(self.list(), index));

        if let Some(session) = &mut self.search_session {
            session.recovery = recovery;
        }
    }

    fn rebuild_search(&mut self, selected_changed: bool) {
        if self.pattern().is_some_and(|pattern| pattern.is_empty()) {
            return;
        }

        if let Some(search) = &mut self.search_session {
            search.build_matches(&self.list.items);

            let selected_matches = self
                .list
                .get_selected()
                .is_some_and(|item| item.to_string().to_lowercase().contains(&search.pattern));

            if selected_changed || !selected_matches {
                search.recover_item(&mut self.list);
                search.follow_direction(&mut self.list);
            }
        }
    }

    fn track_selection_change(&mut self, action: impl FnOnce(&mut Self)) -> bool {
        let selected_id = self.selected_id().map(ToOwned::to_owned);

        action(self);

        selected_id.as_deref() != self.selected_id()
    }
}

impl<T: Searchable, S: State + Default> SearchableList<T, S> {
    pub fn reset_with_items(&mut self, items: Vec<T>) {
        *self = Self::with_items(items);
    }
}

impl<T: Searchable, S: State> SearchList for SearchableList<T, S> {
    fn begin_search(&mut self, direction: SearchDirection) {
        let mut search_session = SearchSession::new(self.list());
        search_session.direction = direction;

        self.search_session = Some(search_session);
    }

    fn set_search_pattern(&mut self, pattern: &str, update: SearchUpdate) -> bool {
        self.track_selection_change(|this| {
            if let Some(session) = &mut this.search_session {
                session.search(&mut this.list, pattern, update);
            }
        })
    }

    fn repeat_last(&mut self, last: &LastSearch, opposite: bool) -> bool {
        self.track_selection_change(|this| {
            let mut session = SearchSession::new(this.list());
            session.repeat_last(&mut this.list, last, opposite);
            this.search_session = Some(session);
        })
    }

    fn abort_search(&mut self) -> bool {
        self.track_selection_change(|this| {
            if let Some(session) = &mut this.search_session.take() {
                session.recover_item(&mut this.list);
            }
        })
    }

    fn finish_search(&mut self) -> Option<LastSearch> {
        self.search_session
            .take()
            .map(SearchSession::into_last_search)
    }

    fn pattern(&self) -> Option<&str> {
        self.search_session
            .as_ref()
            .map(|session| session.pattern.as_str())
    }

    fn has_matches(&self) -> bool {
        self.search_session
            .as_ref()
            .is_some_and(SearchSession::any_matches)
    }
}

impl<T, S: State> Scrollable for SearchableList<T, S> {
    fn len(&self) -> usize {
        self.list.len()
    }

    fn offset(&self) -> usize {
        self.list.state.offset()
    }

    fn offset_mut(&mut self) -> &mut usize {
        self.list.state.offset_mut()
    }

    fn visible_lines(&self) -> usize {
        self.list.visible_lines as usize
    }
}

impl<T, S: State> Selectable for SearchableList<T, S> {
    fn selected(&self) -> Option<usize> {
        self.list.selected()
    }

    fn selected_mut(&mut self) -> &mut Option<usize> {
        self.list.selected_mut()
    }

    fn set_selected_index(&mut self, index: Option<usize>) {
        self.list.set_selected_index(index);
    }
}

pub struct SelectionItem<T> {
    pub checked: bool,
    pub item: T,
}

impl<T> SelectionItem<T> {
    pub const fn new(item: T) -> Self {
        Self {
            checked: false,
            item,
        }
    }
}

impl<T: Display> Display for SelectionItem<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] {}",
            if self.checked { "*" } else { " " },
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

#[derive(PartialEq)]
pub enum SelectionMode {
    Single,
    Multiple,
}

pub struct SelectionList<T: ListItem> {
    inner: SearchableList<SelectionItem<T>, ListState>,
    mode: SelectionMode,
    selection_required: bool,
}

impl<T: ListItem> SelectionList<T> {
    pub fn get_selected(&self) -> Option<&SelectionItem<T>> {
        self.inner.get_selected()
    }

    pub fn find_by_id(&self, id: &str) -> Option<usize> {
        self.inner.find_by_id(id)
    }
}

impl<T: Searchable> SelectionList<T> {
    pub fn new(items: Vec<T>, mode: SelectionMode, selection_required: bool) -> Self {
        let items = items.into_iter().map(SelectionItem::new).collect();

        let mut list = Self {
            inner: SearchableList::with_items(items),
            mode,
            selection_required,
        };

        if selection_required {
            list.set_checked_at(0, true);
        }

        list
    }

    pub fn set_checked_at(&mut self, index: usize, checked: bool) {
        let Some(target) = self.items().get(index) else {
            return;
        };

        let clear_other_checks = checked && self.mode == SelectionMode::Single;

        if target.checked == checked && !clear_other_checks {
            return;
        }

        if !checked
            && self.selection_required
            && self
                .items()
                .iter()
                .enumerate()
                .all(|(other_index, item)| other_index == index || !item.checked)
        {
            return;
        }

        if clear_other_checks {
            let mut item_index = 0;
            self.inner.update_each(|item| {
                item.checked = item_index == index;
                item_index += 1;
            });
        } else {
            self.inner
                .update_item_at(index, |item| item.checked = checked);
        }
    }

    pub fn toggle_selected(&mut self) {
        if let Some(index) = self.selected() {
            let checked = self.items()[index].checked;
            self.set_checked_at(index, !checked);
        }
    }

    pub fn check_indices(&mut self, indices: &[usize]) {
        for &index in indices {
            if index >= self.len() {
                continue;
            }

            self.set_checked_at(index, true);

            if self.mode == SelectionMode::Single {
                break;
            }
        }
    }

    pub fn check_all(&mut self) {
        if self.mode == SelectionMode::Multiple {
            self.update_each(|item| item.checked = true);
        }
    }

    pub fn uncheck_all(&mut self) {
        if !self.selection_required {
            self.update_each(|item| item.checked = false);
        }
    }

    pub fn get_checked_items(&self) -> Vec<&T> {
        self.items()
            .iter()
            .filter(|item| item.checked)
            .map(|item| &item.item)
            .collect()
    }

    pub fn get_checked_item(&self) -> Option<&T> {
        self.items()
            .iter()
            .find(|item| item.checked)
            .map(|v| &v.item)
    }
}

impl<T: ListItem + Display> SearchList for SelectionList<T> {
    fn begin_search(&mut self, direction: SearchDirection) {
        self.inner.begin_search(direction);
    }

    fn set_search_pattern(&mut self, pattern: &str, update: SearchUpdate) -> bool {
        self.inner.set_search_pattern(pattern, update)
    }

    fn repeat_last(&mut self, last: &LastSearch, opposite: bool) -> bool {
        self.inner.repeat_last(last, opposite)
    }

    fn abort_search(&mut self) -> bool {
        self.inner.abort_search()
    }

    fn finish_search(&mut self) -> Option<LastSearch> {
        self.inner.finish_search()
    }

    fn pattern(&self) -> Option<&str> {
        self.inner.pattern()
    }

    fn has_matches(&self) -> bool {
        self.inner.has_matches()
    }
}

impl<T: ListItem> Deref for SelectionList<T> {
    type Target = SearchableList<SelectionItem<T>, ListState>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T: ListItem> DerefMut for SelectionList<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T: Searchable> Default for SelectionList<T> {
    fn default() -> Self {
        Self {
            inner: SearchableList::with_items(Vec::default()),
            mode: SelectionMode::Multiple,
            selection_required: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ListItem, Selectable, SelectionList, SelectionMode};

    fn list(mode: SelectionMode, selection_required: bool) -> SelectionList<String> {
        SelectionList::new(
            ["alpha", "beta", "gamma"].map(str::to_owned).to_vec(),
            mode,
            selection_required,
        )
    }

    fn checked_items(list: &SelectionList<String>) -> Vec<&str> {
        list.get_checked_items()
            .into_iter()
            .map(String::as_str)
            .collect()
    }

    #[test]
    fn initialization_checks_the_first_item_only_when_required() {
        for (mode, required) in [
            (SelectionMode::Single, false),
            (SelectionMode::Single, true),
            (SelectionMode::Multiple, false),
            (SelectionMode::Multiple, true),
        ] {
            let list = list(mode, required);
            let expected = if required { vec!["alpha"] } else { vec![] };

            assert_eq!(checked_items(&list), expected);
            assert_eq!(
                list.get_checked_item().map(String::as_str),
                expected.first().copied()
            );
        }
    }

    #[test]
    fn checking_an_item_in_single_mode_replaces_the_previous_check() {
        let mut list = list(SelectionMode::Single, false);
        list.set_checked_at(0, true);

        list.set_checked_at(2, true);

        assert_eq!(checked_items(&list), vec!["gamma"]);
        assert_eq!(list.get_checked_item().map(String::as_str), Some("gamma"));
    }

    #[test]
    fn multiple_mode_allows_independent_checks() {
        let mut list = list(SelectionMode::Multiple, false);
        list.set_checked_at(0, true);
        list.set_checked_at(2, true);
        assert_eq!(checked_items(&list), vec!["alpha", "gamma"]);

        list.set_checked_at(0, false);
        assert_eq!(checked_items(&list), vec!["gamma"]);

        list.set_checked_at(2, false);
        assert!(checked_items(&list).is_empty());
    }

    #[test]
    fn required_single_selection_can_be_replaced_but_not_unchecked() {
        let mut list = list(SelectionMode::Single, true);

        list.set_checked_at(0, false);
        assert_eq!(checked_items(&list), vec!["alpha"]);

        list.toggle_selected();
        assert_eq!(checked_items(&list), vec!["alpha"]);

        list.set_checked_at(2, true);
        assert_eq!(checked_items(&list), vec!["gamma"]);

        list.set_checked_at(2, false);
        assert_eq!(checked_items(&list), vec!["gamma"]);
    }

    #[test]
    fn required_multiple_selection_only_prevents_removing_the_last_check() {
        let mut list = list(SelectionMode::Multiple, true);
        list.set_checked_at(1, true);

        list.set_checked_at(0, false);
        assert_eq!(checked_items(&list), vec!["beta"]);

        list.set_checked_at(1, false);
        assert_eq!(checked_items(&list), vec!["beta"]);
    }

    #[test]
    fn toggling_the_highlighted_item_respects_the_selection_mode() {
        for (mode, after_check, after_uncheck) in [
            (SelectionMode::Single, vec!["beta"], vec![]),
            (
                SelectionMode::Multiple,
                vec!["alpha", "beta"],
                vec!["alpha"],
            ),
        ] {
            let mut list = list(mode, false);
            list.set_checked_at(0, true);
            list.select_with_index(1);
            assert_eq!(list.get_selected().map(ListItem::id), Some("beta"));
            assert_eq!(checked_items(&list), vec!["alpha"]);

            list.toggle_selected();
            assert_eq!(checked_items(&list), after_check);

            list.toggle_selected();
            assert_eq!(checked_items(&list), after_uncheck);
            assert_eq!(list.get_selected().map(ListItem::id), Some("beta"));
        }
    }

    #[test]
    fn checking_indices_in_single_mode_uses_the_first_valid_index() {
        let mut list = list(SelectionMode::Single, false);
        list.set_checked_at(0, true);

        list.check_indices(&[3, 2, 1]);

        assert_eq!(checked_items(&list), vec!["gamma"]);
    }

    #[test]
    fn bulk_checks_respect_the_mode_and_selection_requirement() {
        for (mode, required, after_check, after_uncheck) in [
            (SelectionMode::Single, false, vec!["beta"], vec![]),
            (SelectionMode::Single, true, vec!["beta"], vec!["beta"]),
            (
                SelectionMode::Multiple,
                false,
                vec!["alpha", "beta", "gamma"],
                vec![],
            ),
            (
                SelectionMode::Multiple,
                true,
                vec!["alpha", "beta", "gamma"],
                vec!["alpha", "beta", "gamma"],
            ),
        ] {
            let mut list = list(mode, required);
            list.set_checked_at(1, true);

            list.check_all();
            assert_eq!(checked_items(&list), after_check);

            list.uncheck_all();
            assert_eq!(checked_items(&list), after_uncheck);
        }
    }
}
