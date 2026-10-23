//! User settings, stored as one JSON value per key in the `settings` table.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::store::Store;

/// Sent to the UI in camelCase. Stored under the snake_case keys in `load`/`save`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub retention_days: u32,
    pub capture_images: bool,
    pub skip_secrets: bool,
    pub ignored_apps: Vec<String>,
    pub hotkey: String,
    pub launch_at_login: bool,
    /// Also put picked text on the Linux primary selection.
    pub primary_selection: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            retention_days: 3,
            capture_images: true,
            skip_secrets: false,
            ignored_apps: Vec::new(),
            hotkey: "Ctrl+Alt+V".into(),
            launch_at_login: true,
            primary_selection: true,
        }
    }
}

impl Settings {
    /// Missing or unreadable keys fall back to their defaults.
    pub fn load(store: &Store) -> rusqlite::Result<Settings> {
        let d = Settings::default();
        Ok(Settings {
            retention_days: read(store, "retention_days", d.retention_days)?.clamp(1, 365),
            capture_images: read(store, "capture_images", d.capture_images)?,
            skip_secrets: read(store, "skip_secrets", d.skip_secrets)?,
            ignored_apps: read(store, "ignored_apps", d.ignored_apps)?,
            hotkey: read(store, "hotkey", d.hotkey)?,
            launch_at_login: read(store, "launch_at_login", d.launch_at_login)?,
            primary_selection: read(store, "primary_selection", d.primary_selection)?,
        })
    }

    pub fn save(&self, store: &Store) -> rusqlite::Result<()> {
        write(store, "retention_days", &self.retention_days)?;
        write(store, "capture_images", &self.capture_images)?;
        write(store, "skip_secrets", &self.skip_secrets)?;
        write(store, "ignored_apps", &self.ignored_apps)?;
        write(store, "hotkey", &self.hotkey)?;
        write(store, "launch_at_login", &self.launch_at_login)?;
        write(store, "primary_selection", &self.primary_selection)
    }
}

fn read<T: DeserializeOwned>(store: &Store, key: &str, default: T) -> rusqlite::Result<T> {
    let value = store.get_setting(key)?;
    Ok(value.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or(default))
}

fn write<T: Serialize>(store: &Store, key: &str, value: &T) -> rusqlite::Result<()> {
    let json = serde_json::to_string(value).expect("settings values always serialize");
    store.set_setting(key, &json)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Must match `Settings` in src/api.ts.
    #[test]
    fn ui_sees_camel_case_field_names() {
        let json = serde_json::to_value(Settings::default()).unwrap();
        let mut keys: Vec<_> = json.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            ["captureImages", "hotkey", "ignoredApps", "launchAtLogin", "primarySelection", "retentionDays", "skipSecrets"]
        );
        let back: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(back, Settings::default());
    }

    #[test]
    fn empty_store_gives_defaults() {
        let store = Store::open_in_memory().unwrap();
        let s = Settings::load(&store).unwrap();
        assert_eq!(s, Settings::default());
        assert_eq!(s.retention_days, 3);
        assert!(!s.skip_secrets, "secret filter is off by default");
        assert!(s.primary_selection);
    }

    #[test]
    fn saved_settings_load_back() {
        let store = Store::open_in_memory().unwrap();
        let s = Settings {
            retention_days: 30,
            capture_images: false,
            skip_secrets: true,
            ignored_apps: vec!["KeePassXC".into()],
            hotkey: "Ctrl+Shift+H".into(),
            launch_at_login: false,
            primary_selection: false,
        };
        s.save(&store).unwrap();
        assert_eq!(Settings::load(&store).unwrap(), s);
    }

    #[test]
    fn retention_is_clamped_to_1_through_365() {
        let store = Store::open_in_memory().unwrap();
        Settings { retention_days: 0, ..Default::default() }.save(&store).unwrap();
        assert_eq!(Settings::load(&store).unwrap().retention_days, 1);
        Settings { retention_days: 9999, ..Default::default() }.save(&store).unwrap();
        assert_eq!(Settings::load(&store).unwrap().retention_days, 365);
    }

    #[test]
    fn a_corrupt_key_falls_back_without_losing_others() {
        let store = Store::open_in_memory().unwrap();
        Settings { skip_secrets: true, ..Default::default() }.save(&store).unwrap();
        store.set_setting("retention_days", "not json").unwrap();
        let s = Settings::load(&store).unwrap();
        assert_eq!(s.retention_days, 3);
        assert!(s.skip_secrets);
    }
}
