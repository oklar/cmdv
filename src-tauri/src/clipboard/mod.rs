pub mod monitor;
pub mod source;

pub use monitor::ClipboardMonitor;

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

    fn excluded_from_monitoring() -> bool {
        use windows_sys::Win32::System::DataExchange::{
            IsClipboardFormatAvailable, RegisterClipboardFormatW,
        };
        let name: Vec<u16> = "ExcludeClipboardContentFromMonitorProcessing"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        unsafe { IsClipboardFormatAvailable(RegisterClipboardFormatW(name.as_ptr())) != 0 }
    }

    /// Uses the real clipboard; writes are marked private and the previous text is restored.
    #[test]
    #[ignore]
    fn private_clipboard_round_trip() {
        let mut clipboard = arboard::Clipboard::new().unwrap();
        let previous = clipboard.get_text().ok();

        set_private_text("cmdv private clipboard test").unwrap();
        assert!(excluded_from_monitoring(), "the copy is marked private");

        let digest: [u8; 32] = Sha256::digest(b"cmdv private clipboard test").into();
        set_private_text("something copied later").unwrap();
        clear_if_unchanged(&digest);
        assert_eq!(
            clipboard.get_text().unwrap(),
            "something copied later",
            "a clipboard that changed since is left alone"
        );

        set_private_text("cmdv private clipboard test").unwrap();
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
