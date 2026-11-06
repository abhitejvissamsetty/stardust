use serde::Serialize;

use crate::BrowserState;

pub type TabId = u32;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Tab {
    pub id: TabId,
    pub title: String,
    #[serde(flatten)]
    pub page: BrowserState,
    /// The tab's Back and Forward list, kept up to date for saving.
    #[serde(skip)]
    pub history_entries: Vec<String>,
    #[serde(skip)]
    pub history_index: usize,
    /// The tab this one was opened from, shown again when this one closes. It's
    /// forgotten once the person switches away, as other browsers do.
    #[serde(skip)]
    pub opener: Option<TabId>,
}

/// The ordered tabs in a window and which one is showing.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TabStrip {
    tabs: Vec<Tab>,
    active_id: Option<TabId>,
    #[serde(skip)]
    next_id: TabId,
}

impl TabStrip {
    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn active_id(&self) -> Option<TabId> {
        self.active_id
    }

    pub fn active(&self) -> Option<&Tab> {
        self.active_id.and_then(|id| self.get(id))
    }

    pub fn get(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.id == id)
    }

    pub fn get_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|tab| tab.id == id)
    }

    /// Adds a tab to the right of the active one, without showing it.
    pub fn add(&mut self) -> TabId {
        self.next_id += 1;
        let id = self.next_id;
        let index = self
            .active_id
            .and_then(|active| self.index_of(active))
            .map_or(self.tabs.len(), |index| index + 1);
        self.tabs.insert(
            index,
            Tab {
                id,
                ..Default::default()
            },
        );
        id
    }

    /// Returns whether the tab exists.
    pub fn select(&mut self, id: TabId) -> bool {
        let exists = self.get(id).is_some();
        if exists {
            if let Some(previous) = self.active_id.filter(|previous| *previous != id)
                && let Some(tab) = self.get_mut(previous)
            {
                tab.opener = None;
            }
            self.active_id = Some(id);
        }
        exists
    }

    /// Records which tab opened another.
    pub fn set_opener(&mut self, id: TabId, opener: TabId) {
        if let Some(tab) = self.get_mut(id) {
            tab.opener = Some(opener);
        }
    }

    /// Removes a tab. If it was showing, the tab that opened it shows next, or else its
    /// right neighbor, or else its left.
    pub fn remove(&mut self, id: TabId) {
        let Some(index) = self.index_of(id) else {
            return;
        };
        let removed = self.tabs.remove(index);
        for tab in &mut self.tabs {
            if tab.opener == Some(id) {
                tab.opener = None;
            }
        }
        if self.active_id == Some(id) {
            let opener = removed.opener.filter(|opener| self.get(*opener).is_some());
            let next = opener.or_else(|| {
                self.tabs
                    .get(index)
                    .or_else(|| self.tabs.last())
                    .map(|tab| tab.id)
            });
            self.active_id = next;
        }
    }

    /// The tab `offset` places from the active one, wrapping around.
    pub fn neighbor(&self, offset: i32) -> Option<TabId> {
        let index = self.index_of(self.active_id?)? as i32;
        let next = (index + offset).rem_euclid(self.tabs.len() as i32);
        Some(self.tabs[next as usize].id)
    }

    /// The tab at a zero-based position. Positions past the end give nothing.
    pub fn at(&self, index: usize) -> Option<TabId> {
        self.tabs.get(index).map(|tab| tab.id)
    }

    pub fn last(&self) -> Option<TabId> {
        self.tabs.last().map(|tab| tab.id)
    }

    /// Moves a tab to a zero-based position, clamped to the strip.
    pub fn move_to(&mut self, id: TabId, index: usize) {
        if let Some(from) = self.index_of(id) {
            let tab = self.tabs.remove(from);
            self.tabs.insert(index.min(self.tabs.len()), tab);
        }
    }

    /// Moves the active tab `offset` places, stopping at either end.
    pub fn move_active(&mut self, offset: i32) {
        let Some(id) = self.active_id else {
            return;
        };
        if let Some(index) = self.index_of(id) {
            let target = (index as i32 + offset).max(0) as usize;
            self.move_to(id, target);
        }
    }

    fn index_of(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(strip: &TabStrip) -> Vec<TabId> {
        strip.tabs().iter().map(|tab| tab.id).collect()
    }

    fn strip_of_three() -> TabStrip {
        let mut strip = TabStrip::default();
        for _ in 0..3 {
            let id = strip.add();
            strip.select(id);
        }
        strip
    }

    #[test]
    fn new_tabs_open_next_to_the_active_one() {
        let mut strip = strip_of_three();
        strip.select(1);
        let new = strip.add();
        assert_eq!(order(&strip), vec![1, new, 2, 3]);
        assert_eq!(strip.active_id(), Some(1));
    }

    #[test]
    fn closing_the_active_tab_shows_its_right_neighbor() {
        let mut strip = strip_of_three();
        strip.select(2);
        strip.remove(2);
        assert_eq!(strip.active_id(), Some(3));
    }

    #[test]
    fn closing_the_last_position_shows_the_left_neighbor() {
        let mut strip = strip_of_three();
        strip.remove(3);
        assert_eq!(strip.active_id(), Some(2));
    }

    #[test]
    fn closing_a_background_tab_keeps_the_active_one() {
        let mut strip = strip_of_three();
        strip.remove(1);
        assert_eq!(strip.active_id(), Some(3));
        assert_eq!(order(&strip), vec![2, 3]);
    }

    #[test]
    fn closing_every_tab_leaves_nothing_active() {
        let mut strip = TabStrip::default();
        let id = strip.add();
        strip.select(id);
        strip.remove(id);
        assert_eq!(strip.active_id(), None);
        assert_eq!(strip.neighbor(1), None);
    }

    #[test]
    fn neighbors_wrap_around() {
        let mut strip = strip_of_three();
        assert_eq!(strip.neighbor(1), Some(1));
        strip.select(1);
        assert_eq!(strip.neighbor(-1), Some(3));
    }

    #[test]
    fn selecting_a_missing_tab_changes_nothing() {
        let mut strip = strip_of_three();
        assert!(!strip.select(99));
        assert_eq!(strip.active_id(), Some(3));
    }

    #[test]
    fn moving_tabs_clamps_to_the_strip() {
        let mut strip = strip_of_three();
        strip.move_to(1, 99);
        assert_eq!(order(&strip), vec![2, 3, 1]);
        strip.move_to(1, 0);
        assert_eq!(order(&strip), vec![1, 2, 3]);
        assert_eq!(strip.active_id(), Some(3));
    }

    #[test]
    fn closing_an_opened_tab_returns_to_its_opener() {
        let mut strip = strip_of_three();
        strip.select(1);
        let opened = strip.add();
        strip.set_opener(opened, 1);
        strip.select(opened);
        strip.remove(opened);
        assert_eq!(strip.active_id(), Some(1));
    }

    #[test]
    fn switching_away_forgets_the_opener() {
        let mut strip = strip_of_three();
        strip.select(1);
        let opened = strip.add();
        strip.set_opener(opened, 1);
        strip.select(opened);
        strip.select(3);
        strip.select(opened);
        strip.remove(opened);
        // The right neighbor of the opened tab, which sat between 1 and 2.
        assert_eq!(strip.active_id(), Some(2));
    }

    #[test]
    fn a_closed_opener_is_forgotten() {
        let mut strip = strip_of_three();
        strip.select(1);
        let opened = strip.add();
        strip.set_opener(opened, 1);
        strip.remove(1);
        strip.select(opened);
        strip.remove(opened);
        assert_eq!(strip.active_id(), Some(2));
    }

    #[test]
    fn moving_the_active_tab_stops_at_the_ends() {
        let mut strip = strip_of_three();
        strip.move_active(1);
        assert_eq!(order(&strip), vec![1, 2, 3]);
        strip.move_active(-1);
        assert_eq!(order(&strip), vec![1, 3, 2]);
        strip.move_active(-5);
        assert_eq!(order(&strip), vec![3, 1, 2]);
    }

    #[test]
    fn ids_are_never_reused() {
        let mut strip = strip_of_three();
        strip.remove(3);
        assert_eq!(strip.add(), 4);
    }

    #[test]
    fn serializes_for_the_toolbar() {
        let mut strip = TabStrip::default();
        let id = strip.add();
        strip.select(id);
        strip.get_mut(id).unwrap().page.can_go_back = true;
        let json = serde_json::to_string(&strip).unwrap();
        assert!(json.contains(r#""activeId":1"#));
        assert!(json.contains(r#""canGoBack":true"#));
    }
}
