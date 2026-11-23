//! The asset library: everything a creator has kept, in one place with tags and search.
//! It holds screenshots (their pictures are files; only their details are here) and a
//! tag list, and gathers the side panel's clips and colours beside them.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::SidePanelData;

pub const MAX_SHOTS: usize = 200;
pub const MAX_TAGS_PER_ASSET: usize = 10;
pub const MAX_TAG_CHARS: usize = 30;
pub const MAX_SHOT_TITLE_CHARS: usize = 100;
pub const MAX_SHOT_SOURCE_CHARS: usize = 2_000;
/// How many assets one search shows. More than that, narrow the search.
pub const MAX_RESULTS: usize = 120;

/// A saved screenshot. Its picture is the file `shot-<id>.png` in the library folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shot {
    pub id: u32,
    pub title: String,
    pub saved_ms: i64,
    pub width: u32,
    pub height: u32,
    /// The page it was taken of, if it was a web page.
    pub source: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    pub shots: Vec<Shot>,
    /// Tags by asset key, such as `clip:3`, `color:#FF8800` or `shot:1`.
    pub tags: BTreeMap<String, Vec<String>>,
    next_id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetKind {
    Screenshot,
    Clip,
    Color,
}

impl AssetKind {
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "screenshot" => Some(Self::Screenshot),
            "clip" => Some(Self::Clip),
            "color" => Some(Self::Color),
            _ => None,
        }
    }
}

/// One thing in the library, whatever it is, for showing and searching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub key: String,
    pub kind: AssetKind,
    pub title: String,
    /// What's kept besides the title: a clip's note and address, a
    /// colour's code, a screenshot's page.
    pub detail: String,
    pub tags: Vec<String>,
    pub saved_ms: i64,
    /// For screenshots, the number of its picture.
    pub shot_id: Option<u32>,
    pub width: u32,
    pub height: u32,
}

/// A tag as it's kept: lower case, one line, no stray spaces, at most 30 characters.
pub fn clean_tag(text: &str) -> Option<String> {
    let words: Vec<String> = text
        .split_whitespace()
        .map(|word| {
            word.chars()
                .filter(|char| !char.is_control())
                .collect::<String>()
        })
        .filter(|word| !word.is_empty())
        .collect();
    let tag: String = words
        .join(" ")
        .to_lowercase()
        .chars()
        .take(MAX_TAG_CHARS)
        .collect();
    let tag = tag.trim().trim_start_matches('#').trim().to_owned();
    (!tag.is_empty()).then_some(tag)
}

fn clean_title(text: &str, limit: usize) -> String {
    let line: String = text
        .chars()
        .map(|char| if char.is_control() { ' ' } else { char })
        .collect();
    line.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}

impl Library {
    /// Reads saved details. Damaged data gives an empty library, and anything over a limit
    /// or with a bad tag is cut back, since the file could have been edited.
    pub fn parse(json: &str) -> Self {
        let mut library: Self = serde_json::from_str(json).unwrap_or_default();
        let mut seen = HashSet::new();
        library
            .shots
            .retain(|shot| shot.width > 0 && shot.height > 0 && seen.insert(shot.id));
        library.shots.truncate(MAX_SHOTS);
        for shot in &mut library.shots {
            shot.title = clean_title(&shot.title, MAX_SHOT_TITLE_CHARS);
            if shot.title.is_empty() {
                shot.title = "Screenshot".to_owned();
            }
            if !(shot.source.starts_with("http://") || shot.source.starts_with("https://"))
                || shot.source.chars().count() > MAX_SHOT_SOURCE_CHARS
            {
                shot.source.clear();
            }
        }
        for tags in library.tags.values_mut() {
            let mut kept: Vec<String> = Vec::new();
            for tag in tags.iter().filter_map(|tag| clean_tag(tag)) {
                if !kept.contains(&tag) {
                    kept.push(tag);
                }
            }
            kept.truncate(MAX_TAGS_PER_ASSET);
            *tags = kept;
        }
        library.tags.retain(|_, tags| !tags.is_empty());
        let highest = library.shots.iter().map(|shot| shot.id).max().unwrap_or(0);
        library.next_id = library.next_id.max(highest);
        library
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("Failed to serialize the library")
    }

    /// Adds a screenshot's details and returns its number, or `None` when the library
    /// is full. The caller writes the picture to `shot-<id>.png`.
    pub fn add_shot(
        &mut self,
        title: &str,
        source: &str,
        width: u32,
        height: u32,
        now_ms: i64,
    ) -> Option<u32> {
        if self.shots.len() >= MAX_SHOTS || width == 0 || height == 0 {
            return None;
        }
        self.next_id += 1;
        let id = self.next_id;
        let web = source.starts_with("http://") || source.starts_with("https://");
        let title = clean_title(title, MAX_SHOT_TITLE_CHARS);
        self.shots.insert(
            0,
            Shot {
                id,
                title: if title.is_empty() {
                    "Screenshot".to_owned()
                } else {
                    title
                },
                saved_ms: now_ms,
                width,
                height,
                source: if web && source.chars().count() <= MAX_SHOT_SOURCE_CHARS {
                    source.to_owned()
                } else {
                    String::new()
                },
            },
        );
        Some(id)
    }

    pub fn remove_shot(&mut self, id: u32) -> bool {
        let before = self.shots.len();
        self.shots.retain(|shot| shot.id != id);
        self.tags.remove(&format!("shot:{id}"));
        self.shots.len() != before
    }

    pub fn rename_shot(&mut self, id: u32, title: &str) -> bool {
        let title = clean_title(title, MAX_SHOT_TITLE_CHARS);
        match self.shots.iter_mut().find(|shot| shot.id == id) {
            Some(shot) if !title.is_empty() => {
                shot.title = title;
                true
            }
            _ => false,
        }
    }

    pub fn has_shot(&self, id: u32) -> bool {
        self.shots.iter().any(|shot| shot.id == id)
    }

    /// Tags an asset. False if the tag is empty, already there, or the asset has the most
    /// it can. The key isn't checked against what exists: tags are dropped by `prune`.
    pub fn add_tag(&mut self, key: &str, tag: &str) -> bool {
        let Some(tag) = clean_tag(tag) else {
            return false;
        };
        let tags = self.tags.entry(key.to_owned()).or_default();
        if tags.contains(&tag) || tags.len() >= MAX_TAGS_PER_ASSET {
            if tags.is_empty() {
                self.tags.remove(key);
            }
            return false;
        }
        tags.push(tag);
        true
    }

    pub fn remove_tag(&mut self, key: &str, tag: &str) -> bool {
        let Some(tags) = self.tags.get_mut(key) else {
            return false;
        };
        let before = tags.len();
        tags.retain(|saved| saved != tag);
        let changed = tags.len() != before;
        if tags.is_empty() {
            self.tags.remove(key);
        }
        changed
    }

    /// Forgets tags of things that no longer exist.
    pub fn prune(&mut self, panel: &SidePanelData) {
        let live: HashSet<String> = self
            .shots
            .iter()
            .map(|shot| format!("shot:{}", shot.id))
            .chain(panel.clips.iter().map(|clip| format!("clip:{}", clip.id)))
            .chain(panel.colors.iter().map(|color| format!("color:{color}")))
            .collect();
        self.tags.retain(|key, _| live.contains(key));
    }

    /// Everything kept, newest first: screenshots and clips by when they were saved, then
    /// colours in the order they're kept in.
    pub fn assets(&self, panel: &SidePanelData) -> Vec<Asset> {
        let tags_of = |key: &str| self.tags.get(key).cloned().unwrap_or_default();
        let mut assets: Vec<Asset> = Vec::new();
        for shot in &self.shots {
            let key = format!("shot:{}", shot.id);
            assets.push(Asset {
                tags: tags_of(&key),
                key,
                kind: AssetKind::Screenshot,
                title: shot.title.clone(),
                detail: shot.source.clone(),
                saved_ms: shot.saved_ms,
                shot_id: Some(shot.id),
                width: shot.width,
                height: shot.height,
            });
        }
        for clip in &panel.clips {
            let key = format!("clip:{}", clip.id);
            let detail = if clip.note.is_empty() {
                clip.url.clone()
            } else {
                format!("{}\n{}", clip.url, clip.note)
            };
            assets.push(Asset {
                tags: tags_of(&key),
                key,
                kind: AssetKind::Clip,
                title: clip.title.clone(),
                detail,
                saved_ms: clip.saved_ms,
                shot_id: None,
                width: 0,
                height: 0,
            });
        }
        assets.sort_by_key(|asset| std::cmp::Reverse(asset.saved_ms));
        for color in &panel.colors {
            let key = format!("color:{color}");
            assets.push(Asset {
                tags: tags_of(&key),
                key,
                kind: AssetKind::Color,
                title: color.clone(),
                detail: String::new(),
                saved_ms: 0,
                shot_id: None,
                width: 0,
                height: 0,
            });
        }
        assets
    }
}

/// Narrows assets to those matching a search, a kind and a tag. Every word of the search
/// must be in the title, the detail or a tag, in any case.
pub fn search(
    assets: Vec<Asset>,
    query: &str,
    kind: Option<AssetKind>,
    tag: Option<&str>,
) -> Vec<Asset> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let tag = tag.and_then(clean_tag);
    assets
        .into_iter()
        .filter(|asset| kind.is_none_or(|kind| asset.kind == kind))
        .filter(|asset| tag.as_ref().is_none_or(|tag| asset.tags.contains(tag)))
        .filter(|asset| {
            let haystack = format!(
                "{}\n{}\n{}",
                asset.title,
                asset.detail,
                asset.tags.join(" ")
            )
            .to_lowercase();
            words.iter().all(|word| haystack.contains(word.as_str()))
        })
        .collect()
}

/// Every tag in use among the assets, most used first.
pub fn tag_counts(assets: &[Asset]) -> Vec<(String, usize)> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for tag in assets.iter().flat_map(|asset| &asset.tags) {
        *counts.entry(tag.as_str()).or_default() += 1;
    }
    let mut counts: Vec<(String, usize)> = counts
        .into_iter()
        .map(|(tag, count)| (tag.to_owned(), count))
        .collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel() -> SidePanelData {
        let mut panel = SidePanelData::default();
        panel.add_clip(
            "Editing tips",
            "https://a.example/tips",
            "ripple delete saves time",
            100,
        );
        panel.add_clip("Sign-off", "https://b.example/", "Thanks for watching!", 90);
        panel.add_color("#ff8800");
        panel
    }

    #[test]
    fn tags_are_tidied_and_limited() {
        assert_eq!(
            clean_tag("  Thumbnail  Ideas "),
            Some("thumbnail ideas".to_owned())
        );
        assert_eq!(clean_tag("#Brand"), Some("brand".to_owned()));
        assert_eq!(clean_tag("a\u{7}b"), Some("ab".to_owned()));
        assert_eq!(clean_tag("   "), None);
        assert_eq!(clean_tag("#"), None);
        assert_eq!(
            clean_tag(&"x".repeat(50)).unwrap().chars().count(),
            MAX_TAG_CHARS
        );
        let mut library = Library::default();
        for number in 0..MAX_TAGS_PER_ASSET {
            assert!(library.add_tag("clip:1", &format!("tag{number}")));
        }
        assert!(!library.add_tag("clip:1", "one too many"));
        assert!(!library.add_tag("clip:1", "TAG1"));
    }

    #[test]
    fn adds_tags_removes_them_and_forgets_empty_ones() {
        let mut library = Library::default();
        assert!(library.add_tag("clip:1", "Research"));
        assert!(!library.add_tag("clip:1", "research"));
        assert!(library.remove_tag("clip:1", "research"));
        assert!(!library.remove_tag("clip:1", "research"));
        assert!(library.tags.is_empty());
        assert!(!library.add_tag("clip:1", "  "));
        assert!(library.tags.is_empty());
    }

    #[test]
    fn screenshots_are_added_numbered_renamed_and_removed() {
        let mut library = Library::default();
        let first = library
            .add_shot("  Hero  shot ", "https://a.example/", 1280, 680, 10)
            .unwrap();
        let second = library
            .add_shot("", "javascript:alert(1)", 800, 600, 20)
            .unwrap();
        assert_eq!((first, second), (1, 2));
        assert_eq!(library.shots[0].title, "Screenshot");
        assert_eq!(library.shots[0].source, "");
        assert_eq!(library.shots[1].title, "Hero shot");
        assert!(library.add_shot("x", "", 0, 10, 1).is_none());
        assert!(library.rename_shot(first, "Landing page"));
        assert!(!library.rename_shot(first, "   ") && !library.rename_shot(99, "x"));
        library.add_tag("shot:1", "landing");
        assert!(library.remove_shot(first) && !library.remove_shot(first));
        assert!(library.tags.is_empty());
        assert!(!library.has_shot(first) && library.has_shot(second));
    }

    #[test]
    fn the_library_is_limited() {
        let mut library = Library::default();
        for number in 0..MAX_SHOTS {
            assert!(
                library
                    .add_shot(&format!("s{number}"), "", 10, 10, number as i64)
                    .is_some()
            );
        }
        assert!(library.add_shot("one too many", "", 10, 10, 0).is_none());
    }

    #[test]
    fn gathers_everything_with_screenshots_and_clips_newest_first() {
        let mut library = Library::default();
        library.add_shot("Old shot", "", 10, 10, 50);
        let panel = panel();
        let assets = library.assets(&panel);
        let kinds: Vec<_> = assets.iter().map(|asset| asset.kind).collect();
        assert_eq!(
            kinds,
            [
                AssetKind::Clip,
                AssetKind::Clip,
                AssetKind::Screenshot,
                AssetKind::Color
            ]
        );
        assert_eq!(assets[0].key, "clip:1");
        assert!(assets[0].detail.contains("ripple delete"));
        assert_eq!(assets[3].key, "color:#FF8800");
    }

    #[test]
    fn search_needs_every_word_in_title_detail_or_tags() {
        let mut library = Library::default();
        library.add_tag("clip:1", "Research");
        let all = library.assets(&panel());
        let found = |query: &str| search(all.clone(), query, None, None).len();
        assert_eq!(found(""), 3);
        assert_eq!(found("editing"), 1);
        assert_eq!(found("RIPPLE time"), 1);
        assert_eq!(found("research"), 1);
        assert_eq!(found("editing watching"), 0);
        assert_eq!(found("ff8800"), 1);
        assert_eq!(
            search(all.clone(), "", Some(AssetKind::Color), None).len(),
            1
        );
        assert_eq!(search(all.clone(), "", None, Some("Research")).len(), 1);
        assert_eq!(search(all, "", None, Some("nothing")).len(), 0);
    }

    #[test]
    fn counts_tags_most_used_first() {
        let mut library = Library::default();
        library.add_tag("clip:1", "b");
        library.add_tag("clip:2", "b");
        library.add_tag("clip:2", "a");
        let counts = tag_counts(&library.assets(&panel()));
        assert_eq!(counts, [("b".to_owned(), 2), ("a".to_owned(), 1)]);
    }

    #[test]
    fn tags_of_things_that_are_gone_are_dropped() {
        let mut library = Library::default();
        library.add_tag("clip:1", "keep");
        library.add_tag("clip:99", "gone");
        library.add_tag("color:#000000", "gone");
        library.prune(&panel());
        assert_eq!(library.tags.keys().collect::<Vec<_>>(), ["clip:1"]);
    }

    #[test]
    fn saved_data_is_checked_when_read() {
        let json = r#"{"shots":[
            {"id":1,"title":"ok","saved_ms":1,"width":10,"height":10,"source":"https://a.example/"},
            {"id":1,"title":"duplicate","saved_ms":1,"width":10,"height":10,"source":""},
            {"id":2,"title":"","saved_ms":1,"width":0,"height":10,"source":""},
            {"id":3,"title":"bad source","saved_ms":1,"width":5,"height":5,"source":"file:///etc/passwd"}
        ],"tags":{"clip:1":["Good","good","  "],"clip:2":[" "]},"next_id":0}"#;
        let library = Library::parse(json);
        assert_eq!(library.shots.len(), 2);
        assert_eq!(library.shots[1].source, "");
        assert_eq!(library.tags.len(), 1);
        assert_eq!(library.tags["clip:1"], ["good"]);
        let mut library = library;
        assert_eq!(library.add_shot("n", "", 1, 1, 1), Some(4));
        assert_eq!(Library::parse("not json"), Library::default());
    }
}
