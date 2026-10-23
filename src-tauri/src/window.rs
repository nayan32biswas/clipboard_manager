//! The popup window, tray menu, global hotkey and autostart.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewWindow, Window, WindowEvent};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::gnome_hotkey;
use crate::history::{now_ms, History};

const MAIN: &str = "main";
/// Right after showing, the OS can report a stray focus loss. Ignore it.
const BLUR_GRACE_MS: i64 = 300;
/// How long to wait before checking whether a focus loss was real.
const BLUR_CHECK_MS: u64 = 150;
static SHOWN_AT: AtomicI64 = AtomicI64::new(0);
/// True while the settings page is recording a new shortcut.
static HOTKEY_PAUSED: AtomicBool = AtomicBool::new(false);
static FRAME: Mutex<Frame> = Mutex::new(Frame::Idle);

/// Linux: GTK draws the title bar and edges. Dragging them reports focus
/// lost until the button is let go, so a plain focus check would hide the
/// window mid-drag. Clicks inside the page never reach the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Frame {
    Idle,
    Pressed,
    /// A move or resize is in progress.
    Grabbed,
}

#[derive(Debug, Clone, Copy)]
enum FrameEvent {
    Press,
    Release,
    Blur,
    Focus,
}

fn frame_after(state: Frame, event: FrameEvent) -> Frame {
    match (state, event) {
        (_, FrameEvent::Press) => Frame::Pressed,
        (Frame::Pressed, FrameEvent::Release) => Frame::Idle,
        (Frame::Pressed, FrameEvent::Blur) => Frame::Grabbed,
        (Frame::Grabbed, FrameEvent::Focus) => Frame::Idle,
        (state, _) => state,
    }
}

/// Returns the new state.
fn frame_event(event: FrameEvent) -> Frame {
    let mut frame = FRAME.lock().unwrap();
    *frame = frame_after(*frame, event);
    *frame
}

fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(MAIN)
}

pub fn show(app: &AppHandle) {
    let Some(win) = main_window(app) else { return };
    place(&win);
    *FRAME.lock().unwrap() = Frame::Idle;
    SHOWN_AT.store(now_ms(), Ordering::Relaxed);
    let _ = win.unminimize();
    let _ = win.show();
    let _ = win.set_focus();
    let _ = app.emit("window-shown", ());
}

pub fn hide(app: &AppHandle) {
    let Some(win) = main_window(app) else { return };
    remember_size(app, &win);
    // Never leave the app unreachable because recording was cut short.
    resume_hotkey(app);
    let _ = win.hide();
}

pub fn toggle(app: &AppHandle) {
    let Some(win) = main_window(app) else { return };
    let visible = win.is_visible().unwrap_or(false);
    let focused = win.is_focused().unwrap_or(false);
    if visible && focused {
        hide(app);
    } else {
        show(app);
    }
}

/// Wayland doesn't let apps place windows, so center there. Elsewhere, open
/// next to the mouse, kept inside the screen.
fn place(win: &WebviewWindow) {
    if cfg!(target_os = "linux") {
        let _ = win.center();
        return;
    }
    let placed = (|| {
        let cursor = win.cursor_position().ok()?;
        let monitor = win.monitor_from_point(cursor.x, cursor.y).ok()??;
        let area = monitor.work_area();
        let size = win.outer_size().ok()?;
        let fit = |pos: f64, start: i32, len: u32, win_len: u32| {
            let max = start + len as i32 - win_len as i32;
            (pos as i32).min(max).max(start)
        };
        let x = fit(cursor.x, area.position.x, area.size.width, size.width);
        let y = fit(cursor.y, area.position.y, area.size.height, size.height);
        win.set_position(PhysicalPosition::new(x, y)).ok()
    })();
    if placed.is_none() {
        let _ = win.center();
    }
}

fn remember_size(app: &AppHandle, win: &WebviewWindow) {
    let (Ok(size), Ok(scale)) = (win.inner_size(), win.scale_factor()) else { return };
    let logical = size.to_logical::<f64>(scale);
    let value = format!("[{},{}]", logical.width.round(), logical.height.round());
    app.state::<History>().set_ui_value("window_size", &value);
}

pub fn restore_size(app: &AppHandle) {
    let Some(win) = main_window(app) else { return };
    let saved = app.state::<History>().ui_value("window_size");
    if let Some([w, h]) = saved.and_then(|v| serde_json::from_str::<[f64; 2]>(&v).ok()) {
        let _ = win.set_size(LogicalSize::new(w, h));
    }
}

fn blur_means_hide(since_shown_ms: i64, still_focused: bool) -> bool {
    since_shown_ms > BLUR_GRACE_MS && !still_focused
}

pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if window.label() != MAIN {
        return;
    }
    match event {
        // Grabbing the frame to resize or move hands focus from the page to
        // the window itself, which arrives as a focus loss. Check again a
        // moment later and only hide if focus really went elsewhere.
        WindowEvent::Focused(true) => {
            frame_event(FrameEvent::Focus);
        }
        WindowEvent::Focused(false) => {
            if frame_event(FrameEvent::Blur) == Frame::Grabbed {
                return;
            }
            let app = window.app_handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(BLUR_CHECK_MS));
                let Some(win) = main_window(&app) else { return };
                let since_shown = now_ms() - SHOWN_AT.load(Ordering::Relaxed);
                let grabbed = *FRAME.lock().unwrap() == Frame::Grabbed;
                if !grabbed && blur_means_hide(since_shown, win.is_focused().unwrap_or(false)) {
                    hide(&app);
                }
            });
        }
        // Closing only hides. Quit is in the tray menu.
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            hide(window.app_handle());
        }
        _ => {}
    }
}

/// Linux only: tells `on_window_event` when the title bar or an edge is pressed.
#[cfg(target_os = "linux")]
pub fn watch_frame(app: &AppHandle) {
    use gtk::prelude::*;
    let Some(gtk_win) = main_window(app).and_then(|w| w.gtk_window().ok()) else { return };
    gtk_win.connect_button_press_event(|_, ev| {
        if ev.button() == 1 {
            frame_event(FrameEvent::Press);
        }
        gtk::glib::Propagation::Proceed
    });
    gtk_win.connect_button_release_event(|_, ev| {
        if ev.button() == 1 {
            frame_event(FrameEvent::Release);
        }
        gtk::glib::Propagation::Proceed
    });
}

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let pause = CheckMenuItem::with_id(app, "pause", "Pause capture", true, false, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show_item, &pause, &settings, &sep, &quit])?;

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("ClipboardManager")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "show" => show(app),
            "pause" => {
                let history = app.state::<History>();
                history.set_paused(!history.is_paused());
                let _ = pause.set_checked(history.is_paused());
            }
            "settings" => {
                show(app);
                let _ = app.emit("open-settings", ());
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                toggle(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

fn parse_hotkey(hotkey: &str) -> Result<Shortcut, String> {
    hotkey.parse::<Shortcut>().map_err(|_| format!("\"{hotkey}\" is not a valid shortcut"))
}

/// On GNOME Wayland, GNOME owns the shortcut and the X11 grab is skipped.
/// Having both would toggle twice while an X11 app has focus.
pub fn register_hotkey(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    let shortcut = parse_hotkey(hotkey)?;
    let result = if gnome_hotkey::active() {
        gnome_hotkey::set(&shortcut)
    } else {
        app.global_shortcut().register(shortcut).map_err(|e| e.to_string())
    };
    result.map_err(|e| format!("could not register \"{hotkey}\": {e}"))
}

fn unregister_hotkey(app: &AppHandle, hotkey: &str) {
    if gnome_hotkey::active() {
        if let Err(e) = gnome_hotkey::disable() {
            eprintln!("hotkey: {e}");
        }
    } else if let Ok(shortcut) = parse_hotkey(hotkey) {
        let _ = app.global_shortcut().unregister(shortcut);
    }
}

/// The OS swallows a registered shortcut, so it must be off for the
/// settings page to see it being pressed.
pub fn pause_hotkey(app: &AppHandle) {
    if !HOTKEY_PAUSED.swap(true, Ordering::SeqCst) {
        unregister_hotkey(app, &app.state::<History>().settings().hotkey);
    }
}

pub fn resume_hotkey(app: &AppHandle) {
    if HOTKEY_PAUSED.swap(false, Ordering::SeqCst) {
        let hotkey = app.state::<History>().settings().hotkey;
        if let Err(e) = register_hotkey(app, &hotkey) {
            eprintln!("hotkey: {e}");
        }
    }
}

/// Swaps the hotkey. If the new one fails, the old one stays active.
pub fn change_hotkey(app: &AppHandle, old: &str, new: &str) -> Result<(), String> {
    parse_hotkey(new)?;
    unregister_hotkey(app, old);
    register_hotkey(app, new).inspect_err(|_| {
        let _ = register_hotkey(app, old);
    })
}

/// Dev builds skip this so `tauri dev` doesn't add itself to login items.
pub fn apply_autostart(app: &AppHandle, enabled: bool) {
    if cfg!(debug_assertions) {
        return;
    }
    let autostart = app.autolaunch();
    let result = if enabled { autostart.enable() } else { autostart.disable() };
    if let Err(e) = result {
        eprintln!("autostart: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grabbing_the_frame_does_not_hide() {
        // Resize or move: reported as a focus loss, but the window still has focus.
        assert!(!blur_means_hide(5_000, true));
    }

    #[test]
    fn switching_to_another_app_hides() {
        assert!(blur_means_hide(5_000, false));
    }

    fn run(events: &[FrameEvent]) -> Frame {
        events.iter().fold(Frame::Idle, |state, &e| frame_after(state, e))
    }

    #[test]
    fn dragging_the_frame_counts_as_a_grab_until_focus_returns() {
        use FrameEvent::*;
        // The press also reports focus gained just before the grab starts.
        assert_eq!(run(&[Press, Focus, Blur]), Frame::Grabbed);
        assert_eq!(run(&[Press, Focus, Blur, Focus]), Frame::Idle);
    }

    #[test]
    fn clicking_the_frame_without_dragging_still_hides_on_a_later_blur() {
        use FrameEvent::*;
        assert_eq!(run(&[Press, Focus, Release, Blur]), Frame::Idle);
    }

    #[test]
    fn switching_apps_is_not_a_grab() {
        assert_eq!(run(&[FrameEvent::Blur]), Frame::Idle);
    }

    #[test]
    fn a_stray_blur_right_after_showing_is_ignored() {
        assert!(!blur_means_hide(BLUR_GRACE_MS - 1, false));
    }
}
