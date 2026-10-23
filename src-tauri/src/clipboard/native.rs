//! The system clipboard through `clipboard-rs` (Windows, macOS, X11).
//! `arboard` writes the Linux primary selection, which `clipboard-rs` can't.

use std::path::Path;
#[cfg(target_os = "linux")]
use std::sync::Mutex;

use clipboard_rs::common::RustImage;
use clipboard_rs::{
    Clipboard, ClipboardContext, ClipboardHandler, ClipboardWatcher, ClipboardWatcherContext,
    ContentFormat, RustImageData, WatcherShutdown,
};

use super::{secret, ClipboardWriter};
use crate::capture::{Clip, ClipContent};
use crate::images::RawImage;

pub struct NativeClipboard {
    ctx: ClipboardContext,
    /// Must stay alive: X11 only hands out what we set while we own it.
    #[cfg(target_os = "linux")]
    primary: Mutex<Option<arboard::Clipboard>>,
}

// SAFETY: clipboard-rs marks its Windows context Send + Sync. Every call
// here opens and closes the clipboard itself, so sharing is fine.
unsafe impl Send for NativeClipboard {}
unsafe impl Sync for NativeClipboard {}

impl NativeClipboard {
    pub fn new() -> crate::Result<Self> {
        Ok(NativeClipboard {
            ctx: ClipboardContext::new()?,
            #[cfg(target_os = "linux")]
            primary: Mutex::new(None),
        })
    }

    /// Reads what's on the clipboard now. Text wins over images, because apps
    /// like Excel offer both and the text is what people want back.
    /// Returns None for content we don't keep (files, empty clipboard).
    pub fn read(&self) -> Option<Clip> {
        let formats = self.ctx.available_formats().unwrap_or_default();
        let secret = secret::is_secret(&formats, |f| self.ctx.get_buffer(f).ok());

        let content = if self.ctx.has(ContentFormat::Text) {
            ClipContent::Text(self.ctx.get_text().ok()?)
        } else if self.ctx.has(ContentFormat::Image) {
            let rgba = self.ctx.get_image().ok()?.to_rgba8().ok()?;
            ClipContent::Image(RawImage { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() })
        } else {
            return None;
        };
        // The source app comes in phase 7 (ignored_apps).
        Some(Clip { content, secret, source_app: None })
    }
}

impl ClipboardWriter for NativeClipboard {
    fn set_text(&self, text: &str) -> crate::Result<()> {
        self.ctx.set_text(text.to_owned())
    }

    fn set_image_png(&self, path: &Path) -> crate::Result<()> {
        let path = path.to_str().ok_or("image path is not valid UTF-8")?;
        self.ctx.set_image(RustImageData::from_path(path)?)
    }

    #[cfg(target_os = "linux")]
    fn set_primary_text(&self, text: &str) -> crate::Result<()> {
        use arboard::{LinuxClipboardKind, SetExtLinux};
        let mut primary = self.primary.lock().unwrap();
        if primary.is_none() {
            *primary = Some(arboard::Clipboard::new()?);
        }
        let clipboard = primary.as_mut().expect("set just above");
        clipboard.set().clipboard(LinuxClipboardKind::Primary).text(text.to_owned())?;
        Ok(())
    }
}

struct Handler<F>(F);

impl<F: FnMut()> ClipboardHandler for Handler<F> {
    fn on_clipboard_change(&mut self) {
        (self.0)()
    }
}

/// Calls `on_change` on a background thread after every clipboard change.
/// Dropping the returned handle stops watching.
pub fn watch(on_change: impl FnMut() + Send + 'static) -> crate::Result<WatcherShutdown> {
    let mut watcher = ClipboardWatcherContext::new()?;
    watcher.add_handler(Handler(on_change));
    let shutdown = watcher.get_shutdown_channel();
    std::thread::Builder::new()
        .name("clipboard-watcher".into())
        .spawn(move || watcher.start_watch())?;
    Ok(shutdown)
}
