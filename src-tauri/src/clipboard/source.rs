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

#[cfg(target_os = "windows")]
fn get_foreground_app_windows() -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
fn get_foreground_app_macos() -> Option<String> {
    None
}
