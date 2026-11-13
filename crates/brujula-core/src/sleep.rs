//! Tabs that were left alone for a while go to sleep: their page is thrown away, which frees the
//! memory and the processor time it was using, and loaded again the moment the tab is opened. The
//! tab stays in the sidebar with its title and icon. Unlike archiving, nothing is closed.

use crate::tabs::{TabId, TabModel};

const MINUTE_MS: u64 = 60_000;

fn is_web(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// The tabs to put to sleep at `now` when the limit is `after_minutes` (0: none). The tab in
/// front, tabs that play sound or are loading, and pages that are not web pages stay awake.
pub fn drowsy(model: &TabModel, now: u64, after_minutes: u32) -> Vec<TabId> {
    if after_minutes == 0 {
        return Vec::new();
    }
    let limit = u64::from(after_minutes) * MINUTE_MS;
    model
        .tabs()
        .iter()
        .filter(|t| {
            Some(t.id) != model.selected()
                && !t.audible
                && !t.loading
                && is_web(&t.url)
                && t.active > 0
                && now.saturating_sub(t.active) >= limit
        })
        .map(|t| t.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_000 * MINUTE_MS;

    fn model(ages: &[u64]) -> (TabModel, Vec<TabId>) {
        let mut m = TabModel::new();
        let mut ids = Vec::new();
        for (i, age) in ages.iter().enumerate() {
            let (id, _) = m.open(&format!("https://t{i}.example/"), None, false);
            m.touch(id, NOW - age * MINUTE_MS);
            ids.push(id);
        }
        // The tab in front is the last one opened.
        (m, ids)
    }

    #[test]
    fn only_tabs_idle_for_long_enough_sleep() {
        let (mut m, ids) = model(&[5, 40, 40, 90]);
        m.select(ids[0]);
        assert_eq!(drowsy(&m, NOW, 30), vec![ids[1], ids[2], ids[3]]);
        assert_eq!(drowsy(&m, NOW, 60), vec![ids[3]]);
        assert!(drowsy(&m, NOW, 0).is_empty());
    }

    #[test]
    fn the_tab_in_front_sound_loading_and_non_web_pages_stay_awake() {
        let (mut m, ids) = model(&[90, 90, 90, 90]);
        m.select(ids[0]);
        m.update(ids[1], |t| t.audible = true);
        m.update(ids[2], |t| t.loading = true);
        m.update(ids[3], |t| t.url = "brujula://settings".into());
        assert!(drowsy(&m, NOW, 30).is_empty());
    }

    #[test]
    fn a_tab_never_seen_in_use_is_left_alone() {
        let (mut m, ids) = model(&[90, 90]);
        m.select(ids[0]);
        m.update(ids[1], |t| t.active = 0);
        assert!(drowsy(&m, NOW, 30).is_empty());
    }
}
