//! The tab model: an ordered list with pinned tabs first, a selection, and a stack of closed
//! tabs to reopen. No widgets here; the UI renders it.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TabId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    pub id: TabId,
    pub title: String,
    pub url: String,
    pub pinned: bool,
    pub loading: bool,
    pub audible: bool,
}

/// What a change to the model means for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Opened(TabId),
    Closed(TabId),
    Updated(TabId),
    Moved(TabId),
    Selected(TabId),
}

#[derive(Debug, Default)]
pub struct TabModel {
    next_id: u64,
    tabs: Vec<Tab>,
    selected: Option<TabId>,
    closed: Vec<(String, String, usize)>,
}

const MAX_CLOSED: usize = 25;

impl TabModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn selected(&self) -> Option<TabId> {
        self.selected
    }

    pub fn get(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    pub fn position(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    fn pinned_count(&self) -> usize {
        self.tabs.iter().take_while(|t| t.pinned).count()
    }

    /// Opens a tab. `after` places it right after another tab (a link opened from a page);
    /// otherwise it goes to the end. It never lands among the pinned tabs. The new tab becomes
    /// the selected one when `select` is true.
    pub fn open(&mut self, url: &str, after: Option<TabId>, select: bool) -> (TabId, Vec<Change>) {
        let id = TabId(self.next_id);
        self.next_id += 1;
        let at = match after.and_then(|a| self.position(a)) {
            Some(p) => (p + 1).max(self.pinned_count()),
            None => self.tabs.len(),
        };
        self.tabs.insert(
            at,
            Tab {
                id,
                title: String::new(),
                url: url.to_owned(),
                pinned: false,
                loading: false,
                audible: false,
            },
        );
        let mut changes = vec![Change::Opened(id)];
        if select || self.selected.is_none() {
            self.selected = Some(id);
            changes.push(Change::Selected(id));
        }
        (id, changes)
    }

    /// Closes a tab. If it was selected, the neighbour that was next to it is selected (the
    /// one that takes its place, or the previous one if it was last). Closed tabs are
    /// remembered for [`reopen`](Self::reopen) unless blank.
    pub fn close(&mut self, id: TabId) -> Vec<Change> {
        let Some(pos) = self.position(id) else {
            return Vec::new();
        };
        let tab = self.tabs.remove(pos);
        if !tab.url.is_empty() && tab.url != "about:blank" {
            self.closed.push((tab.url, tab.title, pos));
            if self.closed.len() > MAX_CLOSED {
                self.closed.remove(0);
            }
        }
        let mut changes = vec![Change::Closed(id)];
        if self.selected == Some(id) {
            self.selected = if self.tabs.is_empty() {
                None
            } else {
                Some(self.tabs[pos.min(self.tabs.len() - 1)].id)
            };
            if let Some(s) = self.selected {
                changes.push(Change::Selected(s));
            }
        }
        changes
    }

    /// Reopens the most recently closed tab in its old position (clamped).
    pub fn reopen(&mut self) -> Option<(TabId, Vec<Change>)> {
        let (url, title, pos) = self.closed.pop()?;
        let (id, mut changes) = self.open(&url, None, true);
        let from = self.position(id)?;
        let to = pos.min(self.tabs.len() - 1).max(self.pinned_count());
        if from != to {
            let tab = self.tabs.remove(from);
            self.tabs.insert(to, tab);
            changes.push(Change::Moved(id));
        }
        if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
            t.title = title;
        }
        Some((id, changes))
    }

    pub fn select(&mut self, id: TabId) -> Vec<Change> {
        if self.get(id).is_none() || self.selected == Some(id) {
            return Vec::new();
        }
        self.selected = Some(id);
        vec![Change::Selected(id)]
    }

    /// Selects the next (`+1`) or previous (`-1`) tab, wrapping around.
    pub fn select_relative(&mut self, delta: isize) -> Vec<Change> {
        let Some(cur) = self.selected.and_then(|s| self.position(s)) else {
            return Vec::new();
        };
        let n = self.tabs.len() as isize;
        let next = (cur as isize + delta).rem_euclid(n) as usize;
        let id = self.tabs[next].id;
        self.select(id)
    }

    /// Pins or unpins. Pinned tabs live at the start, in the order they were pinned; an
    /// unpinned tab goes right after the pinned ones.
    pub fn set_pinned(&mut self, id: TabId, pinned: bool) -> Vec<Change> {
        let Some(pos) = self.position(id) else {
            return Vec::new();
        };
        if self.tabs[pos].pinned == pinned {
            return Vec::new();
        }
        let mut tab = self.tabs.remove(pos);
        tab.pinned = pinned;
        let at = self.pinned_count();
        self.tabs.insert(at, tab);
        vec![Change::Moved(id), Change::Updated(id)]
    }

    /// Moves a tab to `to`, but never across the pinned/unpinned boundary.
    pub fn move_to(&mut self, id: TabId, to: usize) -> Vec<Change> {
        let Some(pos) = self.position(id) else {
            return Vec::new();
        };
        let pinned = self.tabs[pos].pinned;
        let pc = self.pinned_count();
        let (lo, hi) = if pinned {
            (0, pc - 1)
        } else {
            (pc, self.tabs.len() - 1)
        };
        let to = to.clamp(lo, hi);
        if to == pos {
            return Vec::new();
        }
        let tab = self.tabs.remove(pos);
        self.tabs.insert(to, tab);
        vec![Change::Moved(id)]
    }

    pub fn update(&mut self, id: TabId, f: impl FnOnce(&mut Tab)) -> Vec<Change> {
        match self.tabs.iter_mut().find(|t| t.id == id) {
            Some(t) => {
                let before = t.clone();
                f(t);
                if *t == before {
                    Vec::new()
                } else {
                    vec![Change::Updated(id)]
                }
            }
            None => Vec::new(),
        }
    }

    /// Closes every tab except `keep` and the pinned ones. Returns the closed ids.
    pub fn close_others(&mut self, keep: TabId) -> Vec<TabId> {
        let ids: Vec<TabId> = self
            .tabs
            .iter()
            .filter(|t| t.id != keep && !t.pinned)
            .map(|t| t.id)
            .collect();
        for id in &ids {
            self.close(*id);
        }
        ids
    }

    /// Closes the unpinned tabs below `from` in the list.
    pub fn close_below(&mut self, from: TabId) -> Vec<TabId> {
        let Some(pos) = self.position(from) else {
            return Vec::new();
        };
        let ids: Vec<TabId> = self.tabs[pos + 1..]
            .iter()
            .filter(|t| !t.pinned)
            .map(|t| t.id)
            .collect();
        for id in &ids {
            self.close(*id);
        }
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(n: usize) -> (TabModel, Vec<TabId>) {
        let mut m = TabModel::new();
        let ids = (0..n)
            .map(|i| m.open(&format!("https://e{i}.com"), None, false).0)
            .collect();
        (m, ids)
    }

    #[test]
    fn first_tab_is_selected() {
        let (m, ids) = model(3);
        assert_eq!(m.selected(), Some(ids[0]));
    }

    #[test]
    fn open_after_goes_next_to_the_opener() {
        let (mut m, ids) = model(3);
        let (new, _) = m.open("https://x.com", Some(ids[0]), true);
        assert_eq!(m.position(new), Some(1));
        assert_eq!(m.selected(), Some(new));
    }

    #[test]
    fn closing_selected_selects_the_one_that_takes_its_place() {
        let (mut m, ids) = model(3);
        m.select(ids[1]);
        m.close(ids[1]);
        assert_eq!(m.selected(), Some(ids[2]));
        m.close(ids[2]);
        assert_eq!(m.selected(), Some(ids[0]));
        m.close(ids[0]);
        assert_eq!(m.selected(), None);
        assert!(m.is_empty());
    }

    #[test]
    fn closing_unselected_keeps_selection() {
        let (mut m, ids) = model(3);
        m.close(ids[2]);
        assert_eq!(m.selected(), Some(ids[0]));
    }

    #[test]
    fn pinned_tabs_stay_first() {
        let (mut m, ids) = model(4);
        m.set_pinned(ids[2], true);
        assert_eq!(m.tabs()[0].id, ids[2]);
        m.set_pinned(ids[3], true);
        assert_eq!(m.tabs()[1].id, ids[3]);
        let (new, _) = m.open("https://n.com", Some(ids[2]), false);
        assert!(
            m.position(new).unwrap() >= 2,
            "new tab must not land among pinned"
        );
        m.set_pinned(ids[2], false);
        assert_eq!(m.tabs()[0].id, ids[3]);
        assert_eq!(m.position(ids[2]), Some(1));
    }

    #[test]
    fn move_never_crosses_the_pinned_boundary() {
        let (mut m, ids) = model(4);
        m.set_pinned(ids[0], true);
        m.move_to(ids[2], 0);
        assert_eq!(m.position(ids[2]), Some(1));
        m.move_to(ids[0], 3);
        assert_eq!(m.position(ids[0]), Some(0));
        m.move_to(ids[1], 99);
        assert_eq!(m.position(ids[1]), Some(3));
    }

    #[test]
    fn reopen_restores_url_title_and_position() {
        let (mut m, ids) = model(3);
        m.update(ids[1], |t| t.title = "Dos".into());
        m.close(ids[1]);
        let (id, _) = m.reopen().unwrap();
        assert_eq!(m.position(id), Some(1));
        assert_eq!(m.get(id).unwrap().url, "https://e1.com");
        assert_eq!(m.get(id).unwrap().title, "Dos");
        assert!(m.reopen().is_none());
    }

    #[test]
    fn blank_tabs_are_not_reopened() {
        let mut m = TabModel::new();
        let (a, _) = m.open("about:blank", None, true);
        m.close(a);
        assert!(m.reopen().is_none());
    }

    #[test]
    fn select_relative_wraps() {
        let (mut m, ids) = model(3);
        m.select_relative(-1);
        assert_eq!(m.selected(), Some(ids[2]));
        m.select_relative(1);
        assert_eq!(m.selected(), Some(ids[0]));
    }

    #[test]
    fn close_others_keeps_pinned_and_given() {
        let (mut m, ids) = model(5);
        m.set_pinned(ids[3], true);
        m.close_others(ids[1]);
        let left: Vec<_> = m.tabs().iter().map(|t| t.id).collect();
        assert_eq!(left, vec![ids[3], ids[1]]);
    }

    #[test]
    fn close_below_only_closes_unpinned_after() {
        let (mut m, ids) = model(4);
        m.close_below(ids[1]);
        let left: Vec<_> = m.tabs().iter().map(|t| t.id).collect();
        assert_eq!(left, vec![ids[0], ids[1]]);
    }

    #[test]
    fn closed_history_is_bounded() {
        let mut m = TabModel::new();
        for i in 0..40 {
            let (id, _) = m.open(&format!("https://e{i}.com"), None, true);
            m.close(id);
        }
        let mut n = 0;
        while m.reopen().is_some() {
            n += 1;
        }
        assert_eq!(n, MAX_CLOSED);
    }

    #[test]
    fn update_reports_only_real_changes() {
        let (mut m, ids) = model(1);
        assert!(m.update(ids[0], |t| t.title = String::new()).is_empty());
        assert_eq!(
            m.update(ids[0], |t| t.loading = true),
            vec![Change::Updated(ids[0])]
        );
    }
}
