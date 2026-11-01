//! The browser's saved data: bookmarks, history and settings, in one SQLite file.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

pub use rusqlite::Result;
use serde::Serialize;

const MIGRATIONS: [&str; 1] = ["
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
"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    pub url: String,
    pub title: String,
    pub created_ms: i64,
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
            "SELECT url, title, created_ms FROM bookmarks
             WHERE title LIKE ?1 ESCAPE '\\' OR url LIKE ?1 ESCAPE '\\'
             ORDER BY created_ms DESC, url LIMIT ?2",
        )?;
        statement
            .query_map(params![like_pattern(query), limit], |row| {
                Ok(Bookmark {
                    url: row.get(0)?,
                    title: row.get(1)?,
                    created_ms: row.get(2)?,
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
