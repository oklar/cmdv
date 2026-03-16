<p align="center">
  <img src="src/assets/icon.png" width="128" height="128" alt="CMDV icon" />
</p>

<h1 align="center">CMDV</h1>

<p align="center">
  Zero-knowledge encrypted clipboard manager for Windows and Linux.
</p>

<p align="center">
  <a href="https://github.com/oklar/cmdv/releases/latest">Download</a> ·
  <a href="#features">Features</a> ·
  <a href="#development">Development</a>
</p>

---

## Features

- **Clipboard history** — automatically captures text, images, and files you copy
- **Zero-knowledge encryption** — all data is encrypted locally with AES-256-GCM before it leaves your machine
- **Mnemonic recovery** — BIP-39 mnemonic phrase for key backup and device pairing
- **Cloud sync** — optional end-to-end encrypted sync across devices
- **Sensitive data detection** — automatically flags passwords, tokens, and API keys
- **Search and filter** — full-text search with content type filtering and favorites
- **System tray** — runs quietly in the background with a global shortcut (Ctrl+U)
- **Auto-updates** — the app silently updates itself from GitHub Releases
- **Cross-platform** — Windows (NSIS installer) and Linux (AppImage, .deb)

## Architecture

| Layer       | Technology                                                      |
| ----------- | --------------------------------------------------------------- |
| Frontend    | React 19, Tailwind CSS 4, TypeScript                            |
| Backend     | Rust, Tauri v2                                                  |
| Database    | SQLite with SQLCipher (encrypted at rest)                       |
| Crypto      | AES-256-GCM, Argon2, BLAKE3, HKDF-SHA256                        |
| Key storage | OS keychain (Windows Credential Manager / Linux Secret Service) |
| Sync        | Client-side encryption → REST API → R2 blob storage             |

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

Releases are automated via GitHub Actions. To publish a new version:

1. Bump `version` in `src-tauri/tauri.conf.json`
2. Commit the change
3. Tag and push:

```bash
git tag v0.1.0
git push --tags
```

4. GitHub Actions builds Windows + Linux artifacts and creates a **draft** release
5. Review the draft on GitHub, then publish

Existing users receive the update automatically on next app launch.

## License

[MIT](LICENSE)
