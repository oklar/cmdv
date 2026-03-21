---
name: Start minimized OS autostart
overview: Register Cmdv for login autostart with a --tray argument; parse it at runtime and, after vault unlock, hide the main window once and show a desktop notification. Manual launches omit --tray and behave as today.
todos:
  - id: plugin-autostart
    content: Add tauri-plugin-autostart (Cargo, npm, lib.rs Builder with args, capabilities)
    status: completed
  - id: cli-tray-flag
    content: Parse --tray at startup; expose to hide/notify command (OnceLock or static)
    status: completed
  - id: command-hide-notify
    content: Tauri command hide main + notify_rust; call once from App when unlocked
    status: completed
  - id: settings-toggle
    content: Settings toggle wires autostart enable/disable + is_enabled for UI state
    status: completed
isProject: true
---

# Login autostart: tray + notification

## Problem

Distinguish **OS-driven startup** (user logs in; shell runs the app) from **user-driven startup** (shortcut, Start menu). Only the former should minimize to tray and show a “running in tray” notification.

## Approach

**Autostart entry carries a flag.** Use [tauri-plugin-autostart](https://v2.tauri.app/plugin/autostart/) with `Builder::new().args(["--tray"]).build()` (exact API per current plugin version) so the scheduled login command is `Cmdv.exe --tray` (Linux: same argv pattern). A normal desktop shortcut or `cmdv` with no args does not pass `--tray`.

**Parse once at process start.** `std::env::args()` contains `--tray` iff this process was started from that entry. Store the result in `std::sync::OnceLock<bool>` or equivalent so any command can read it without reparsing.

**When to hide.** Setup wizard and lock screen must remain visible; hiding only after `get_vault_status` resolves to an unlocked vault (same gate as other “main UI ready” logic). Implement as: when `appState === "unlocked"` in `[src/App.tsx](src/App.tsx)`, call a single Tauri command once per process (ref guard) that: if the tray flag is true, `WebviewWindow::hide("main")` and fire `notify_rust` (reuse patterns from `[hide_to_tray](src-tauri/src/lib.rs)` / `notify_update_available`).

**Settings.** One control, e.g. “Open at login (minimized to tray)”, bound to the plugin’s `enable()` / `disable()` and initial state from `is_enabled()`. Enabling registration must be the path that includes `--tray` in the autostart invocation; disabling removes the entry.

## Integration points

| Area                                                                         | Change                                                                                          |
| ---------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `[src-tauri/Cargo.toml](src-tauri/Cargo.toml)`                               | `tauri-plugin-autostart` dependency                                                             |
| `[package.json](package.json)`                                               | `@tauri-apps/plugin-autostart`                                                                  |
| `[src-tauri/src/lib.rs](src-tauri/src/lib.rs)`                               | `.plugin(autostart Builder with args)`, tray flag init, new `#[tauri::command]` for hide+notify |
| `[src-tauri/capabilities/desktop.json](src-tauri/capabilities/desktop.json)` | Autostart permissions per Tauri v2                                                              |
| `[src/App.tsx](src/App.tsx)`                                                 | `useEffect` + `useRef` when `appState === "unlocked"`                                           |
| `[src/components/Settings.tsx](src/components/Settings.tsx)`                 | Toggle + plugin imports                                                                         |

## Edge cases

- **macOS:** Autostart uses Launch Agent / different mechanics; validate against plugin docs.
- **Flash before hide:** Window may appear briefly before unlock; acceptable unless you later add `visible: false` for `--tray` only (ordering vs webview load).
- **Double invocation:** Guard so hide+notify runs once per process even if React remounts.

## Out of scope

- “Always start minimized” for every manual launch (separate product decision).
- Installer registering autostart without the app (user-controlled toggle only).

## Verification

- Toggle on → entry registered; log out/in → process has `--tray`, after unlock: hidden + notification; tray opens window.
- Toggle off → no login launch.
- Start from Explorer / menu without `--tray` → window visible, no tray-only notification.
