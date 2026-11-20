pub mod monitor;
pub mod source;

pub use monitor::ClipboardMonitor;

/// Puts secret text on the clipboard marked so clipboard monitors and history skip it:
/// `ExcludeClipboardContentFromMonitorProcessing` on Windows (which also keeps it out of
/// clipboard history and the cloud clipboard), KDE's password-manager hint on Linux.
pub fn set_private_text(text: &str) -> Result<(), String> {
    let mut last_err = String::new();
    for attempt in 0..5 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let mut clipboard = match arboard::Clipboard::new() {
            Ok(c) => c,
            Err(e) => {
                last_err = e.to_string();
                continue;
            }
        };
        let set = clipboard.set();
        #[cfg(target_os = "windows")]
        let set = {
            use arboard::SetExtWindows;
            set.exclude_from_monitoring()
        };
        #[cfg(target_os = "linux")]
        let set = {
            use arboard::SetExtLinux;
            set.exclude_from_history()
        };
        #[cfg(target_os = "macos")]
        let set = {
            use arboard::SetExtApple;
            set.exclude_from_history()
        };
        match set.text(text) {
            Ok(()) => return Ok(()),
            Err(e) => last_err = e.to_string(),
        }
    }
    Err(format!("Could not copy to the clipboard: {last_err}"))
}

/// Empties the clipboard if it still holds the text whose SHA-256 is `digest`.
pub fn clear_if_unchanged(digest: &[u8; 32]) {
    use sha2::{Digest, Sha256};
    use zeroize::Zeroize;

    let Ok(mut clipboard) = arboard::Clipboard::new() else {
        return;
    };
    let Ok(mut current) = clipboard.get_text() else {
        return;
    };
    let same = Sha256::digest(current.as_bytes()).as_slice() == digest;
    current.zeroize();
    if same {
        let _ = clipboard.clear();
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    /// Uses the real Windows clipboard, so it's ignored by default:
    /// `cargo test -- --ignored private_clipboard`. Every write is marked private, so nothing
    /// lands in Windows clipboard history or a running clipboard manager, and the previous text
    /// is put back at the end.
    #[test]
    #[ignore]
    fn private_clipboard_round_trip() {
        let mut clipboard = arboard::Clipboard::new().unwrap();
        let previous = clipboard.get_text().ok();

        set_private_text("cmdv private clipboard test").unwrap();
        assert!(source::clipboard_marked_private(), "private copy is detected");

        clipboard.clear().unwrap();
        assert!(!source::clipboard_marked_private(), "an empty clipboard is not flagged");

        {
            use arboard::SetExtWindows;
            clipboard
                .set()
                .exclude_from_history()
                .text("cmdv history flag test")
                .unwrap();
        }
        assert!(
            source::clipboard_marked_private(),
            "CanIncludeInClipboardHistory = 0 is detected"
        );

        let digest: [u8; 32] = Sha256::digest(b"cmdv history flag test").into();
        set_private_text("something copied later").unwrap();
        clear_if_unchanged(&digest);
        assert_eq!(
            clipboard.get_text().unwrap(),
            "something copied later",
            "a clipboard that changed since is left alone"
        );

        set_private_text("cmdv history flag test").unwrap();
        clear_if_unchanged(&digest);
        assert!(
            clipboard.get_text().map(|t| t.is_empty()).unwrap_or(true),
            "the copied text is cleared"
        );

        match previous {
            Some(text) => set_private_text(&text).unwrap(),
            None => {
                let _ = clipboard.clear();
            }
        }
    }
}
