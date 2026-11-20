const EXCLUDED_APPS: &[&str] = &[
    "1password",
    "bitwarden",
    "keepass",
    "keepassxc",
    "lastpass",
    "dashlane",
    "enpass",
    "nordpass",
];

pub fn get_foreground_app() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        get_foreground_app_windows()
    }
    #[cfg(target_os = "macos")]
    {
        get_foreground_app_macos()
    }
    #[cfg(target_os = "linux")]
    {
        None
    }
}

pub fn clipboard_marked_private() -> bool {
    #[cfg(target_os = "windows")]
    {
        clipboard_marked_private_windows()
    }
    #[cfg(target_os = "linux")]
    {
        clipboard_marked_private_x11().unwrap_or(false)
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        false
    }
}

/// arboard reads the clipboard over X11 (XWayland on Wayland), so the hint is read the same way.
#[cfg(target_os = "linux")]
fn clipboard_marked_private_x11() -> Option<bool> {
    use std::time::{Duration, Instant};
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, CreateWindowAux, WindowClass};
    use x11rb::protocol::Event;
    use x11rb::{COPY_DEPTH_FROM_PARENT, COPY_FROM_PARENT, CURRENT_TIME, NONE};

    let (conn, screen_num) = x11rb::connect(None).ok()?;
    let atom = |name: &[u8]| -> Option<u32> {
        Some(conn.intern_atom(false, name).ok()?.reply().ok()?.atom)
    };
    let clipboard = atom(b"CLIPBOARD")?;
    let hint = atom(b"x-kde-passwordManagerHint")?;
    let property = atom(b"CMDV_PASSWORD_HINT")?;

    if conn.get_selection_owner(clipboard).ok()?.reply().ok()?.owner == NONE {
        return Some(false);
    }

    let window = conn.generate_id().ok()?;
    conn.create_window(
        COPY_DEPTH_FROM_PARENT,
        window,
        conn.setup().roots[screen_num].root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_ONLY,
        COPY_FROM_PARENT,
        &CreateWindowAux::new(),
    )
    .ok()?;
    conn.convert_selection(window, clipboard, hint, property, CURRENT_TIME)
        .ok()?;
    conn.flush().ok()?;

    let deadline = Instant::now() + Duration::from_millis(200);
    while Instant::now() < deadline {
        match conn.poll_for_event().ok()? {
            Some(Event::SelectionNotify(event)) if event.requestor == window => {
                if event.property == NONE {
                    return Some(false);
                }
                let reply = conn
                    .get_property(true, window, property, AtomEnum::ANY, 0, 64)
                    .ok()?
                    .reply()
                    .ok()?;
                return Some(reply.value.starts_with(b"secret"));
            }
            Some(_) => {}
            None => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn clipboard_format(name: &str) -> u32 {
    use windows_sys::Win32::System::DataExchange::RegisterClipboardFormatW;
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe { RegisterClipboardFormatW(wide.as_ptr()) }
}

#[cfg(target_os = "windows")]
fn clipboard_marked_private_windows() -> bool {
    use windows_sys::Win32::System::DataExchange::IsClipboardFormatAvailable;

    for name in [
        "ExcludeClipboardContentFromMonitorProcessing",
        "Clipboard Viewer Ignore",
    ] {
        let format = clipboard_format(name);
        if format != 0 && unsafe { IsClipboardFormatAvailable(format) } != 0 {
            return true;
        }
    }

    let history = clipboard_format("CanIncludeInClipboardHistory");
    if history != 0 && unsafe { IsClipboardFormatAvailable(history) } != 0 {
        // If the value can't be read right now, skip this poll; the next one tries again.
        return read_clipboard_u32(history).map_or(true, |value| value == 0);
    }

    false
}

#[cfg(target_os = "windows")]
fn read_clipboard_u32(format: u32) -> Option<u32> {
    use windows_sys::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    unsafe {
        let mut opened = false;
        for _ in 0..5 {
            if OpenClipboard(core::ptr::null_mut()) != 0 {
                opened = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if !opened {
            return None;
        }

        let mut value = None;
        let handle = GetClipboardData(format);
        if !handle.is_null() && GlobalSize(handle) >= 4 {
            let ptr = GlobalLock(handle) as *const u32;
            if !ptr.is_null() {
                value = Some(ptr.read_unaligned());
                GlobalUnlock(handle);
            }
        }
        CloseClipboard();
        value
    }
}

pub fn is_excluded_app(app_name: &str) -> bool {
    let lower = app_name.to_lowercase();
    EXCLUDED_APPS
        .iter()
        .any(|excluded| lower.contains(excluded))
}

pub fn is_excluded_with_custom(app_name: &str, custom_list: &[String]) -> bool {
    let lower = app_name.to_lowercase();
    EXCLUDED_APPS
        .iter()
        .any(|excluded| lower.contains(excluded))
        || custom_list
            .iter()
            .any(|excluded| lower.contains(&excluded.to_lowercase()))
}

#[cfg(target_os = "windows")]
fn get_foreground_app_windows() -> Option<String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }

        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }

        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }

        let mut buf = [0u16; 260];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size);
        CloseHandle(handle);

        if ok == 0 {
            return None;
        }

        let path = String::from_utf16_lossy(&buf[..size as usize]);
        path.rsplit('\\').next().map(|s| s.to_string())
    }
}

#[cfg(target_os = "macos")]
fn get_foreground_app_macos() -> Option<String> {
    use std::process::Command;
    let output = Command::new("osascript")
        .args(["-e", "tell application \"System Events\" to get name of first application process whose frontmost is true"])
        .output()
        .ok()?;
    if output.status.success() {
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    } else {
        None
    }
}
