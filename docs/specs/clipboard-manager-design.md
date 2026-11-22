# ClipboardManager — Design

Clipboard history app for Windows, macOS and Linux.
Every copy is saved. A hotkey opens a small window. You search, pick an item, and it goes back on the clipboard.

## Overview

| Topic | Behavior |
|---|---|
| Stack | Tauri 2. Rust backend, plain TypeScript + Vite UI |
| Picking an item | Copies it to the clipboard. Optional auto-paste, off by default |
| Content | Text and images. Items can be pinned |
| Secret filter | Skips password-manager items. Off by default |
| Retention | 3 days by default, configurable. Pinned items never expire |

## Architecture

```
Windows/macOS/X11 ── native source (clipboard-rs watcher) ─┐
                                                           ├─► capture ─► store (SQLite) ◄── UI
GNOME Wayland ────── gnome source (planned, D-Bus) ───────┘
```

- **Source.** The native source watches the clipboard. A GNOME source is planned for when `XDG_SESSION_TYPE=wayland` and `XDG_CURRENT_DESKTOP` contains `GNOME`.
- **Capture.** For each new clip:
  1. Skip it if the filter is on and it's secret, or if it came from an ignored app.
  2. Dedupe by hash. A repeat moves the existing item to the top. This also absorbs the app's own clipboard writes.
  3. Save images to disk, write the row, and notify the UI.
- **Store.** `rusqlite` with `bundled`. FTS5 with the `trigram` tokenizer does substring search. Queries under 3 characters use `LIKE`.
- **Writing to the clipboard.** Native: `clipboard-rs`. GNOME (planned): the extension's `SetContent`.

### Data model

```sql
items(id INTEGER PK, kind TEXT CHECK(kind IN ('text','image')),
      text TEXT, image_path TEXT, thumb_path TEXT,
      hash TEXT UNIQUE,                          -- sha256, for dedupe
      created_at INTEGER, last_used_at INTEGER,  -- unix ms
      pinned INTEGER DEFAULT 0, source_app TEXT)
items_fts USING fts5(text, content='items', content_rowid='id', tokenize='trigram')
settings(key TEXT PK, value TEXT)                -- JSON values
```

- **Retention.** Delete rows where `pinned = 0 AND last_used_at < now - retention_days`, and their image files. Runs at startup, every hour, and when the setting changes.
- **Images.** A PNG and a thumbnail in the app data directory. Images over 20 MB are skipped.

### Settings

| Key | Default |
|---|---|
| `retention_days` | 3 (range 1–365) |
| `capture_images` | true |
| `skip_secrets` | false |
| `ignored_apps` | [] |
| `hotkey` | `Ctrl+Alt+V` |
| `launch_at_login` | true |
| `primary_selection` | true. Linux only: picked text also goes on the primary selection |
| `auto_paste` | false. After a pick, the window hides and the app presses Ctrl+V (Cmd+V on macOS) in the previous app |

### Secret markers

| OS | Marker |
|---|---|
| Windows | `ExcludeClipboardContentFromMonitorProcessing`, or `CanIncludeInClipboardHistory` = 0 |
| macOS | `org.nspasteboard.ConcealedType` |
| Linux | MIME `x-kde-passwordManagerHint` = `secret` |

## Dependencies

- **Rust:** `tauri` 2, `tauri-plugin-global-shortcut`, `tauri-plugin-single-instance`, `tauri-plugin-autostart`, `clipboard-rs`, `rusqlite`, `arboard` (Linux only), `image`, `sha2`, `serde`.
- **UI:** `@tauri-apps/api`, `vite`, `typescript`, `vitest`.

## GNOME extension (planned)

On GNOME Wayland, a GNOME Shell extension watches the clipboard and handles the hotkey.

- **Watching.** `owner-changed` on `global.display.get_selection()` for CLIPBOARD.
- **D-Bus service** on the session bus as `dev.nayan.ClipboardManager.Shell`:
  - Signal `Changed(as mimetypes, s sourceApp)`. Metadata only. `sourceApp` is the focused window's WM class.
  - `GetContent(s mime) → ay`
  - `SetContent(s mime, ay data)`
  - `Ping() → s version`
- **Hotkey.** `Main.wm.addKeybinding`, which calls `Toggle()` on `dev.nayan.ClipboardManager`.
- **App not running.** Copies are not recorded.
- **Versions.** GNOME Shell 45+, ESM format.

Without the extension, the hotkey is a GNOME custom shortcut that runs `clipboard-manager --toggle`. It's updated when the hotkey changes and turned off while the settings page records keys.

## Window

- About 420×520, resizable, remembers its size.
- Hides on blur or Esc. Search gets focus on open.
- Centered on Wayland, near the cursor on Windows and macOS.
- Keys: ↑/↓ select, Enter copies (and pastes, if on) and hides, Ctrl+P pins, Delete removes, Ctrl+, opens settings.
- List: pinned first, then newest. Each row shows a 2-line preview or a thumbnail, plus relative time.
- Tray menu: Show, Pause capture, Settings, Quit. Linux needs AppIndicator support. macOS has no Dock icon.
- Single instance. A second launch toggles the window, so `clipboard-manager --toggle` works as a hotkey.

## Packaging

`.github/workflows/release.yml` builds `.msi`/NSIS, `.deb`/`.rpm`/`.AppImage` and an AUR recipe into a draft GitHub Release. macOS `.dmg` is built locally. Builds are unsigned.

## Not done yet

- GNOME extension, `clipboard/gnome.rs`, and an "extension not found" banner when `Ping` fails. The `.deb` will install the extension to `/usr/share/gnome-shell/extensions/`.
- Source app for `ignored_apps` (Windows: `GetClipboardOwner`; macOS: frontmost app; GNOME: WM class), plus a settings editor for the list.
- macOS build in CI.
- With `panic = "abort"`, a crashing watcher thread can't recover.

## Risks

- The GNOME extension API changes slightly between GNOME versions.
- Setting images through `St.Clipboard` is untested.
- CI can build macOS but can't test it.
- Auto-paste sends Ctrl+V, which some Linux terminals don't use for paste.

## Verification

- **Automated:** `cargo test` and `npm test`. CI runs both on Linux and Windows before each release build.
- **Manual, on each supported OS:**
  1. Copy text in 3 apps. All 3 appear, newest first.
  2. Copy the same text twice. One row, at the top.
  3. Copy a screenshot. A thumbnail appears, and Enter restores the image.
  4. Search a mid-word substring and a 2-letter query. Both find the item.
  5. With retention = 1 and other items aged 2 days, unpinned items are removed and pinned ones stay.
  6. Filter on: a password-manager secret is not saved. Filter off: it is.
  7. The hotkey toggles the window from any app. Esc and blur hide it.
  8. After a reboot, the app autostarts and history persists.
