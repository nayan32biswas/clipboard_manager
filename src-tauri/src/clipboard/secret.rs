//! Detects clips that password managers mark as secret.

/// `formats` are the clipboard format names on offer. `read` fetches a
/// format's bytes, for markers whose value matters.
pub fn is_secret(formats: &[String], read: impl Fn(&str) -> Option<Vec<u8>>) -> bool {
    formats.iter().any(|format| match format.as_str() {
        // Windows: set by password managers and by Windows itself for some apps.
        "ExcludeClipboardContentFromMonitorProcessing" => true,
        // Windows: a DWORD. 0 means "keep out of clipboard history".
        "CanIncludeInClipboardHistory" => read(format)
            .is_some_and(|v| v.len() >= 4 && v[..4] == [0, 0, 0, 0]),
        // macOS (nspasteboard.org convention).
        "org.nspasteboard.ConcealedType" => true,
        // Linux: set by KeePassXC.
        "x-kde-passwordManagerHint" => read(format).is_some_and(|v| v.trim_ascii() == b"secret"),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(formats: &[&str], value: &[u8]) -> bool {
        let formats: Vec<String> = formats.iter().map(|f| f.to_string()).collect();
        is_secret(&formats, |_| Some(value.to_vec()))
    }

    #[test]
    fn plain_clip_is_not_secret() {
        assert!(!check(&["CF_UNICODETEXT", "text/plain"], b""));
    }

    #[test]
    fn windows_exclude_marker_is_secret() {
        assert!(check(&["ExcludeClipboardContentFromMonitorProcessing"], b""));
    }

    #[test]
    fn windows_history_flag_is_secret_only_when_zero() {
        assert!(check(&["CanIncludeInClipboardHistory"], &[0, 0, 0, 0]));
        assert!(!check(&["CanIncludeInClipboardHistory"], &[1, 0, 0, 0]));
    }

    #[test]
    fn macos_concealed_type_is_secret() {
        assert!(check(&["public.utf8-plain-text", "org.nspasteboard.ConcealedType"], b""));
    }

    #[test]
    fn kde_password_hint_is_secret_only_when_value_is_secret() {
        assert!(check(&["x-kde-passwordManagerHint"], b"secret"));
        assert!(!check(&["x-kde-passwordManagerHint"], b"other"));
    }
}
