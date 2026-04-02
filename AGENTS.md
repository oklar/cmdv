# AGENTS.md

## Cursor Cloud specific instructions

### Product overview

CMDV is a Tauri v2 desktop application (React 19 + Rust) — a zero-knowledge encrypted clipboard manager. There are **no external services or databases** to run; SQLite (SQLCipher) is embedded and the encrypted DB is created automatically at app launch.

### Prerequisites (system-level, already installed in snapshot)

- Node.js LTS (v22+), npm
- Rust stable (≥1.77.2; many dependencies require ≥1.85, so keep Rust up to date via `rustup update stable && rustup default stable`)
- Linux system packages: `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev libsecret-1-dev patchelf libgtk-3-dev`

### Commands quick-reference

| Task | Command | Notes |
|------|---------|-------|
| Install JS deps | `npm install` | Uses npm (lockfile: `package-lock.json`) |
| Lint (ESLint) | `npm run lint` | |
| Frontend build | `npm run build` | `tsc -b && vite build` |
| Rust check | `cargo check` | Run from `src-tauri/` |
| Rust tests | `cargo test` | Run from `src-tauri/`; 44 unit tests |
| Dev mode | `npm run tauri dev` | Starts Vite dev server (port 5173) + compiles & runs Rust backend |
| Production build | `npm run tauri build` | Outputs to `src-tauri/target/release/bundle/` |

### Non-obvious caveats

- **Rust toolchain version**: The default Rust in the base image may be too old (1.83). Dependencies like `dlopen2`, `image`, `zbus` require Rust ≥1.85–1.88. Always run `rustup default stable` to ensure the latest stable toolchain is active before building.
- **Linux keyring**: The app stores encryption keys in the OS keychain via `libsecret` / gnome-keyring. In a headless/cloud VM, a "login" keyring collection must exist. If the app crashes on startup with a keychain error, create one via `seahorse` or `secret-tool` before running.
- **Display server required**: This is a GUI desktop app. A display server (X11/Xvfb) must be running (`DISPLAY=:1` or similar). The Cloud Agent VM already provides this.
- **Global shortcut**: The app registers `Ctrl+U` as the global shortcut to show/hide the window. It also runs as a system tray app.
- **First-run setup**: On first launch, the app shows a welcome wizard requiring a vault password. After setup, it generates a 24-word BIP39 recovery phrase and starts monitoring the clipboard.
- **No Cargo.lock committed**: `Cargo.lock` is not committed to the repo (common for applications using latest compatible versions). The first `cargo check`/`cargo build` will resolve and download all crate dependencies.
