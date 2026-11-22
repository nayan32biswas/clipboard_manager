//! Deletes old history. Pinned items never expire.

use super::db::Store;

pub const DAY_MS: i64 = 24 * 60 * 60 * 1000;

impl Store {
    /// Deletes unpinned rows not used in the last `retention_days`, plus their
    /// image files. Returns how many rows were removed.
    pub fn purge_expired(&self, now_ms: i64, retention_days: u32) -> rusqlite::Result<usize> {
        let cutoff = now_ms - i64::from(retention_days) * DAY_MS;
        self.delete_where("pinned = 0 AND last_used_at < ?1", [cutoff])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{hash_bytes, NewItem};

    const NOW: i64 = 100 * DAY_MS;

    #[test]
    fn removes_only_unpinned_items_older_than_the_limit() {
        let store = Store::open_in_memory().unwrap();
        store.insert(&NewItem::text("old", None), NOW - 2 * DAY_MS).unwrap();
        let (pinned, _) = store.insert(&NewItem::text("old pinned", None), NOW - 5 * DAY_MS).unwrap();
        store.toggle_pin(pinned).unwrap();
        store.insert(&NewItem::text("fresh", None), NOW - DAY_MS / 2).unwrap();

        assert_eq!(store.purge_expired(NOW, 1).unwrap(), 1);
        let left: Vec<_> = store.list(10).unwrap().into_iter().map(|i| i.text.unwrap()).collect();
        assert_eq!(left, ["old pinned", "fresh"]);
        assert!(store.search("old", 10).unwrap().iter().all(|i| i.pinned));
    }

    #[test]
    fn item_exactly_at_the_cutoff_is_kept() {
        let store = Store::open_in_memory().unwrap();
        store.insert(&NewItem::text("edge", None), NOW - 3 * DAY_MS).unwrap();
        assert_eq!(store.purge_expired(NOW, 3).unwrap(), 0);
        assert_eq!(store.list(10).unwrap().len(), 1);
    }

    #[test]
    fn reusing_an_item_resets_its_age() {
        let store = Store::open_in_memory().unwrap();
        store.insert(&NewItem::text("again", None), NOW - 10 * DAY_MS).unwrap();
        store.insert(&NewItem::text("again", None), NOW - 1000).unwrap();
        assert_eq!(store.purge_expired(NOW, 3).unwrap(), 0);
    }

    #[test]
    fn expired_images_lose_their_files_too() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("a.png");
        let thumb = dir.path().join("a.thumb.png");
        std::fs::write(&image, b"png").unwrap();
        std::fs::write(&thumb, b"thumb").unwrap();
        let store = Store::open_in_memory().unwrap();
        store
            .insert(&NewItem::image(&image, &thumb, hash_bytes(b"a"), None), NOW - 4 * DAY_MS)
            .unwrap();

        assert_eq!(store.purge_expired(NOW, 3).unwrap(), 1);
        assert!(!image.exists());
        assert!(!thumb.exists());
    }
}
