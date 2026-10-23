pub mod capture;
pub mod clipboard;
pub mod commands;
pub mod gnome_hotkey;
pub mod history;
pub mod images;
pub mod settings;
pub mod store;
pub mod window;

use std::sync::Mutex;
use std::time::Duration;

use tauri::{App, Emitter, Manager};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::ShortcutState;

use crate::capture::Outcome;
use crate::clipboard::native::{self, NativeClipboard};
use crate::history::{now_ms, History};
use crate::store::Store;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Autostart launches with this flag so the app starts in the tray.
const HIDDEN_ARG: &str = "--hidden";

/// Keeps the clipboard watcher running for the app's lifetime.
struct Watcher(#[allow(dead_code)] Mutex<clipboard_rs::WatcherShutdown>);

pub fn run() {
    tauri::Builder::default()
        // Must come first. A second launch (or `--toggle`) toggles this window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| window::toggle(app)))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        window::toggle(app);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec![HIDDEN_ARG])))
        .invoke_handler(tauri::generate_handler![
            commands::search,
            commands::copy_item,
            commands::toggle_pin,
            commands::delete_item,
            commands::clear_history,
            commands::get_settings,
            commands::set_settings,
            commands::hide_window,
            commands::pause_hotkey,
            commands::resume_hotkey,
        ])
        .on_window_event(window::on_window_event)
        .setup(|app| setup(app).map_err(|e| e as Box<dyn std::error::Error>))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn setup(app: &mut App) -> Result<()> {
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);

    let data_dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&data_dir)?;
    let store = Store::open(&data_dir.join("history.db"))?;
    let history = History::new(store, data_dir.join("images"), Box::new(NativeClipboard::new()?))?;
    history.purge_expired(now_ms())?;
    let settings = history.settings();
    app.manage(history);

    let handle = app.handle().clone();
    start_watcher(&handle)?;
    start_retention_timer(&handle);
    window::setup_tray(&handle)?;
    if let Err(e) = window::register_hotkey(&handle, &settings.hotkey) {
        eprintln!("hotkey: {e}");
    }
    window::apply_autostart(&handle, settings.launch_at_login);
    window::restore_size(&handle);
    #[cfg(target_os = "linux")]
    window::watch_frame(&handle);

    if !std::env::args().any(|a| a == HIDDEN_ARG) {
        window::show(&handle);
    }
    Ok(())
}

fn start_watcher(app: &tauri::AppHandle) -> Result<()> {
    let reader = NativeClipboard::new()?;
    let watcher_app = app.clone();
    let shutdown = native::watch(move || {
        let app = &watcher_app;
        let history = app.state::<History>();
        match history.on_clip(reader.read(), now_ms()) {
            Ok(Some(Outcome::Added(_) | Outcome::Bumped(_))) => {
                let _ = app.emit("history-changed", ());
            }
            Ok(_) => {}
            Err(e) => eprintln!("capture: {e}"),
        }
    })?;
    app.manage(Watcher(Mutex::new(shutdown)));
    Ok(())
}

/// Retention also runs at startup and whenever the setting changes.
fn start_retention_timer(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(60 * 60));
        if let Err(e) = app.state::<History>().purge_expired(now_ms()) {
            eprintln!("retention: {e}");
        }
    });
}
