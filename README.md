<p align="center">
  <img src="src/assets/icon-bg-black.png" width="128" height="128" alt="CMDV icon" />
</p>

<h1 align="center">CMDV</h1>

<p align="center">
  Encrypted clipboard manager for Windows and Linux.
</p>

<p align="center">
  <a href="https://github.com/oklar/cmdv/actions/workflows/ci.yml"><img src="https://github.com/oklar/cmdv/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="https://github.com/oklar/cmdv/actions/workflows/release.yml"><img src="https://github.com/oklar/cmdv/actions/workflows/release.yml/badge.svg" alt="Release" /></a>
  <a href="https://github.com/oklar/cmdv/releases/latest"><img src="https://img.shields.io/github/v/release/oklar/cmdv" alt="Latest release" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/oklar/cmdv" alt="MIT License" /></a>
</p>

<p align="center">
  <a href="https://tauri.app/"><img src="https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white" alt="Tauri 2" /></a>
  <a href="https://react.dev/"><img src="https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=black" alt="React 19" /></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-stable-orange?logo=rust&logoColor=white" alt="Rust" /></a>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20Linux-0078D6" alt="Windows and Linux" />
  <img src="https://img.shields.io/badge/storage-SQLCipher-003B57" alt="SQLCipher" />
</p>

<p align="center">
  <a href="https://github.com/oklar/cmdv/releases/latest">Download</a> ·
  <a href="#features">Features</a> ·
  <a href="#development">Development</a>
</p>

---

## Features

- **Clipboard history** — automatically captures text and images you copy
- **Encrypted vault** — history is stored in a SQLCipher database; keys live in the OS keychain (Windows Credential Manager / Linux Secret Service)
- **Vault password + recovery phrase** — BIP-39 mnemonic for recovery and device pairing (QR)
- **Secure paste** — hotkey (Ctrl+Shift+C) creates a one-time encrypted link; ciphertext is uploaded, the decryption key stays in the URL fragment (not sent to the server). Paste links opened in a browser can hand off to the desktop app via `cmdv://`
- **Password-manager awareness** — skips capture when the foreground app is a known password manager (Windows)
- **Search and filter** — full-text search with content type filtering and favorites
- **Encrypted backup** — export and import an encrypted blob of your history (manual sync between devices today)
- **System tray** — runs in the background; global shortcut Ctrl+U to open
- **Auto-updates** — checks GitHub Releases on launch; install from Settings
- **Cross-platform** — Windows (NSIS installer) and Linux (AppImage, .deb)

### Planned

- **Cloud sync** — optional sync across devices; client-side encryption before upload (zero-knowledge on the server)
- **File clipboard** — capture file paths from the clipboard
- **macOS Universal Links** — associate `cmdv.to` directly with the app (Apple entitlements + `.well-known/apple-app-site-association`)
- **Mobile deep links** — `tauri-plugin-deep-link` supports verified HTTPS on iOS/Android
- **Deep link URL versioning** — backward compat strategy if `cmdv://` format evolves

## Architecture

| Layer        | Technology                                                      |
| ------------ | --------------------------------------------------------------- |
| Frontend     | React 19, Tailwind CSS 4, TypeScript                            |
| Backend      | Rust, Tauri v2                                                  |
| Database     | SQLite with SQLCipher (encrypted at rest)                       |
| Crypto       | AES-256-GCM (vault, backups), AES-128-GCM (secure paste), Argon2, BLAKE3, HKDF-SHA256 |
| Key storage  | OS keychain                                                     |
| Secure paste | Encrypted upload to API; paste site decrypts in the browser using the URL fragment |

## Download

Grab the latest release from the [Releases page](https://github.com/oklar/cmdv/releases/latest).

| Platform | Format               | File                        |
| -------- | -------------------- | --------------------------- |
| Windows  | NSIS installer       | `cmdv_x.y.z_x64-setup.exe`  |
| Linux    | AppImage (universal) | `cmdv_x.y.z_amd64.AppImage` |
| Linux    | Debian package       | `cmdv_x.y.z_amd64.deb`      |

**Windows note:** The installer is currently unsigned, so SmartScreen may show an "unknown publisher" warning. Click "More info" → "Run anyway" to proceed.

**Linux AppImage:** Make it executable and run:

```bash
chmod +x cmdv_*.AppImage
./cmdv_*.AppImage
```

## Development

### Prerequisites

- [Node.js](https://nodejs.org/) (LTS)
- [Rust](https://rustup.rs/) (stable)
- Tauri v2 system dependencies:
  - **Windows:** WebView2 (included in Windows 10/11)
  - **Linux:** `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev libsecret-1-dev patchelf`

### Setup

```bash
git clone https://github.com/oklar/cmdv.git
cd cmdv
npm install
```

### Run in development

```bash
npm run tauri dev
```

### Build for production

```bash
npm run tauri build
```

Bundles are output to `src-tauri/target/release/bundle/`.

## Releasing

Releases are automated via GitHub Actions ([Release workflow](https://github.com/oklar/cmdv/actions/workflows/release.yml)). Pushing to `main` only runs CI — **Release runs when you push a tag**.

To publish a new version:

1. Bump `version` in `src-tauri/tauri.conf.json`
2. Commit the change
3. Push the commit, then tag **that commit** and push the tag (tag name must match the config, e.g. `"0.9.4"` → `v0.9.4`):

```bash
git push origin main
git tag v0.9.4
git push origin v0.9.4
```

4. The **Release** workflow builds Windows + Linux artifacts and creates a **draft** release (~15–20 min)
5. Review the draft on GitHub, then publish

Existing users receive the update automatically on next app launch.

Avoid `git push --tags` for releases — it pushes every local tag and is easy to tag the wrong commit before the version bump is on `main`. To re-trigger a release after fixing a tag, delete it on the remote and push again:

```bash
git push origin :refs/tags/v0.9.4
git tag -d v0.9.4
git tag v0.9.4
git push origin v0.9.4
```
