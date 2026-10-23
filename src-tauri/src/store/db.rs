use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, Row};
use sha2::{Digest, Sha256};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS items (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('text', 'image')),
    text TEXT,
    image_path TEXT,
    thumb_path TEXT,
    hash TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL,
    last_used_at INTEGER NOT NULL,
    pinned INTEGER NOT NULL DEFAULT 0,
    source_app TEXT
);
CREATE VIRTUAL TABLE IF NOT EXISTS items_fts
    USING fts5(text, content='items', content_rowid='id', tokenize='trigram');
CREATE TRIGGER IF NOT EXISTS items_ai AFTER INSERT ON items BEGIN
    INSERT INTO items_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER IF NOT EXISTS items_ad AFTER DELETE ON items BEGIN
    INSERT INTO items_fts(items_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;
CREATE TRIGGER IF NOT EXISTS items_au AFTER UPDATE OF text ON items BEGIN
    INSERT INTO items_fts(items_fts, rowid, text) VALUES ('delete', old.id, old.text);
    INSERT INTO items_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT);
";

const ORDER: &str = "ORDER BY pinned DESC, last_used_at DESC, id DESC";

const COLUMNS: &str =
    "id, kind, text, image_path, thumb_path, hash, created_at, last_used_at, pinned, source_app";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Image,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Image => "image",
        }
    }
}

/// A clip ready to be saved. Image files must already be on disk.
#[derive(Debug, Clone, PartialEq)]
pub struct NewItem {
    pub kind: Kind,
    pub text: Option<String>,
    pub image_path: Option<String>,
    pub thumb_path: Option<String>,
    pub hash: String,
    pub source_app: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: i64,
    pub kind: Kind,
    pub text: Option<String>,
    pub image_path: Option<String>,
    pub thumb_path: Option<String>,
    pub hash: String,
    pub created_at: i64,
    pub last_used_at: i64,
    pub pinned: bool,
    pub source_app: Option<String>,
}

pub struct Store {
    conn: Connection,
}

/// sha256 of the clip content, as lowercase hex. Used for dedupe.
pub fn hash_bytes(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
}

impl NewItem {
    pub fn text(text: &str, source_app: Option<&str>) -> Self {
        NewItem {
            kind: Kind::Text,
            text: Some(text.to_owned()),
            image_path: None,
            thumb_path: None,
            hash: hash_bytes(text.as_bytes()),
            source_app: source_app.map(str::to_owned),
        }
    }
}

impl NewItem {
    pub fn image(image_path: &Path, thumb_path: &Path, hash: String, source_app: Option<&str>) -> Self {
        NewItem {
            kind: Kind::Image,
            text: None,
            image_path: Some(image_path.to_string_lossy().into_owned()),
            thumb_path: Some(thumb_path.to_string_lossy().into_owned()),
            hash,
            source_app: source_app.map(str::to_owned),
        }
    }
}

/// Best effort: a file that's already gone is fine.
pub(super) fn remove_files<'a>(paths: impl IntoIterator<Item = &'a Option<String>>) {
    for path in paths.into_iter().flatten() {
        let _ = std::fs::remove_file(path);
    }
}

impl Store {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Self> {
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    /// Saves a clip. A repeat (same hash) moves the existing row to the top
    /// instead. Returns the row id and whether it was new.
    pub fn insert(&self, item: &NewItem, now_ms: i64) -> rusqlite::Result<(i64, bool)> {
        if let Some(id) = self.bump_hash(&item.hash, now_ms)? {
            return Ok((id, false));
        }
        self.conn.execute(
            "INSERT INTO items (kind, text, image_path, thumb_path, hash, created_at, last_used_at, source_app)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7)",
            params![
                item.kind.as_str(),
                item.text,
                item.image_path,
                item.thumb_path,
                item.hash,
                now_ms,
                item.source_app
            ],
        )?;
        Ok((self.conn.last_insert_rowid(), true))
    }

    /// Substring search, case-insensitive. Empty query lists everything.
    pub fn search(&self, query: &str, limit: usize) -> rusqlite::Result<Vec<Item>> {
        if query.trim().is_empty() {
            return self.list(limit);
        }
        // Trigrams need at least 3 characters, so shorter queries use LIKE.
        let (filter, arg) = if query.chars().count() < 3 {
            let escaped = query.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
            ("text LIKE ?1 ESCAPE '\\'", format!("%{escaped}%"))
        } else {
            // A quoted phrase makes FTS treat operators and quotes as plain text.
            (
                "id IN (SELECT rowid FROM items_fts WHERE items_fts MATCH ?1)",
                format!("\"{}\"", query.replace('"', "\"\"")),
            )
        };
        let sql = format!("SELECT {COLUMNS} FROM items WHERE {filter} {ORDER} LIMIT ?2");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![arg, limit as i64], row_to_item)?;
        rows.collect()
    }

    /// Returns the new pinned state, or None if the id doesn't exist.
    pub fn toggle_pin(&self, id: i64) -> rusqlite::Result<Option<bool>> {
        self.conn
            .query_row(
                "UPDATE items SET pinned = 1 - pinned WHERE id = ?1 RETURNING pinned",
                [id],
                |r| r.get(0),
            )
            .optional()
    }

    /// Deletes the row and its image files. Returns false if the id doesn't exist.
    pub fn delete(&self, id: i64) -> rusqlite::Result<bool> {
        let files: Option<(Option<String>, Option<String>)> = self
            .conn
            .query_row(
                "DELETE FROM items WHERE id = ?1 RETURNING image_path, thumb_path",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match files {
            Some((image, thumb)) => {
                remove_files([&image, &thumb]);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Marks an existing item (by hash) as just used. Returns its id.
    pub fn bump_hash(&self, hash: &str, now_ms: i64) -> rusqlite::Result<Option<i64>> {
        self.conn
            .query_row(
                "UPDATE items SET last_used_at = ?2 WHERE hash = ?1 RETURNING id",
                params![hash, now_ms],
                |r| r.get(0),
            )
            .optional()
    }

    pub fn get(&self, id: i64) -> rusqlite::Result<Option<Item>> {
        let sql = format!("SELECT {COLUMNS} FROM items WHERE id = ?1");
        self.conn.query_row(&sql, [id], row_to_item).optional()
    }

    /// Marks an item as just used, moving it to the top.
    pub fn touch(&self, id: i64, now_ms: i64) -> rusqlite::Result<bool> {
        let n = self
            .conn
            .execute("UPDATE items SET last_used_at = ?2 WHERE id = ?1", params![id, now_ms])?;
        Ok(n > 0)
    }

    /// Deletes every unpinned item and its files. Returns how many were removed.
    pub fn clear_unpinned(&self) -> rusqlite::Result<usize> {
        self.delete_where("pinned = 0", [])
    }

    /// Deletes matching rows, then their image files.
    pub(super) fn delete_where(&self, filter: &str, args: impl rusqlite::Params) -> rusqlite::Result<usize> {
        let sql = format!("DELETE FROM items WHERE {filter} RETURNING image_path, thumb_path");
        let mut stmt = self.conn.prepare(&sql)?;
        let files = stmt
            .query_map(args, |r| Ok([r.get(0)?, r.get(1)?]))?
            .collect::<rusqlite::Result<Vec<[Option<String>; 2]>>>()?;
        remove_files(files.iter().flatten());
        Ok(files.len())
    }

    pub fn get_setting(&self, key: &str) -> rusqlite::Result<Option<String>> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Pinned items first, then most recently used.
    pub fn list(&self, limit: usize) -> rusqlite::Result<Vec<Item>> {
        let sql = format!("SELECT {COLUMNS} FROM items {ORDER} LIMIT ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([limit as i64], row_to_item)?;
        rows.collect()
    }
}

fn row_to_item(r: &Row) -> rusqlite::Result<Item> {
    let kind: String = r.get(1)?;
    Ok(Item {
        id: r.get(0)?,
        kind: if kind == "image" { Kind::Image } else { Kind::Text },
        text: r.get(2)?,
        image_path: r.get(3)?,
        thumb_path: r.get(4)?,
        hash: r.get(5)?,
        created_at: r.get(6)?,
        last_used_at: r.get(7)?,
        pinned: r.get(8)?,
        source_app: r.get(9)?,
    })
}

#[cfg(test)]
#[path = "db_tests.rs"]
mod tests;
