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

pub fn is_excluded_app(app_name: &str) -> bool {
    let lower = app_name.to_lowercase();
    EXCLUDED_APPS.iter().any(|excluded| lower.contains(excluded))
}

pub fn is_excluded_with_custom(app_name: &str, custom_list: &[String]) -> bool {
    let lower = app_name.to_lowercase();
    EXCLUDED_APPS.iter().any(|excluded| lower.contains(excluded))
        || custom_list.iter().any(|excluded| lower.contains(&excluded.to_lowercase()))
}

#[cfg(target_os = "windows")]
fn get_foreground_app_windows() -> Option<String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    use windows_sys::Win32::Foundation::CloseHandle;

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
        if name.is_empty() { None } else { Some(name) }
    } else {
        None
    }
}
