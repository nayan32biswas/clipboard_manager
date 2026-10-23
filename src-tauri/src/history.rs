//! The app's backend API. Commands, the tray and the watcher all go
//! through `History`. Nothing here knows about Tauri or the UI.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

use serde::Serialize;

use crate::capture::{self, Clip, Outcome};
use crate::clipboard::ClipboardWriter;
use crate::images;
use crate::settings::Settings;
use crate::store::{Item, Kind, Store};

/// Longest text preview sent to the UI. Rows only show 2 lines.
pub const PREVIEW_CHARS: usize = 400;
pub const LIST_LIMIT: usize = 200;

pub fn now_ms() -> i64 {
    let since_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    since_epoch.as_millis() as i64
}

/// One row as the UI sees it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemView {
    pub id: i64,
    pub kind: &'static str,
    pub preview: Option<String>,
    /// Thumbnail as a data URL, for images.
    pub thumb: Option<String>,
    pub pinned: bool,
    pub last_used_at: i64,
    pub source_app: Option<String>,
}

pub struct History {
    store: Mutex<Store>,
    settings: Mutex<Settings>,
    images_dir: PathBuf,
    paused: AtomicBool,
    writer: Box<dyn ClipboardWriter>,
}

impl History {
    pub fn new(store: Store, images_dir: PathBuf, writer: Box<dyn ClipboardWriter>) -> crate::Result<Self> {
        let settings = Settings::load(&store)?;
        Ok(History {
            store: Mutex::new(store),
            settings: Mutex::new(settings),
            images_dir,
            paused: AtomicBool::new(false),
            writer,
        })
    }

    fn store(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap()
    }

    pub fn search(&self, query: &str) -> crate::Result<Vec<ItemView>> {
        let items = self.store().search(query, LIST_LIMIT)?;
        Ok(items.into_iter().map(to_view).collect())
    }

    /// Puts the item back on the clipboard and moves it to the top.
    pub fn copy_item(&self, id: i64, now_ms: i64) -> crate::Result<()> {
        let item = self.store().get(id)?.ok_or("item not found")?;
        match (item.kind, &item.text, &item.image_path) {
            (Kind::Text, Some(text), _) => {
                self.writer.set_text(text)?;
                // A failure here shouldn't undo the normal copy.
                if self.settings().primary_selection {
                    if let Err(e) = self.writer.set_primary_text(text) {
                        eprintln!("primary selection: {e}");
                    }
                }
            }
            (Kind::Image, _, Some(path)) => self.writer.set_image_png(Path::new(path))?,
            _ => return Err("item has no content".into()),
        }
        self.store().touch(id, now_ms)?;
        Ok(())
    }

    pub fn toggle_pin(&self, id: i64) -> crate::Result<bool> {
        Ok(self.store().toggle_pin(id)?.ok_or("item not found")?)
    }

    pub fn delete(&self, id: i64) -> crate::Result<()> {
        self.store().delete(id)?;
        Ok(())
    }

    pub fn clear(&self) -> crate::Result<usize> {
        Ok(self.store().clear_unpinned()?)
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    /// Saves and returns the settings as stored (clamped). Applies the new
    /// retention right away.
    pub fn update_settings(&self, new: Settings, now_ms: i64) -> crate::Result<Settings> {
        let saved = {
            let store = self.store();
            new.save(&store)?;
            Settings::load(&store)?
        };
        *self.settings.lock().unwrap() = saved.clone();
        self.purge_expired(now_ms)?;
        Ok(saved)
    }

    pub fn purge_expired(&self, now_ms: i64) -> crate::Result<usize> {
        let days = self.settings.lock().unwrap().retention_days;
        Ok(self.store().purge_expired(now_ms, days)?)
    }

    /// Small UI state (like window size) kept next to the settings.
    pub fn ui_value(&self, key: &str) -> Option<String> {
        self.store().get_setting(&format!("ui.{key}")).ok().flatten()
    }

    /// Best effort: losing UI state is harmless.
    pub fn set_ui_value(&self, key: &str, value: &str) {
        let _ = self.store().set_setting(&format!("ui.{key}"), value);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    /// Handles one clipboard change. Returns None when paused or when there
    /// was nothing to keep.
    pub fn on_clip(&self, clip: Option<Clip>, now_ms: i64) -> crate::Result<Option<Outcome>> {
        let Some(clip) = clip.filter(|_| !self.is_paused()) else {
            return Ok(None);
        };
        let settings = self.settings();
        let outcome = capture::capture(&self.store(), &settings, &self.images_dir, clip, now_ms)?;
        Ok(Some(outcome))
    }
}

fn to_view(item: Item) -> ItemView {
    ItemView {
        id: item.id,
        kind: match item.kind {
            Kind::Text => "text",
            Kind::Image => "image",
        },
        preview: item.text.map(|t| t.chars().take(PREVIEW_CHARS).collect()),
        thumb: item.thumb_path.and_then(|p| images::data_url(Path::new(&p))),
        pinned: item.pinned,
        last_used_at: item.last_used_at,
        source_app: item.source_app,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::ClipContent;
    use crate::images::RawImage;
    use std::sync::Arc;

    #[derive(Default)]
    struct FakeClipboard {
        writes: Mutex<Vec<String>>,
    }

    impl ClipboardWriter for Arc<FakeClipboard> {
        fn set_text(&self, text: &str) -> crate::Result<()> {
            self.writes.lock().unwrap().push(format!("text:{text}"));
            Ok(())
        }
        fn set_image_png(&self, path: &Path) -> crate::Result<()> {
            assert!(path.exists(), "image file must exist when copied");
            self.writes.lock().unwrap().push("image".into());
            Ok(())
        }
        fn set_primary_text(&self, text: &str) -> crate::Result<()> {
            self.writes.lock().unwrap().push(format!("primary:{text}"));
            Ok(())
        }
    }

    struct Env {
        history: History,
        clipboard: Arc<FakeClipboard>,
        _dir: tempfile::TempDir,
    }

    fn env() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let clipboard = Arc::new(FakeClipboard::default());
        let history = History::new(
            Store::open_in_memory().unwrap(),
            dir.path().join("images"),
            Box::new(clipboard.clone()),
        )
        .unwrap();
        Env { history, clipboard, _dir: dir }
    }

    fn text(t: &str) -> Option<Clip> {
        Some(Clip { content: ClipContent::Text(t.into()), secret: false, source_app: None })
    }

    fn image() -> Option<Clip> {
        let img = RawImage { width: 2, height: 2, rgba: vec![255; 16] };
        Some(Clip { content: ClipContent::Image(img), secret: false, source_app: None })
    }

    fn previews(items: &[ItemView]) -> Vec<&str> {
        items.iter().map(|i| i.preview.as_deref().unwrap_or("<image>")).collect()
    }

    #[test]
    fn clips_show_up_in_search_newest_first() {
        let e = env();
        e.history.on_clip(text("first"), 1).unwrap();
        e.history.on_clip(text("second"), 2).unwrap();
        assert_eq!(previews(&e.history.search("").unwrap()), ["second", "first"]);
        assert_eq!(previews(&e.history.search("fir").unwrap()), ["first"]);
    }

    #[test]
    fn empty_clipboard_change_is_ignored() {
        let e = env();
        assert_eq!(e.history.on_clip(None, 1).unwrap(), None);
    }

    #[test]
    fn paused_history_keeps_nothing() {
        let e = env();
        e.history.set_paused(true);
        assert!(e.history.is_paused());
        assert_eq!(e.history.on_clip(text("x"), 1).unwrap(), None);
        e.history.set_paused(false);
        assert!(e.history.on_clip(text("x"), 2).unwrap().is_some());
        assert_eq!(e.history.search("").unwrap().len(), 1);
    }

    #[test]
    fn long_text_preview_is_cut_on_a_char_boundary() {
        let e = env();
        let long = "é".repeat(PREVIEW_CHARS + 50);
        e.history.on_clip(text(&long), 1).unwrap();
        let preview = e.history.search("").unwrap()[0].preview.clone().unwrap();
        assert_eq!(preview.chars().count(), PREVIEW_CHARS);
    }

    #[test]
    fn image_rows_carry_a_thumbnail_data_url() {
        let e = env();
        e.history.on_clip(image(), 1).unwrap();
        let row = &e.history.search("").unwrap()[0];
        assert_eq!(row.kind, "image");
        assert_eq!(row.preview, None);
        assert!(row.thumb.as_deref().unwrap().starts_with("data:image/png;base64,"));
    }

    #[test]
    fn copy_item_writes_text_and_moves_it_to_top() {
        let e = env();
        e.history.on_clip(text("old"), 1).unwrap();
        e.history.on_clip(text("new"), 2).unwrap();
        let old_id = e.history.search("old").unwrap()[0].id;

        e.history.copy_item(old_id, 3).unwrap();
        assert_eq!(*e.clipboard.writes.lock().unwrap(), ["text:old", "primary:old"]);
        assert_eq!(previews(&e.history.search("").unwrap()), ["old", "new"]);
    }

    #[test]
    fn primary_selection_can_be_turned_off() {
        let e = env();
        let settings = Settings { primary_selection: false, ..e.history.settings() };
        e.history.update_settings(settings, 1).unwrap();
        e.history.on_clip(text("abc"), 1).unwrap();
        let id = e.history.search("").unwrap()[0].id;
        e.history.copy_item(id, 2).unwrap();
        assert_eq!(*e.clipboard.writes.lock().unwrap(), ["text:abc"]);
    }

    #[test]
    fn copy_item_writes_images_from_their_file() {
        let e = env();
        e.history.on_clip(image(), 1).unwrap();
        let id = e.history.search("").unwrap()[0].id;
        e.history.copy_item(id, 2).unwrap();
        assert_eq!(*e.clipboard.writes.lock().unwrap(), ["image"]);
    }

    #[test]
    fn copy_of_missing_item_is_an_error() {
        let e = env();
        assert!(e.history.copy_item(5, 1).is_err());
        assert!(e.clipboard.writes.lock().unwrap().is_empty());
    }

    #[test]
    fn our_own_copy_coming_back_does_not_duplicate() {
        let e = env();
        e.history.on_clip(text("abc"), 1).unwrap();
        let id = e.history.search("").unwrap()[0].id;
        e.history.copy_item(id, 2).unwrap();
        assert_eq!(e.history.on_clip(text("abc"), 3).unwrap(), Some(Outcome::Bumped(id)));
        assert_eq!(e.history.search("").unwrap().len(), 1);
    }

    #[test]
    fn pin_delete_and_clear() {
        let e = env();
        e.history.on_clip(text("keep"), 1).unwrap();
        e.history.on_clip(text("gone"), 2).unwrap();
        e.history.on_clip(text("also gone"), 3).unwrap();
        let rows = e.history.search("").unwrap();
        let (also, gone, keep) = (rows[0].id, rows[1].id, rows[2].id);

        assert!(e.history.toggle_pin(keep).unwrap());
        assert!(e.history.search("").unwrap()[0].pinned);
        e.history.delete(gone).unwrap();
        assert_eq!(previews(&e.history.search("").unwrap()), ["keep", "also gone"]);
        assert_eq!(e.history.clear().unwrap(), 1);
        assert_eq!(previews(&e.history.search("").unwrap()), ["keep"]);
        assert!(e.history.toggle_pin(also).is_err(), "pinning a deleted item fails");
    }

    #[test]
    fn settings_changes_apply_to_capture() {
        let e = env();
        let mut s = e.history.settings();
        assert!(s.capture_images);
        s.capture_images = false;
        e.history.update_settings(s, 1).unwrap();
        assert_eq!(e.history.on_clip(image(), 2).unwrap(), Some(Outcome::Skipped("images off")));
    }

    #[test]
    fn update_settings_returns_clamped_values_and_purges() {
        let e = env();
        e.history.on_clip(text("ancient"), 0).unwrap();
        let s = Settings { retention_days: 0, ..e.history.settings() };
        let saved = e.history.update_settings(s, 10 * crate::store::retention::DAY_MS).unwrap();
        assert_eq!(saved.retention_days, 1);
        assert_eq!(e.history.settings().retention_days, 1);
        assert!(e.history.search("").unwrap().is_empty(), "old item purged by the new retention");
    }

    #[test]
    fn ui_values_round_trip_without_touching_settings() {
        let e = env();
        assert_eq!(e.history.ui_value("window_size"), None);
        e.history.set_ui_value("window_size", "[400,500]");
        assert_eq!(e.history.ui_value("window_size").as_deref(), Some("[400,500]"));
        assert_eq!(e.history.settings(), Settings::default());
    }

    #[test]
    fn settings_persist_in_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("h.db");
        let open = || {
            let writer: Box<dyn ClipboardWriter> = Box::new(Arc::new(FakeClipboard::default()));
            History::new(Store::open(&path).unwrap(), dir.path().join("img"), writer).unwrap()
        };
        let s = Settings { skip_secrets: true, ..open().settings() };
        open().update_settings(s, 1).unwrap();
        assert!(open().settings().skip_secrets);
    }
}
