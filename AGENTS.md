# ClipboardManager — agent instructions

Cross-platform clipboard history app (like Ditto) for Windows, macOS and Linux. Ubuntu/GNOME on Wayland must work.
Read `docs/specs/2026-09-30-clipboard-manager-design.md` before changing behavior. Setup and build steps per OS: `docs/setup.md`.

## Hard rules
- History never leaves the machine. No network calls, no telemetry.
- The "skip secret items" filter is off by default.
- D-Bus signals carry metadata only. Clipboard content goes through method calls.
- No UI framework. Plain TypeScript + DOM.

## Code style
- One file per topic, about 100–300 lines. Split only past ~400 lines. No tiny one-function files.
- Short comments, only where the "why" isn't obvious.

## Git
- `main` stays clean. Do each piece of work on its own branch (e.g. `feat/storage-core`). Don't merge into `main` unless asked.
- Run git commands inside this folder only.

## Commands (full list per OS in `docs/setup.md`)
- `npm run tauri dev` — run the app
- `npm run tauri build` — build installers
- `npm test` — UI tests (Vitest)
- `cargo test --manifest-path src-tauri/Cargo.toml` — Rust tests

Run the relevant tests before saying something works.
