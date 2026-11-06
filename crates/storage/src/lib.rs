//! The browser's saved data: bookmarks, history and settings, in one SQLite file.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

pub use rusqlite::Result;
use serde::Serialize;

const MIGRATIONS: [&str; 3] = [
    "
    CREATE TABLE bookmarks (
        url TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        created_ms INTEGER NOT NULL
    );
    CREATE TABLE history (
        url TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        last_visit_ms INTEGER NOT NULL,
        visit_count INTEGER NOT NULL
    );
    CREATE INDEX history_by_time ON history (last_visit_ms DESC);
    CREATE TABLE settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
",
    "
    CREATE TABLE bookmark_folders (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL
    );
    ALTER TABLE bookmarks ADD COLUMN folder_id INTEGER;
    CREATE TABLE downloads (
        path TEXT PRIMARY KEY,
        file_name TEXT NOT NULL,
        state TEXT NOT NULL,
        received_bytes INTEGER NOT NULL,
        total_bytes INTEGER NOT NULL,
        finished_ms INTEGER NOT NULL
    );
",
    // Saved logins. The passwords themselves are in the system's password store.
    "
    CREATE TABLE logins (
        origin TEXT NOT NULL,
        username TEXT NOT NULL,
        created_ms INTEGER NOT NULL,
        used_ms INTEGER NOT NULL,
        PRIMARY KEY (origin, username)
    );
    CREATE TABLE never_save_logins (
        origin TEXT PRIMARY KEY
    );
",
];

/// How many saved downloads to show after a restart.
const DOWNLOAD_RECORD_LIMIT: u32 = 50;
/// How many matches of each kind to consider when ranking suggestions.
const SUGGESTION_CANDIDATES: u32 = 200;

/// A saved login, without its password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Login {
    pub origin: String,
    pub username: String,
    pub used_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    pub url: String,
    pub title: String,
    pub created_ms: i64,
    pub folder_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BookmarkFolder {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub url: String,
    pub title: String,
    pub is_bookmark: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadRecord {
    pub path: String,
    pub file_name: String,
    pub state: String,
    pub received_bytes: i64,
    pub total_bytes: i64,
    pub finished_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub last_visit_ms: i64,
    pub visit_count: i64,
}

pub struct Storage {
    connection: Connection,
}

impl Storage {
    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        Self::with_connection(connection)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::with_connection(Connection::open_in_memory()?)
    }

    fn with_connection(connection: Connection) -> Result<Self> {
        let storage = Self { connection };
        storage.migrate()?;
        Ok(storage)
    }

    fn migrate(&self) -> Result<()> {
        let version: usize = self
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))?;
        for (index, migration) in MIGRATIONS.iter().enumerate().skip(version) {
            self.connection.execute_batch(&format!(
                "BEGIN; {migration} PRAGMA user_version = {}; COMMIT;",
                index + 1
            ))?;
        }
        Ok(())
    }

    pub fn add_bookmark(&self, url: &str, title: &str, now_ms: i64) -> Result<()> {
        self.connection.execute(
            "INSERT INTO bookmarks (url, title, created_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT (url) DO UPDATE SET title = excluded.title",
            params![url, title, now_ms],
        )?;
        Ok(())
    }

    pub fn remove_bookmark(&self, url: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM bookmarks WHERE url = ?1", [url])?;
        Ok(())
    }

    pub fn is_bookmarked(&self, url: &str) -> Result<bool> {
        self.connection
            .query_row("SELECT 1 FROM bookmarks WHERE url = ?1", [url], |_| Ok(()))
            .optional()
            .map(|found| found.is_some())
    }

    /// Newest first, matching the query in the title or URL. An empty query matches all.
    pub fn bookmarks(&self, query: &str, limit: u32) -> Result<Vec<Bookmark>> {
        let mut statement = self.connection.prepare(
            "SELECT url, title, created_ms, folder_id FROM bookmarks
             WHERE title LIKE ?1 ESCAPE '\\' OR url LIKE ?1 ESCAPE '\\'
             ORDER BY created_ms DESC, url LIMIT ?2",
        )?;
        statement
            .query_map(params![like_pattern(query), limit], |row| {
                Ok(Bookmark {
                    url: row.get(0)?,
                    title: row.get(1)?,
                    created_ms: row.get(2)?,
                    folder_id: row.get(3)?,
                })
            })?
            .collect()
    }

    pub fn record_visit(&self, url: &str, title: &str, now_ms: i64) -> Result<()> {
        self.connection.execute(
            "INSERT INTO history (url, title, last_visit_ms, visit_count) VALUES (?1, ?2, ?3, 1)
             ON CONFLICT (url) DO UPDATE SET
                 title = CASE excluded.title WHEN '' THEN title ELSE excluded.title END,
                 last_visit_ms = excluded.last_visit_ms,
                 visit_count = visit_count + 1",
            params![url, title, now_ms],
        )?;
        Ok(())
    }

    pub fn set_history_title(&self, url: &str, title: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE history SET title = ?2 WHERE url = ?1",
            params![url, title],
        )?;
        Ok(())
    }

    /// Most recent first, matching the query in the title or URL. An empty query matches all.
    pub fn history(&self, query: &str, limit: u32) -> Result<Vec<HistoryEntry>> {
        let mut statement = self.connection.prepare(
            "SELECT url, title, last_visit_ms, visit_count FROM history
             WHERE title LIKE ?1 ESCAPE '\\' OR url LIKE ?1 ESCAPE '\\'
             ORDER BY last_visit_ms DESC, url LIMIT ?2",
        )?;
        statement
            .query_map(params![like_pattern(query), limit], |row| {
                Ok(HistoryEntry {
                    url: row.get(0)?,
                    title: row.get(1)?,
                    last_visit_ms: row.get(2)?,
                    visit_count: row.get(3)?,
                })
            })?
            .collect()
    }

    pub fn delete_history_entry(&self, url: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM history WHERE url = ?1", [url])?;
        Ok(())
    }

    pub fn clear_history(&self) -> Result<()> {
        self.connection.execute("DELETE FROM history", [])?;
        Ok(())
    }

    /// Adds bookmarks from another browser and returns how many were new. Existing
    /// bookmarks keep their own titles.
    pub fn import_bookmarks<'a>(
        &self,
        items: impl IntoIterator<Item = (&'a str, &'a str, i64)>,
    ) -> Result<usize> {
        let transaction = self.connection.unchecked_transaction()?;
        let mut added = 0;
        {
            let mut statement = transaction.prepare(
                "INSERT OR IGNORE INTO bookmarks (url, title, created_ms) VALUES (?1, ?2, ?3)",
            )?;
            for (url, title, time_ms) in items {
                added += statement.execute(params![url, title, time_ms])?;
            }
        }
        transaction.commit()?;
        Ok(added)
    }

    /// Merges history from another browser and returns how many addresses were new.
    /// An address already here keeps the larger visit count, so importing twice doesn't
    /// inflate it, and the more recent visit time and its title.
    pub fn import_history<'a>(
        &self,
        items: impl IntoIterator<Item = (&'a str, &'a str, i64, i64)>,
    ) -> Result<usize> {
        let transaction = self.connection.unchecked_transaction()?;
        let before: i64 =
            transaction.query_row("SELECT COUNT(*) FROM history", [], |row| row.get(0))?;
        {
            let mut statement = transaction.prepare(
                "INSERT INTO history (url, title, last_visit_ms, visit_count) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (url) DO UPDATE SET
                     title = CASE WHEN excluded.last_visit_ms > last_visit_ms AND excluded.title != ''
                                  THEN excluded.title ELSE title END,
                     last_visit_ms = MAX(last_visit_ms, excluded.last_visit_ms),
                     visit_count = MAX(visit_count, excluded.visit_count)",
            )?;
            for (url, title, time_ms, visits) in items {
                statement.execute(params![url, title, time_ms, visits])?;
            }
        }
        let after: i64 =
            transaction.query_row("SELECT COUNT(*) FROM history", [], |row| row.get(0))?;
        transaction.commit()?;
        Ok((after - before) as usize)
    }

    /// Changes a bookmark. Returns false if there's no such bookmark, or the new
    /// address already belongs to another one.
    pub fn update_bookmark(
        &self,
        url: &str,
        new_url: &str,
        title: &str,
        folder_id: Option<i64>,
    ) -> Result<bool> {
        if url != new_url && self.is_bookmarked(new_url)? {
            return Ok(false);
        }
        let changed = self.connection.execute(
            "UPDATE bookmarks SET url = ?2, title = ?3, folder_id = ?4 WHERE url = ?1",
            params![url, new_url, title, folder_id],
        )?;
        Ok(changed > 0)
    }

    pub fn bookmark_folders(&self) -> Result<Vec<BookmarkFolder>> {
        let mut statement = self
            .connection
            .prepare("SELECT id, name FROM bookmark_folders ORDER BY name COLLATE NOCASE, id")?;
        statement
            .query_map([], |row| {
                Ok(BookmarkFolder {
                    id: row.get(0)?,
                    name: row.get(1)?,
                })
            })?
            .collect()
    }

    pub fn create_bookmark_folder(&self, name: &str) -> Result<i64> {
        self.connection
            .execute("INSERT INTO bookmark_folders (name) VALUES (?1)", [name])?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn rename_bookmark_folder(&self, id: i64, name: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE bookmark_folders SET name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        Ok(())
    }

    /// Deletes a folder. Its bookmarks stay, moved out of the folder.
    pub fn delete_bookmark_folder(&self, id: i64) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "UPDATE bookmarks SET folder_id = NULL WHERE folder_id = ?1",
            [id],
        )?;
        transaction.execute("DELETE FROM bookmark_folders WHERE id = ?1", [id])?;
        transaction.commit()
    }

    /// Bookmarks and history matching what's typed, best first.
    pub fn suggestions(&self, query: &str, limit: usize) -> Result<Vec<Suggestion>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let mut scored: Vec<(i64, Suggestion)> = Vec::new();
        for bookmark in self.bookmarks(query, SUGGESTION_CANDIDATES)? {
            let score = match_score(query, &bookmark.url, &bookmark.title) + BOOKMARK_BONUS;
            scored.push((
                score,
                Suggestion {
                    url: bookmark.url,
                    title: bookmark.title,
                    is_bookmark: true,
                },
            ));
        }
        for entry in self.history_by_visits(query)? {
            if scored.iter().any(|(_, item)| item.url == entry.url) {
                continue;
            }
            let score = match_score(query, &entry.url, &entry.title)
                + entry.visit_count.min(MAX_VISIT_BONUS);
            scored.push((
                score,
                Suggestion {
                    url: entry.url,
                    title: entry.title,
                    is_bookmark: false,
                },
            ));
        }
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| a.1.url.len().cmp(&b.1.url.len()))
        });
        scored.truncate(limit);
        Ok(scored.into_iter().map(|(_, item)| item).collect())
    }

    fn history_by_visits(&self, query: &str) -> Result<Vec<HistoryEntry>> {
        let mut statement = self.connection.prepare(
            "SELECT url, title, last_visit_ms, visit_count FROM history
             WHERE title LIKE ?1 ESCAPE '\\' OR url LIKE ?1 ESCAPE '\\'
             ORDER BY visit_count DESC, last_visit_ms DESC LIMIT ?2",
        )?;
        statement
            .query_map(params![like_pattern(query), SUGGESTION_CANDIDATES], |row| {
                Ok(HistoryEntry {
                    url: row.get(0)?,
                    title: row.get(1)?,
                    last_visit_ms: row.get(2)?,
                    visit_count: row.get(3)?,
                })
            })?
            .collect()
    }

    /// Saves a finished download, replacing any earlier record for the same file.
    pub fn save_download(&self, record: &DownloadRecord) -> Result<()> {
        self.connection.execute(
            "INSERT OR REPLACE INTO downloads
             (path, file_name, state, received_bytes, total_bytes, finished_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                record.path,
                record.file_name,
                record.state,
                record.received_bytes,
                record.total_bytes,
                record.finished_ms
            ],
        )?;
        Ok(())
    }

    /// Newest first.
    pub fn downloads(&self) -> Result<Vec<DownloadRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT path, file_name, state, received_bytes, total_bytes, finished_ms
             FROM downloads ORDER BY finished_ms DESC, path LIMIT ?1",
        )?;
        statement
            .query_map([DOWNLOAD_RECORD_LIMIT], |row| {
                Ok(DownloadRecord {
                    path: row.get(0)?,
                    file_name: row.get(1)?,
                    state: row.get(2)?,
                    received_bytes: row.get(3)?,
                    total_bytes: row.get(4)?,
                    finished_ms: row.get(5)?,
                })
            })?
            .collect()
    }

    pub fn remove_download(&self, path: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM downloads WHERE path = ?1", [path])?;
        Ok(())
    }

    pub fn clear_downloads(&self) -> Result<()> {
        self.connection.execute("DELETE FROM downloads", [])?;
        Ok(())
    }

    /// Records a saved login, or marks an existing one as just used.
    pub fn save_login(&self, origin: &str, username: &str, now_ms: i64) -> Result<()> {
        self.connection.execute(
            "INSERT INTO logins (origin, username, created_ms, used_ms) VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT (origin, username) DO UPDATE SET used_ms = excluded.used_ms",
            params![origin, username, now_ms],
        )?;
        Ok(())
    }

    /// One site's logins, most recently used first. Every site's with `None`.
    pub fn logins(&self, origin: Option<&str>) -> Result<Vec<Login>> {
        let mut statement = self.connection.prepare(
            "SELECT origin, username, used_ms FROM logins
             WHERE ?1 IS NULL OR origin = ?1
             ORDER BY used_ms DESC, origin, username",
        )?;
        statement
            .query_map([origin], |row| {
                Ok(Login {
                    origin: row.get(0)?,
                    username: row.get(1)?,
                    used_ms: row.get(2)?,
                })
            })?
            .collect()
    }

    pub fn delete_login(&self, origin: &str, username: &str) -> Result<()> {
        self.connection.execute(
            "DELETE FROM logins WHERE origin = ?1 AND username = ?2",
            params![origin, username],
        )?;
        Ok(())
    }

    pub fn set_never_save_login(&self, origin: &str, never: bool) -> Result<()> {
        if never {
            self.connection.execute(
                "INSERT OR IGNORE INTO never_save_logins (origin) VALUES (?1)",
                [origin],
            )?;
        } else {
            self.connection
                .execute("DELETE FROM never_save_logins WHERE origin = ?1", [origin])?;
        }
        Ok(())
    }

    pub fn never_saves_logins(&self, origin: &str) -> Result<bool> {
        self.connection
            .query_row(
                "SELECT 1 FROM never_save_logins WHERE origin = ?1",
                [origin],
                |_| Ok(()),
            )
            .optional()
            .map(|found| found.is_some())
    }

    pub fn never_save_login_sites(&self) -> Result<Vec<String>> {
        let mut statement = self
            .connection
            .prepare("SELECT origin FROM never_save_logins ORDER BY origin")?;
        statement.query_map([], |row| row.get(0))?.collect()
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        self.connection
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.connection.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

const BOOKMARK_BONUS: i64 = 40;
const MAX_VISIT_BONUS: i64 = 30;

/// Scores how well typed text matches. An address that starts with the text is what
/// people usually mean, so it ranks above a match somewhere inside a title.
fn match_score(query: &str, url: &str, title: &str) -> i64 {
    let query = query.to_lowercase();
    let url = url.to_lowercase();
    let address = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(&url);
    let host_start = address.strip_prefix("www.").unwrap_or(address);
    if host_start.starts_with(&query) || address.starts_with(&query) || url.starts_with(&query) {
        100
    } else if title.to_lowercase().starts_with(&query) {
        60
    } else if url.contains(&query) {
        20
    } else {
        10
    }
}

fn like_pattern(query: &str) -> String {
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn storage() -> Storage {
        Storage::open_in_memory().unwrap()
    }

    #[test]
    fn logins_are_saved_listed_and_removed() {
        let storage = storage();
        storage.save_login("https://a.example", "ann", 1).unwrap();
        storage.save_login("https://a.example", "bob", 2).unwrap();
        storage.save_login("https://b.example", "cat", 3).unwrap();
        let a = storage.logins(Some("https://a.example")).unwrap();
        assert_eq!(
            a.iter()
                .map(|login| login.username.as_str())
                .collect::<Vec<_>>(),
            ["bob", "ann"]
        );
        // Using a login moves it first.
        storage.save_login("https://a.example", "ann", 4).unwrap();
        assert_eq!(
            storage.logins(Some("https://a.example")).unwrap()[0].username,
            "ann"
        );
        assert_eq!(storage.logins(None).unwrap().len(), 3);
        storage.delete_login("https://a.example", "ann").unwrap();
        assert_eq!(storage.logins(Some("https://a.example")).unwrap().len(), 1);
    }

    #[test]
    fn sites_can_be_told_never_to_save() {
        let storage = storage();
        assert!(!storage.never_saves_logins("https://a.example").unwrap());
        storage
            .set_never_save_login("https://a.example", true)
            .unwrap();
        storage
            .set_never_save_login("https://a.example", true)
            .unwrap();
        assert!(storage.never_saves_logins("https://a.example").unwrap());
        assert_eq!(
            storage.never_save_login_sites().unwrap(),
            ["https://a.example"]
        );
        storage
            .set_never_save_login("https://a.example", false)
            .unwrap();
        assert!(!storage.never_saves_logins("https://a.example").unwrap());
    }

    #[test]
    fn bookmarks_can_be_added_found_and_removed() {
        let storage = storage();
        assert!(!storage.is_bookmarked("https://a.com").unwrap());
        storage.add_bookmark("https://a.com", "A", 1).unwrap();
        assert!(storage.is_bookmarked("https://a.com").unwrap());
        storage.remove_bookmark("https://a.com").unwrap();
        assert!(!storage.is_bookmarked("https://a.com").unwrap());
    }

    #[test]
    fn bookmarking_again_updates_the_title() {
        let storage = storage();
        storage.add_bookmark("https://a.com", "Old", 1).unwrap();
        storage.add_bookmark("https://a.com", "New", 2).unwrap();
        let bookmarks = storage.bookmarks("", 10).unwrap();
        assert_eq!(bookmarks.len(), 1);
        assert_eq!(bookmarks[0].title, "New");
        assert_eq!(bookmarks[0].created_ms, 1);
    }

    #[test]
    fn newest_bookmarks_come_first() {
        let storage = storage();
        storage.add_bookmark("https://old.com", "Old", 1).unwrap();
        storage.add_bookmark("https://new.com", "New", 2).unwrap();
        assert_eq!(storage.bookmarks("", 10).unwrap()[0].title, "New");
        assert_eq!(storage.bookmarks("", 1).unwrap().len(), 1);
    }

    #[test]
    fn visits_are_counted_and_keep_their_title() {
        let storage = storage();
        storage.record_visit("https://a.com", "", 1).unwrap();
        storage.set_history_title("https://a.com", "A").unwrap();
        storage.record_visit("https://a.com", "", 5).unwrap();
        let history = storage.history("", 10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].visit_count, 2);
        assert_eq!(history[0].last_visit_ms, 5);
        assert_eq!(history[0].title, "A");
    }

    #[test]
    fn most_recent_visits_come_first() {
        let storage = storage();
        storage.record_visit("https://a.com", "", 1).unwrap();
        storage.record_visit("https://b.com", "", 2).unwrap();
        storage.record_visit("https://a.com", "", 3).unwrap();
        assert_eq!(storage.history("", 10).unwrap()[0].url, "https://a.com");
    }

    #[test]
    fn a_visit_without_a_title_keeps_the_saved_one() {
        let storage = storage();
        storage.record_visit("https://a.com", "A", 1).unwrap();
        storage.record_visit("https://a.com", "", 2).unwrap();
        assert_eq!(storage.history("", 10).unwrap()[0].title, "A");
        storage.record_visit("https://a.com", "New", 3).unwrap();
        assert_eq!(storage.history("", 10).unwrap()[0].title, "New");
    }

    #[test]
    fn search_matches_titles_and_urls() {
        let storage = storage();
        storage
            .record_visit("https://rust-lang.org", "", 1)
            .unwrap();
        storage.record_visit("https://b.com", "", 2).unwrap();
        storage
            .set_history_title("https://b.com", "Rust news")
            .unwrap();
        storage.record_visit("https://c.com", "", 3).unwrap();
        assert_eq!(storage.history("rust", 10).unwrap().len(), 2);
        assert_eq!(storage.history("RUST", 10).unwrap().len(), 2);
    }

    #[test]
    fn search_text_is_literal() {
        let storage = storage();
        storage.record_visit("https://a.com/100%", "", 1).unwrap();
        storage.record_visit("https://b.com/a_b", "", 2).unwrap();
        storage.record_visit("https://c.com", "", 3).unwrap();
        assert_eq!(storage.history("%", 10).unwrap().len(), 1);
        assert_eq!(storage.history("_", 10).unwrap().len(), 1);
        assert_eq!(storage.history("' OR 1=1 --", 10).unwrap().len(), 0);
    }

    #[test]
    fn history_entries_can_be_deleted_and_cleared() {
        let storage = storage();
        storage.record_visit("https://a.com", "", 1).unwrap();
        storage.record_visit("https://b.com", "", 2).unwrap();
        storage.delete_history_entry("https://a.com").unwrap();
        assert_eq!(storage.history("", 10).unwrap().len(), 1);
        storage.clear_history().unwrap();
        assert!(storage.history("", 10).unwrap().is_empty());
    }

    #[test]
    fn clearing_history_keeps_bookmarks() {
        let storage = storage();
        storage.add_bookmark("https://a.com", "A", 1).unwrap();
        storage.record_visit("https://a.com", "", 1).unwrap();
        storage.clear_history().unwrap();
        assert!(storage.is_bookmarked("https://a.com").unwrap());
    }

    #[test]
    fn importing_bookmarks_keeps_existing_ones() {
        let storage = storage();
        storage.add_bookmark("https://a.com", "Mine", 1).unwrap();
        let items = [("https://a.com", "Theirs", 2), ("https://b.com", "B", 3)];
        assert_eq!(storage.import_bookmarks(items).unwrap(), 1);
        assert_eq!(storage.import_bookmarks(items).unwrap(), 0);
        let bookmarks = storage.bookmarks("a.com", 10).unwrap();
        assert_eq!(bookmarks[0].title, "Mine");
    }

    #[test]
    fn importing_history_twice_does_not_inflate_it() {
        let storage = storage();
        storage.record_visit("https://a.com", "Mine", 100).unwrap();
        let items = [
            ("https://a.com", "Older", 50, 7),
            ("https://b.com", "B", 60, 2),
        ];
        assert_eq!(storage.import_history(items).unwrap(), 1);
        assert_eq!(storage.import_history(items).unwrap(), 0);
        let history = storage.history("a.com", 10).unwrap();
        assert_eq!(history[0].visit_count, 7);
        assert_eq!(history[0].last_visit_ms, 100);
        assert_eq!(history[0].title, "Mine");
        assert_eq!(storage.history("", 10).unwrap().len(), 2);
    }

    #[test]
    fn bookmarks_can_be_edited_and_filed() {
        let storage = storage();
        storage.add_bookmark("https://a.com", "A", 1).unwrap();
        let folder = storage.create_bookmark_folder("Work").unwrap();
        assert!(
            storage
                .update_bookmark("https://a.com", "https://b.com", "B", Some(folder))
                .unwrap()
        );
        let saved = &storage.bookmarks("", 10).unwrap()[0];
        assert_eq!(saved.url, "https://b.com");
        assert_eq!(saved.title, "B");
        assert_eq!(saved.folder_id, Some(folder));
        assert!(
            !storage
                .update_bookmark("https://none.com", "https://x.com", "X", None)
                .unwrap()
        );
    }

    #[test]
    fn editing_cannot_overwrite_another_bookmark() {
        let storage = storage();
        storage.add_bookmark("https://a.com", "A", 1).unwrap();
        storage.add_bookmark("https://b.com", "B", 2).unwrap();
        assert!(
            !storage
                .update_bookmark("https://a.com", "https://b.com", "A", None)
                .unwrap()
        );
        assert_eq!(storage.bookmarks("", 10).unwrap().len(), 2);
    }

    #[test]
    fn deleting_a_folder_keeps_its_bookmarks() {
        let storage = storage();
        let folder = storage.create_bookmark_folder("Old").unwrap();
        storage.rename_bookmark_folder(folder, "New").unwrap();
        assert_eq!(storage.bookmark_folders().unwrap()[0].name, "New");
        storage.add_bookmark("https://a.com", "A", 1).unwrap();
        storage
            .update_bookmark("https://a.com", "https://a.com", "A", Some(folder))
            .unwrap();
        storage.delete_bookmark_folder(folder).unwrap();
        assert!(storage.bookmark_folders().unwrap().is_empty());
        assert_eq!(storage.bookmarks("", 10).unwrap()[0].folder_id, None);
    }

    #[test]
    fn suggestions_prefer_addresses_that_start_with_the_text() {
        let storage = storage();
        storage
            .record_visit("https://other.com/github", "About github", 1)
            .unwrap();
        storage
            .record_visit("https://github.com/", "GitHub", 2)
            .unwrap();
        let results = storage.suggestions("git", 5).unwrap();
        assert_eq!(results[0].url, "https://github.com/");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn suggestions_ignore_www_and_scheme() {
        let storage = storage();
        storage
            .record_visit("https://www.rust-lang.org/", "Rust", 1)
            .unwrap();
        storage.record_visit("https://a.com/rust", "x", 2).unwrap();
        assert_eq!(
            storage.suggestions("rust-l", 5).unwrap()[0].url,
            "https://www.rust-lang.org/"
        );
    }

    #[test]
    fn suggestions_rank_bookmarks_and_frequent_visits_higher() {
        let storage = storage();
        storage
            .record_visit("https://site.com/rare", "", 1)
            .unwrap();
        for time in 0..5 {
            storage
                .record_visit("https://site.com/often", "", time)
                .unwrap();
        }
        assert_eq!(
            storage.suggestions("site", 5).unwrap()[0].url,
            "https://site.com/often"
        );
        storage
            .add_bookmark("https://site.com/saved", "Saved", 1)
            .unwrap();
        let first = &storage.suggestions("site", 5).unwrap()[0];
        assert_eq!(first.url, "https://site.com/saved");
        assert!(first.is_bookmark);
    }

    #[test]
    fn suggestions_have_no_duplicates_and_respect_the_limit() {
        let storage = storage();
        storage.add_bookmark("https://a.com", "A", 1).unwrap();
        storage.record_visit("https://a.com", "A", 1).unwrap();
        assert_eq!(storage.suggestions("a.com", 5).unwrap().len(), 1);
        for number in 0..9 {
            storage
                .record_visit(&format!("https://a.com/{number}"), "", 1)
                .unwrap();
        }
        assert_eq!(storage.suggestions("a.com", 5).unwrap().len(), 5);
        assert!(storage.suggestions("  ", 5).unwrap().is_empty());
    }

    fn record(path: &str, finished_ms: i64) -> DownloadRecord {
        DownloadRecord {
            path: path.to_owned(),
            file_name: "f".to_owned(),
            state: "complete".to_owned(),
            received_bytes: 1,
            total_bytes: 1,
            finished_ms,
        }
    }

    #[test]
    fn download_records_are_saved_newest_first() {
        let storage = storage();
        storage.save_download(&record("/a", 1)).unwrap();
        storage.save_download(&record("/b", 2)).unwrap();
        storage.save_download(&record("/a", 3)).unwrap();
        let saved = storage.downloads().unwrap();
        assert_eq!(saved.len(), 2);
        assert_eq!(saved[0].path, "/a");
        storage.remove_download("/a").unwrap();
        assert_eq!(storage.downloads().unwrap().len(), 1);
        storage.clear_downloads().unwrap();
        assert!(storage.downloads().unwrap().is_empty());
    }

    #[test]
    fn databases_from_the_first_version_upgrade_and_keep_their_data() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(MIGRATIONS[0]).unwrap();
        connection.pragma_update(None, "user_version", 1).unwrap();
        connection
            .execute("INSERT INTO bookmarks VALUES ('https://a.com', 'A', 1)", [])
            .unwrap();
        let storage = Storage::with_connection(connection).unwrap();
        let saved = storage.bookmarks("", 10).unwrap();
        assert_eq!(saved[0].url, "https://a.com");
        assert_eq!(saved[0].folder_id, None);
    }

    #[test]
    fn settings_are_saved_and_replaced() {
        let storage = storage();
        assert_eq!(storage.setting("k").unwrap(), None);
        storage.set_setting("k", "1").unwrap();
        storage.set_setting("k", "2").unwrap();
        assert_eq!(storage.setting("k").unwrap().as_deref(), Some("2"));
    }

    #[test]
    fn data_survives_reopening_the_file() {
        let folder = std::env::temp_dir().join(format!("rb-storage-test-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("test.db");
        {
            let storage = Storage::open(&path).unwrap();
            storage.add_bookmark("https://a.com", "A", 1).unwrap();
        }
        let storage = Storage::open(&path).unwrap();
        assert!(storage.is_bookmarked("https://a.com").unwrap());
        drop(storage);
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
