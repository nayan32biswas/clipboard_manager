//! Clipboard access. `native` covers Windows, macOS and X11.

use std::path::Path;

pub mod native;
pub mod secret;

/// Puts a history item back on the system clipboard.
pub trait ClipboardWriter: Send + Sync {
    fn set_text(&self, text: &str) -> crate::Result<()>;
    fn set_image_png(&self, path: &Path) -> crate::Result<()>;
    /// Linux's second clipboard, pasted with middle-click and, in many apps,
    /// Shift+Insert. Other systems don't have one.
    fn set_primary_text(&self, _text: &str) -> crate::Result<()> {
        Ok(())
    }
}
