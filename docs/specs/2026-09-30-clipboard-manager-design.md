# ClipboardManager — Design

A clipboard history app like Ditto, but for Windows, macOS and Linux. Ubuntu/GNOME on Wayland must work.
Every copy is saved. A hotkey opens a small window. You search, pick an item, and it goes back on the clipboard.

## Decisions

| Topic | Decision |
|---|---|
| Stack | Tauri 2. Rust backend, plain TypeScript + Vite UI |
| Linux target | Ubuntu / GNOME / Wayland, tested on a real Ubuntu machine |
| Picking an item | Copy to clipboard only. No auto-paste |
| v1 scope | Text, images, pinning, optional filter for password-manager items |
| Secret filter | Off by default |
| Retention | 3 days by default, user-configurable. Pinned items never expire |

**Why GNOME needs an extension.** Mutter (GNOME's compositor) does not support `wlr-data-control` or `ext-data-control`. So a normal app can't watch the clipboard on GNOME Wayland. Tauri's global-shortcut plugin also doesn't work on Wayland. A GNOME Shell extension runs inside GNOME, so it can do both. GPaste and Clipboard Indicator work the same way.

## Architecture

```
Windows/macOS/X11 ── native source (clipboard-rs watcher) ─┐
                                                           ├─► capture ─► store (SQLite) ◄── UI
GNOME Wayland ────── gnome source (D-Bus to extension) ────┘
```

- **Source.** One `ClipboardSource` trait with two implementations. Use the GNOME source when `XDG_SESSION_TYPE=wayland` and `XDG_CURRENT_DESKTOP` contains `GNOME`. Use the native source everywhere else.
- **Capture.** For each new clip:
  1. Skip it if it's secret and the filter is on, or if it came from an ignored app.
  2. Dedupe by hash. A repeat moves the existing item to the top.
  3. Save images to disk.
  4. Write the row.
  5. Emit an event to the UI.

  Dedupe also absorbs our own clipboard writes.
- **Store.** `rusqlite` with the `bundled` feature. FTS5 with the `trigram` tokenizer gives substring search. Queries shorter than 3 characters fall back to `LIKE`.
- **Writing to the clipboard.** Native: `clipboard-rs`. GNOME: the extension's `SetContent` over D-Bus.

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

- **Retention.** Delete rows where `pinned = 0 AND last_used_at < now - retention_days`, and delete their image files. Run at startup, every hour, and whenever the setting changes.
- **Images.** Store a PNG and a thumbnail in the app data directory. Skip images over 20 MB.

### Settings

| Key | Default |
|---|---|
| `retention_days` | 3 (range 1–365) |
| `capture_images` | true |
| `skip_secrets` | false |
| `ignored_apps` | [] |
| `hotkey` | `Ctrl+Alt+V` (Linux terminals use `Ctrl+Shift+V` for paste) |
| `launch_at_login` | true |
| `primary_selection` | true. Linux only: picked text also goes on the primary selection (middle-click, Shift+Insert). Text only. |

### Secret markers

| OS | Marker |
|---|---|
| Windows | `ExcludeClipboardContentFromMonitorProcessing`, or `CanIncludeInClipboardHistory` = 0 |
| macOS | `org.nspasteboard.ConcealedType` |
| Linux | MIME `x-kde-passwordManagerHint` = `secret` (set by KeePassXC) |

## Layout

```
src/                        main.ts, api.ts, historyView.ts, settingsView.ts, styles.css
src-tauri/src/
  main.rs lib.rs            setup, plugins, source selection
  commands.rs               search, copy_item, toggle_pin, delete, clear, settings
  settings.rs  window.rs    settings load/save; tray, hotkey, show/hide
  gnome_hotkey.rs           hotkey as a GNOME custom shortcut (GNOME Wayland)
  capture.rs  images.rs     capture flow; PNG + thumbnail
  store/{db,retention}.rs
  clipboard/{mod,native,gnome,secret}.rs
gnome-extension/clipboardmanager@nayan.dev/
  metadata.json  extension.js  schemas/*.gschema.xml
.github/workflows/release.yml
```

- **Rust dependencies:** `tauri` 2, `tauri-plugin-global-shortcut`, `tauri-plugin-single-instance`, `tauri-plugin-autostart`, `clipboard-rs`, `rusqlite` (with `bundled`), `zbus` and `arboard` (Linux only; `arboard` writes the primary selection), `image`, `sha2`, `serde`.
- **UI dependencies:** `@tauri-apps/api`, `vite`, `typescript`, `vitest`.

## GNOME extension

- **Clipboard watching.** Listen for `owner-changed` on `global.display.get_selection()` for CLIPBOARD.
- **D-Bus service.** Session bus name `dev.nayan.ClipboardManager.Shell`:
  - Signal `Changed(as mimetypes, s sourceApp)`. Metadata only. `sourceApp` is the focused window's WM class.
  - `GetContent(s mime) → ay`
  - `SetContent(s mime, ay data)`
  - `Ping() → s version`
- **Hotkey.** Registered with `Main.wm.addKeybinding`. It calls `Toggle()` on the app's bus name, `dev.nayan.ClipboardManager`.
  - Until the extension exists, the app writes a GNOME custom shortcut (Settings > Keyboard > Custom Shortcuts) that runs `clipboard-manager --toggle`. Saving a new hotkey updates it, and it's turned off while the settings page records keys. The X11 hotkey is skipped on GNOME Wayland so a press doesn't toggle twice.
- **App not running.** The extension does nothing, and those copies are lost. This is acceptable for v1.
- **Versions.** GNOME Shell 45+, ESM format.

## Window

- About 420×520 and resizable. It remembers its size.
- Hides on blur or Esc. The search box gets focus when the window opens.
- Placement: centered on Wayland, near the cursor on Windows and macOS.
- Keys:
  - ↑/↓ moves the selection.
  - Enter copies the item and hides the window.
  - Ctrl+P pins or unpins.
  - Delete removes the item.
  - Ctrl+, opens settings.
- List: pinned items first, then newest. Each row shows a 2-line preview or a thumbnail, plus relative time.
- Tray menu: Show, Pause capture, Settings, Quit.
  - Ubuntu needs AppIndicator for the tray (on by default).
  - macOS runs as an accessory app (no Dock icon).
- Single instance. A second launch toggles the window, so `clipboardmanager --toggle` works as a fallback hotkey.

## Phases

1. ✅ Scaffold: Tauri 2 + Vite + TS, repo, AGENTS.md.
2. ✅ **Storage core, test-first.** Insert, dedupe, FTS and short-query search, pin, delete, retention.
3. ✅ **Native capture.** Source, secret check, images, capture flow. Test with a fake source. Still needs a manual check on a real Windows clipboard.
4. ✅ **UI.** List, search (100 ms debounce), keyboard, copy on Enter, pin, delete, settings.
5. **Background.** ✅ Tray, hotkey, single instance + `--toggle`, hide on blur, autostart, pause. Still to do: revisit `panic = "abort"` so a crashing watcher thread can recover.
6. **GNOME.** Extension, `clipboard/gnome.rs` (zbus), GNOME detection, `Toggle()`. Show an "extension not found" banner when `Ping` fails.
7. **`ignored_apps`.** Source app per platform:
   - Windows: `GetClipboardOwner` → process name.
   - macOS: frontmost app.
   - GNOME: WM class.

   Plus a settings editor for the list.
8. **Packaging.** `tauri-action` builds `.msi`/NSIS, `.dmg`, `.deb`/`.AppImage`.
   - The `.deb` installs the extension into `/usr/share/gnome-shell/extensions/`. The user runs `gnome-extensions enable clipboardmanager@nayan.dev` once, then re-logs in.
   - Builds are unsigned in v1.

## Risks to test

- The GNOME extension API changes slightly between releases. Expect small fixes per GNOME version.
- Setting images through St.Clipboard is untested. Check it on Ubuntu.
- macOS needs a real Mac. CI can build it, but not test it.

## Verification

- **Automated:** `cargo test` (store, retention, dedupe, search, secret check, capture) and `npm test` (UI logic). CI runs both on all 3 OSes.
- **Manual, on Windows and Ubuntu GNOME Wayland:**
  1. Copy text in 3 apps. All 3 appear, newest first.
  2. Copy the same text twice. Only one row appears, at the top.
  3. Copy a screenshot. A thumbnail appears. Enter restores the image, and it pastes into an image editor.
  4. Search a mid-word substring and a 2-letter query. Both find the item.
  5. Pin an item, then age the others (set `last_used_at` back 2 days, retention = 1). Unpinned items are removed. The pinned item stays.
  6. Filter on: a KeePassXC password is not saved. Filter off: it is saved.
  7. The hotkey toggles the window from any app. Search gets focus. Esc and blur both hide it.
  8. After a reboot, the app autostarts and history persists.
  9. GNOME with the extension disabled: the banner shows and the app does not crash.
