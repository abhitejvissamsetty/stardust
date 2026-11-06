//! Which background tabs to unload to save memory. An unloaded tab keeps its place,
//! title and Back and Forward list, and loads again when it's selected.

/// Tabs left alone this long are unloaded.
pub const DISCARD_IDLE_MS: i64 = 30 * 60 * 1000;
/// Above this many loaded tabs across all windows, the least recently used ones unload.
pub const DISCARD_LOADED_LIMIT: usize = 10;
/// How often the browser checks.
pub const DISCARD_CHECK_MS: i64 = 60 * 1000;

/// A loaded tab, as the discard check sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadedTab<K> {
    pub key: K,
    /// When the tab was last showing, or when it opened if it never has.
    pub last_shown_ms: i64,
    /// Showing in a window, playing sound, loading, asking something or downloading.
    pub protected: bool,
}

/// The tabs to unload, least recently shown first.
pub fn tabs_to_discard<K: Copy>(
    tabs: &[LoadedTab<K>],
    now_ms: i64,
    idle_ms: i64,
    loaded_limit: usize,
) -> Vec<K> {
    let mut candidates: Vec<&LoadedTab<K>> = tabs.iter().filter(|tab| !tab.protected).collect();
    candidates.sort_by_key(|tab| tab.last_shown_ms);

    let mut loaded = tabs.len();
    let mut chosen = Vec::new();
    for tab in candidates {
        let is_idle = now_ms - tab.last_shown_ms >= idle_ms;
        if is_idle || loaded > loaded_limit {
            chosen.push(tab.key);
            loaded -= 1;
        }
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: i64 = 60 * 1000;

    fn tab(key: u32, last_shown_ms: i64, protected: bool) -> LoadedTab<u32> {
        LoadedTab {
            key,
            last_shown_ms,
            protected,
        }
    }

    #[test]
    fn unloads_tabs_left_alone_for_a_while() {
        let now = 100 * MINUTE;
        let tabs = [
            tab(1, now - 40 * MINUTE, false),
            tab(2, now - MINUTE, false),
        ];
        assert_eq!(tabs_to_discard(&tabs, now, DISCARD_IDLE_MS, 10), vec![1]);
    }

    #[test]
    fn never_unloads_protected_tabs() {
        let now = 100 * MINUTE;
        let tabs = [tab(1, 0, true), tab(2, 0, true)];
        assert!(tabs_to_discard(&tabs, now, DISCARD_IDLE_MS, 0).is_empty());
    }

    #[test]
    fn unloads_the_least_recently_shown_above_the_limit() {
        let now = 100 * MINUTE;
        let tabs = [
            tab(1, now - 3 * MINUTE, false),
            tab(2, now - 5 * MINUTE, false),
            tab(3, now - MINUTE, false),
            tab(4, now, true),
        ];
        assert_eq!(tabs_to_discard(&tabs, now, DISCARD_IDLE_MS, 2), vec![2, 1]);
    }

    #[test]
    fn protected_tabs_count_towards_the_limit() {
        let now = 100 * MINUTE;
        let tabs = [tab(1, now, true), tab(2, now, true), tab(3, now, false)];
        assert_eq!(tabs_to_discard(&tabs, now, DISCARD_IDLE_MS, 2), vec![3]);
    }

    #[test]
    fn nothing_unloads_within_the_limit_and_idle_time() {
        let now = 100 * MINUTE;
        let tabs = [tab(1, now - MINUTE, false), tab(2, now - 2 * MINUTE, false)];
        assert!(tabs_to_discard(&tabs, now, DISCARD_IDLE_MS, 10).is_empty());
    }
}
