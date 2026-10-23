//! The hotkey on GNOME Wayland, kept as a GNOME custom shortcut.
//!
//! The global-shortcut plugin grabs keys through X11, and GNOME only passes
//! keys to X11 apps while one of them has focus. So on GNOME Wayland the app
//! writes its own entry under Settings > Keyboard > Custom Shortcuts instead.
//! The entry runs this binary again, and the single-instance plugin turns that
//! second launch into a toggle. The GNOME extension (phase 6) can replace this.

use std::process::Command;

use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

const SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys";
const ENTRY_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding";
const ENTRY_PATH: &str = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/clipboard-manager/";

pub fn active() -> bool {
    let var = |name| std::env::var(name).ok();
    cfg!(target_os = "linux") && is_gnome_wayland(var("XDG_SESSION_TYPE").as_deref(), var("XDG_CURRENT_DESKTOP").as_deref())
}

fn is_gnome_wayland(session: Option<&str>, desktop: Option<&str>) -> bool {
    session == Some("wayland") && desktop.is_some_and(|d| d.split(':').any(|part| part == "GNOME"))
}

/// Creates or updates the entry so `shortcut` toggles the window.
pub fn set(shortcut: &Shortcut) -> Result<(), String> {
    let binding = accelerator(shortcut).ok_or("GNOME can't use this key for a shortcut")?;
    let listed = gsettings(&["get", SCHEMA, "custom-keybindings"])?;
    if let Some(list) = with_entry(&listed) {
        gsettings(&["set", SCHEMA, "custom-keybindings", &list])?;
    }
    set_entry("name", "ClipboardManager")?;
    set_entry("command", &format!("{} --toggle", shell_quote(&launch_path()?)))?;
    set_binding(&binding)
}

/// An empty binding keeps the entry but stops GNOME from catching the keys.
pub fn disable() -> Result<(), String> {
    set_binding("")
}

fn set_binding(binding: &str) -> Result<(), String> {
    set_entry("binding", binding)
}

fn set_entry(key: &str, value: &str) -> Result<(), String> {
    gsettings(&["set", &format!("{ENTRY_SCHEMA}:{ENTRY_PATH}"), key, &gvariant_str(value)]).map(drop)
}

fn gsettings(args: &[&str]) -> Result<String, String> {
    let out = Command::new("gsettings").args(args).output().map_err(|e| format!("could not run gsettings: {e}"))?;
    if !out.status.success() {
        return Err(format!("gsettings failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// An AppImage runs from a new temp folder each time, so point at the file itself.
fn launch_path() -> Result<String, String> {
    if let Ok(appimage) = std::env::var("APPIMAGE") {
        return Ok(appimage);
    }
    let exe = std::env::current_exe().map_err(|e| format!("could not find the app's path: {e}"))?;
    Ok(exe.to_string_lossy().into_owned())
}

/// Adds our entry to gsettings' list of custom shortcuts. None if it's already there.
fn with_entry(listed: &str) -> Option<String> {
    let inner = listed.trim_start_matches("@as").trim().trim_start_matches('[').trim_end_matches(']');
    let mut paths: Vec<&str> = inner.split(',').map(|p| p.trim().trim_matches('\'')).filter(|p| !p.is_empty()).collect();
    if paths.contains(&ENTRY_PATH) {
        return None;
    }
    paths.push(ENTRY_PATH);
    Some(format!("[{}]", paths.iter().map(|p| format!("'{p}'")).collect::<Vec<_>>().join(", ")))
}

/// gsettings reads values as GVariant text, so strings need quotes.
fn gvariant_str(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// GNOME splits the command like a shell does.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// "Ctrl+Alt+V" becomes "<Control><Alt>v".
fn accelerator(shortcut: &Shortcut) -> Option<String> {
    let mut accel = String::new();
    for (flag, name) in [
        (Modifiers::CONTROL, "<Control>"),
        (Modifiers::ALT, "<Alt>"),
        (Modifiers::SHIFT, "<Shift>"),
        (Modifiers::SUPER, "<Super>"),
    ] {
        if shortcut.mods.contains(flag) {
            accel.push_str(name);
        }
    }
    accel.push_str(&keysym(shortcut.key)?);
    Some(accel)
}

/// GTK key names for the keys the settings page can record.
fn keysym(code: Code) -> Option<String> {
    let name = format!("{code:?}");
    if let Some(letter) = name.strip_prefix("Key") {
        return Some(letter.to_lowercase());
    }
    if let Some(digit) = name.strip_prefix("Digit") {
        return Some(digit.to_string());
    }
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if name.strip_prefix('F').is_some_and(digits) {
        return Some(name);
    }
    if let Some(rest) = name.strip_prefix("Numpad") {
        if digits(rest) || ["Add", "Subtract", "Multiply", "Divide", "Decimal", "Enter", "Equal"].contains(&rest) {
            return Some(format!("KP_{rest}"));
        }
        return None;
    }
    let named = match code {
        Code::Space => "space",
        Code::Enter => "Return",
        Code::Tab => "Tab",
        Code::Backspace => "BackSpace",
        Code::Delete => "Delete",
        Code::Insert => "Insert",
        Code::Home => "Home",
        Code::End => "End",
        Code::PageUp => "Page_Up",
        Code::PageDown => "Page_Down",
        Code::ArrowUp => "Up",
        Code::ArrowDown => "Down",
        Code::ArrowLeft => "Left",
        Code::ArrowRight => "Right",
        Code::Minus => "minus",
        Code::Equal => "equal",
        Code::BracketLeft => "bracketleft",
        Code::BracketRight => "bracketright",
        Code::Backslash => "backslash",
        Code::Semicolon => "semicolon",
        Code::Quote => "apostrophe",
        Code::Comma => "comma",
        Code::Period => "period",
        Code::Slash => "slash",
        Code::Backquote => "grave",
        _ => return None,
    };
    Some(named.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accel(text: &str) -> Option<String> {
        accelerator(&text.parse::<Shortcut>().unwrap())
    }

    #[test]
    fn converts_shortcuts_to_gnome_accelerators() {
        assert_eq!(accel("Ctrl+Alt+V").as_deref(), Some("<Control><Alt>v"));
        assert_eq!(accel("Super+Shift+F12").as_deref(), Some("<Shift><Super>F12"));
        assert_eq!(accel("Ctrl+Alt+PageDown").as_deref(), Some("<Control><Alt>Page_Down"));
        assert_eq!(accel("Alt+Digit1").as_deref(), Some("<Alt>1"));
        assert_eq!(accel("Ctrl+Numpad5").as_deref(), Some("<Control>KP_5"));
        assert_eq!(accel("Ctrl+Backquote").as_deref(), Some("<Control>grave"));
    }

    #[test]
    fn other_spellings_of_modifiers_work_too() {
        assert_eq!(accel("Control+Option+v"), accel("Ctrl+Alt+V"));
        assert_eq!(accel("Cmd+K").as_deref(), Some("<Super>k"));
    }

    #[test]
    fn adds_the_entry_to_the_list_once() {
        let ours = format!("['{ENTRY_PATH}']");
        assert_eq!(with_entry("@as []"), Some(ours.clone()));
        assert_eq!(with_entry(&ours), None);
        assert_eq!(
            with_entry("['/custom0/']"),
            Some(format!("['/custom0/', '{ENTRY_PATH}']"))
        );
    }

    #[test]
    fn quotes_paths_with_spaces_and_quotes() {
        assert_eq!(shell_quote("/opt/My App/run"), "'/opt/My App/run'");
        assert_eq!(gvariant_str("'/a b/x' --toggle"), r"'\'/a b/x\' --toggle'");
    }

    #[test]
    fn detects_gnome_wayland_only() {
        assert!(is_gnome_wayland(Some("wayland"), Some("ubuntu:GNOME")));
        assert!(!is_gnome_wayland(Some("x11"), Some("ubuntu:GNOME")));
        assert!(!is_gnome_wayland(Some("wayland"), Some("KDE")));
        assert!(!is_gnome_wayland(None, None));
    }
}
