# CMDV

Zero-knowledge encrypted clipboard manager built with Tauri v2 (Rust + React + TypeScript).

## Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| **Node.js** | >= 20 | https://nodejs.org/ |
| **Rust** | >= 1.77.2 | https://rustup.rs/ |
| **Tauri CLI** | v2 | Included as dev dependency |

### Windows-specific

The Rust backend compiles **OpenSSL from source** (required by SQLCipher and the notification plugin). This needs a full Perl distribution — Git Bash's bundled Perl is missing modules and will fail.

1. Install [Strawberry Perl](https://strawberryperl.com/) (free, ~200 MB).

2. Git Bash (used by Cursor/VS Code terminals) automatically prepends its own Perl (`C:\Program Files\Git\usr\bin\perl.exe`) to PATH at startup, which shadows Strawberry Perl. To fix this, add the following line to `~/.bashrc` (i.e. `C:\Users\<you>\.bashrc`):

   ```bash
   export PATH="/c/Strawberry/perl/bin:$PATH"
   ```

3. Restart your terminal after saving.

Verify the correct Perl is active:

```bash
which perl
# Should show: /c/Strawberry/perl/bin/perl

perl -v
# Should show "MSWin32" in the build string, NOT "msys"
```

### macOS-specific

Xcode Command Line Tools are required:

```
xcode-select --install
```

### Linux-specific

Install system dependencies:

```
sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libssl-dev
```

## Getting started

```bash
# Clone and install
git clone https://github.com/your-org/cmdv3.git
cd cmdv3
npm install

# Run in development mode
npm run tauri dev

# Build for production
npm run tauri build
```

## Project structure

```
cmdv3/
├── src/                    # React frontend (TypeScript)
│   ├── components/         # UI components
│   └── App.tsx             # Root component
├── src-tauri/              # Rust backend
│   ├── src/
│   │   ├── clipboard/      # Clipboard monitoring
│   │   ├── commands/       # Tauri IPC commands
│   │   ├── crypto/         # Encryption, key derivation, BIP39
│   │   ├── db/             # SQLCipher database
│   │   ├── sensitive/      # Sensitive content detection
│   │   ├── storage/        # OS keychain integration
│   │   ├── sync/           # Cloud sync (Cloudflare R2)
│   │   └── lib.rs          # App setup, tray, window management
│   ├── capabilities/       # Tauri permissions
│   └── Cargo.toml          # Rust dependencies
├── package.json            # Node dependencies
└── tauri.conf.json         # Tauri app config
```

## Troubleshooting

### `openssl-sys` build failure on Windows

```
error: failed to run custom build command for `openssl-sys`
'perl' reported failure with exit code: 2
```

This means Git Bash's Perl is being used instead of Strawberry Perl. Fix your PATH so `C:\Strawberry\perl\bin` comes first (see [Windows-specific](#windows-specific) above).

### `Blocking waiting for file lock on artifact directory`

Another Cargo process is holding the build lock. Check Task Manager for stale `cargo.exe` or `rustc.exe` processes and end them.
