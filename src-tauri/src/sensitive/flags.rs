pub fn is_clipboard_concealed() -> bool {
    #[cfg(target_os = "windows")]
    {
        check_windows_concealed()
    }
    #[cfg(target_os = "macos")]
    {
        check_macos_concealed()
    }
    #[cfg(target_os = "linux")]
    {
        false
    }
}

#[cfg(target_os = "windows")]
fn check_windows_concealed() -> bool {
    use windows_sys::Win32::System::DataExchange::{
        OpenClipboard, CloseClipboard, IsClipboardFormatAvailable,
        RegisterClipboardFormatW,
    };
    use std::sync::OnceLock;

    static FORMAT_ID: OnceLock<u32> = OnceLock::new();

    let format = *FORMAT_ID.get_or_init(|| {
        let name: Vec<u16> = "ExcludeClipboardContentFromMonitorProcessing\0"
            .encode_utf16()
            .collect();
        unsafe { RegisterClipboardFormatW(name.as_ptr()) }
    });

    if format == 0 {
        return false;
    }

    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return false;
        }
        let result = IsClipboardFormatAvailable(format) != 0;
        CloseClipboard();
        result
    }
}

#[cfg(target_os = "macos")]
fn check_macos_concealed() -> bool {
    use std::process::Command;
    // Check if the pasteboard has the concealed type via pbpaste metadata
    // This is a lightweight check using osascript
    let output = Command::new("osascript")
        .args(["-e", "tell application \"System Events\" to get clipboard info"])
        .output();
    match output {
        Ok(out) => {
            let s = String::from_utf8_lossy(&out.stdout);
            s.contains("org.nspasteboard.ConcealedType")
        }
        Err(_) => false,
    }
}
