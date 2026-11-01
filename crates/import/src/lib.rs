//! Reads bookmarks and history from other browsers on this computer. It never opens
//! their passwords, cookies or payment data.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::Serialize;

/// Past this, the oldest history is left behind, which keeps imports quick.
const HISTORY_LIMIT: u32 = 20_000;
/// Microseconds between the Chromium epoch (1601) and the Unix epoch (1970).
const CHROMIUM_EPOCH_OFFSET_US: i64 = 11_644_473_600_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Chromium,
    Firefox,
}

/// A browser profile found on this computer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Source {
    pub id: String,
    pub name: String,
    #[serde(skip)]
    kind: Kind,
    #[serde(skip)]
    profile: PathBuf,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Item {
    pub url: String,
    pub title: String,
    /// Milliseconds since 1970: when it was bookmarked or last visited.
    pub time_ms: i64,
    pub visit_count: i64,
}

#[derive(Debug, Default)]
pub struct Imported {
    pub bookmarks: Vec<Item>,
    pub history: Vec<Item>,
}

fn chromium_folders(home: &Path) -> Vec<(&'static str, PathBuf)> {
    if cfg!(target_os = "windows") {
        let local = home.join("AppData/Local");
        vec![
            ("Google Chrome", local.join("Google/Chrome/User Data")),
            ("Chromium", local.join("Chromium/User Data")),
            ("Microsoft Edge", local.join("Microsoft/Edge/User Data")),
            ("Brave", local.join("BraveSoftware/Brave-Browser/User Data")),
        ]
    } else if cfg!(target_os = "macos") {
        let support = home.join("Library/Application Support");
        vec![
            ("Google Chrome", support.join("Google/Chrome")),
            ("Chromium", support.join("Chromium")),
            ("Microsoft Edge", support.join("Microsoft Edge")),
            ("Brave", support.join("BraveSoftware/Brave-Browser")),
        ]
    } else {
        let config = home.join(".config");
        vec![
            ("Google Chrome", config.join("google-chrome")),
            ("Chromium", config.join("chromium")),
            ("Microsoft Edge", config.join("microsoft-edge")),
            ("Brave", config.join("BraveSoftware/Brave-Browser")),
        ]
    }
}

fn firefox_folders(home: &Path) -> Vec<PathBuf> {
    if cfg!(target_os = "windows") {
        vec![home.join("AppData/Roaming/Mozilla/Firefox/Profiles")]
    } else if cfg!(target_os = "macos") {
        vec![home.join("Library/Application Support/Firefox/Profiles")]
    } else {
        // Newer versions keep profiles under `.config`, and older ones in `.mozilla`.
        vec![
            home.join(".config/mozilla/firefox"),
            home.join(".mozilla/firefox"),
        ]
    }
}

fn subfolders(folder: &Path) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    folders.sort();
    folders
}

fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Finds browser profiles with something to import.
pub fn find_sources(home: &Path) -> Vec<Source> {
    let mut sources = Vec::new();
    for (browser, folder) in chromium_folders(home) {
        for profile in subfolders(&folder) {
            if profile.join("Bookmarks").is_file() || profile.join("History").is_file() {
                let profile_name = folder_name(&profile);
                sources.push(Source {
                    id: format!("{browser}/{profile_name}"),
                    name: format!("{browser} ({profile_name})"),
                    kind: Kind::Chromium,
                    profile,
                });
            }
        }
    }
    for profile in firefox_folders(home)
        .iter()
        .flat_map(|folder| subfolders(folder))
    {
        if profile.join("places.sqlite").is_file() {
            let profile_name = folder_name(&profile);
            sources.push(Source {
                id: format!("Firefox/{profile_name}"),
                name: format!("Firefox ({profile_name})"),
                kind: Kind::Firefox,
                profile,
            });
        }
    }
    sources
}

/// Reads a profile. Parts that are missing or damaged are skipped, so a problem with
/// one doesn't lose the other.
pub fn read_source(source: &Source) -> Imported {
    let (bookmarks, history) = match source.kind {
        Kind::Chromium => (
            read_chromium_bookmarks(&source.profile.join("Bookmarks")),
            with_copy(&source.profile.join("History"), read_chromium_history),
        ),
        Kind::Firefox => {
            let places = source.profile.join("places.sqlite");
            (
                with_copy(&places, read_firefox_bookmarks),
                with_copy(&places, read_firefox_history),
            )
        }
    };
    Imported {
        bookmarks: keep_web_items(bookmarks),
        history: keep_web_items(history),
    }
}

fn keep_web_items(items: Vec<Item>) -> Vec<Item> {
    items
        .into_iter()
        .filter(|item| item.url.starts_with("http://") || item.url.starts_with("https://"))
        .collect()
}

/// Opens a private copy, because a running browser keeps its database locked and must
/// not be disturbed.
fn with_copy(
    database: &Path,
    read: impl FnOnce(&Connection) -> rusqlite::Result<Vec<Item>>,
) -> Vec<Item> {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let folder =
        std::env::temp_dir().join(format!("browser-import-{}-{serial}", std::process::id()));
    let items = (|| -> Option<Vec<Item>> {
        fs::create_dir_all(&folder).ok()?;
        let copy = folder.join("copy.sqlite");
        fs::copy(database, &copy).ok()?;
        // Recent changes can still be in the write-ahead log.
        let wal = database.with_file_name(format!("{}-wal", folder_name(database)));
        if wal.is_file() {
            fs::copy(wal, folder.join("copy.sqlite-wal")).ok()?;
        }
        let connection = Connection::open(&copy).ok()?;
        read(&connection).ok()
    })();
    let _ = fs::remove_dir_all(&folder);
    items.unwrap_or_default()
}

fn read_chromium_bookmarks(path: &Path) -> Vec<Item> {
    let Some(json) = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    else {
        return Vec::new();
    };
    let mut items = Vec::new();
    if let Some(roots) = json.get("roots").and_then(|roots| roots.as_object()) {
        for root in roots.values() {
            collect_chromium_bookmarks(root, &mut items);
        }
    }
    items
}

fn collect_chromium_bookmarks(node: &serde_json::Value, items: &mut Vec<Item>) {
    let text = |key: &str| {
        node.get(key)
            .and_then(|value| value.as_str())
            .unwrap_or_default()
    };
    if text("type") == "url" {
        items.push(Item {
            url: text("url").to_owned(),
            title: text("name").to_owned(),
            time_ms: chromium_time_ms(text("date_added").parse().unwrap_or(0)),
            visit_count: 0,
        });
    }
    if let Some(children) = node
        .get("children")
        .and_then(|children| children.as_array())
    {
        for child in children {
            collect_chromium_bookmarks(child, items);
        }
    }
}

fn chromium_time_ms(microseconds: i64) -> i64 {
    if microseconds <= 0 {
        return 0;
    }
    (microseconds - CHROMIUM_EPOCH_OFFSET_US).max(0) / 1000
}

fn read_chromium_history(connection: &Connection) -> rusqlite::Result<Vec<Item>> {
    let mut statement = connection.prepare(
        "SELECT url, COALESCE(title, ''), last_visit_time, visit_count FROM urls
         WHERE hidden = 0 AND visit_count > 0 ORDER BY last_visit_time DESC LIMIT ?1",
    )?;
    statement
        .query_map([HISTORY_LIMIT], |row| {
            Ok(Item {
                url: row.get(0)?,
                title: row.get(1)?,
                time_ms: chromium_time_ms(row.get(2)?),
                visit_count: row.get(3)?,
            })
        })?
        .collect()
}

fn read_firefox_bookmarks(connection: &Connection) -> rusqlite::Result<Vec<Item>> {
    let mut statement = connection.prepare(
        "SELECT p.url, COALESCE(b.title, p.title, ''), COALESCE(b.dateAdded, 0)
         FROM moz_bookmarks b JOIN moz_places p ON b.fk = p.id WHERE b.type = 1",
    )?;
    statement
        .query_map([], |row| {
            Ok(Item {
                url: row.get(0)?,
                title: row.get(1)?,
                time_ms: row.get::<_, i64>(2)? / 1000,
                visit_count: 0,
            })
        })?
        .collect()
}

fn read_firefox_history(connection: &Connection) -> rusqlite::Result<Vec<Item>> {
    let mut statement = connection.prepare(
        "SELECT url, COALESCE(title, ''), last_visit_date, visit_count FROM moz_places
         WHERE hidden = 0 AND visit_count > 0 AND last_visit_date IS NOT NULL
         ORDER BY last_visit_date DESC LIMIT ?1",
    )?;
    statement
        .query_map([HISTORY_LIMIT], |row| {
            Ok(Item {
                url: row.get(0)?,
                title: row.get(1)?,
                time_ms: row.get::<_, i64>(2)? / 1000,
                visit_count: row.get(3)?,
            })
        })?
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempHome(PathBuf);

    impl TempHome {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("import-test-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempHome {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const CHROME_BOOKMARKS: &str = r#"{"roots":{
        "bookmark_bar":{"type":"folder","children":[
            {"type":"url","name":"Rust","url":"https://rust-lang.org/","date_added":"13300000000000000"},
            {"type":"folder","name":"Nested","children":[
                {"type":"url","name":"Inner","url":"http://inner.example/","date_added":"0"},
                {"type":"url","name":"Script","url":"javascript:alert(1)","date_added":"0"}
            ]}
        ]},
        "other":{"type":"folder","children":[{"type":"url","name":"Other","url":"chrome://settings","date_added":"0"}]}
    }}"#;

    fn chrome_profile(home: &Path) -> PathBuf {
        let profile = chromium_folders(home)[0].1.join("Default");
        fs::create_dir_all(&profile).unwrap();
        fs::write(profile.join("Bookmarks"), CHROME_BOOKMARKS).unwrap();
        let history = Connection::open(profile.join("History")).unwrap();
        history
            .execute_batch(
                "CREATE TABLE urls (url TEXT, title TEXT, visit_count INTEGER, last_visit_time INTEGER, hidden INTEGER);
                 INSERT INTO urls VALUES ('https://a.com/', 'A', 3, 13300000000000000, 0);
                 INSERT INTO urls VALUES ('https://hidden.com/', 'H', 3, 13300000000000000, 1);
                 INSERT INTO urls VALUES ('https://never.com/', 'N', 0, 0, 0);
                 INSERT INTO urls VALUES ('file:///secret', 'F', 1, 13300000000000000, 0);",
            )
            .unwrap();
        profile
    }

    fn firefox_profile(home: &Path) -> PathBuf {
        let profile = firefox_folders(home)[0].join("abc.default");
        fs::create_dir_all(&profile).unwrap();
        let places = Connection::open(profile.join("places.sqlite")).unwrap();
        places
            .execute_batch(
                "CREATE TABLE moz_places (id INTEGER PRIMARY KEY, url TEXT, title TEXT, visit_count INTEGER, hidden INTEGER, last_visit_date INTEGER);
                 CREATE TABLE moz_bookmarks (id INTEGER PRIMARY KEY, type INTEGER, fk INTEGER, title TEXT, dateAdded INTEGER);
                 INSERT INTO moz_places VALUES (1, 'https://fx.example/', 'Page', 5, 0, 1700000000000000);
                 INSERT INTO moz_places VALUES (2, 'https://hidden.example/', 'H', 5, 1, 1700000000000000);
                 INSERT INTO moz_places VALUES (3, 'place:parent=x', 'Query', 0, 0, NULL);
                 INSERT INTO moz_bookmarks VALUES (1, 1, 1, 'Saved', 1600000000000000);
                 INSERT INTO moz_bookmarks VALUES (2, 2, NULL, 'Folder', 0);
                 INSERT INTO moz_bookmarks VALUES (3, 1, 3, 'Smart', 0);",
            )
            .unwrap();
        profile
    }

    #[test]
    fn finds_profiles_with_data_only() {
        let home = TempHome::new("find");
        assert!(find_sources(&home.0).is_empty());
        chrome_profile(&home.0);
        firefox_profile(&home.0);
        fs::create_dir_all(chromium_folders(&home.0)[0].1.join("Crash Reports")).unwrap();
        fs::create_dir_all(firefox_folders(&home.0)[0].join("Crash Reports")).unwrap();
        let names: Vec<_> = find_sources(&home.0)
            .into_iter()
            .map(|source| source.name)
            .collect();
        assert_eq!(names, ["Google Chrome (Default)", "Firefox (abc.default)"]);
    }

    #[test]
    fn reads_chromium_bookmarks_in_folders_and_only_web_addresses() {
        let home = TempHome::new("chrome-bookmarks");
        chrome_profile(&home.0);
        let imported = read_source(&find_sources(&home.0)[0]);
        let urls: Vec<_> = imported
            .bookmarks
            .iter()
            .map(|item| item.url.as_str())
            .collect();
        assert_eq!(urls, ["https://rust-lang.org/", "http://inner.example/"]);
        assert_eq!(imported.bookmarks[0].title, "Rust");
        // 13300000000000000 microseconds after 1601 is 2022.
        assert_eq!(imported.bookmarks[0].time_ms, 1_655_526_400_000);
    }

    #[test]
    fn reads_chromium_history_without_hidden_entries() {
        let home = TempHome::new("chrome-history");
        chrome_profile(&home.0);
        let imported = read_source(&find_sources(&home.0)[0]);
        assert_eq!(imported.history.len(), 1);
        assert_eq!(imported.history[0].url, "https://a.com/");
        assert_eq!(imported.history[0].visit_count, 3);
    }

    #[test]
    fn reads_firefox_data() {
        let home = TempHome::new("firefox");
        firefox_profile(&home.0);
        let imported = read_source(&find_sources(&home.0)[0]);
        assert_eq!(imported.bookmarks.len(), 1);
        assert_eq!(imported.bookmarks[0].title, "Saved");
        assert_eq!(imported.bookmarks[0].time_ms, 1_600_000_000_000);
        assert_eq!(imported.history.len(), 1);
        assert_eq!(imported.history[0].url, "https://fx.example/");
        assert_eq!(imported.history[0].time_ms, 1_700_000_000_000);
    }

    #[test]
    fn damaged_files_import_nothing_rather_than_failing() {
        let home = TempHome::new("damaged");
        let profile = chrome_profile(&home.0);
        fs::write(profile.join("Bookmarks"), "not json").unwrap();
        fs::write(profile.join("History"), "not a database").unwrap();
        let imported = read_source(&find_sources(&home.0)[0]);
        assert!(imported.bookmarks.is_empty());
        assert!(imported.history.is_empty());
    }

    #[test]
    fn the_original_database_is_not_changed() {
        let home = TempHome::new("untouched");
        let profile = firefox_profile(&home.0);
        let before = fs::read(profile.join("places.sqlite")).unwrap();
        read_source(&find_sources(&home.0)[0]);
        assert_eq!(fs::read(profile.join("places.sqlite")).unwrap(), before);
    }

    #[test]
    fn sources_never_expose_their_paths_to_the_page() {
        let home = TempHome::new("paths");
        firefox_profile(&home.0);
        let json = serde_json::to_string(&find_sources(&home.0)).unwrap();
        assert!(!json.contains("mozilla"));
    }
}
