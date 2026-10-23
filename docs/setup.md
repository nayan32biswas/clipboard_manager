# Setup and build

How to install the tools, run the app, and build installers on Windows, macOS and Linux.
Build each OS's installer on that OS. Cross-compiling is not supported. CI builds all three (phase 8).

## What you need

| Tool | Version | Why |
|---|---|---|
| Rust (via `rustup`) | stable | Backend |
| Node.js | 20 or newer (LTS recommended) | UI build and the Tauri CLI |
| OS build tools | see below | C/C++ compiler, and the webview on Linux |

The Tauri CLI comes in with `npm install`. You don't need a global install.

## Windows 10/11

1. **C++ build tools.** Install [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) and select the **Desktop development with C++** workload.
2. **WebView2.** Windows 11 and Windows 10 (1803+) already have it. Otherwise, install the Evergreen Bootstrapper from [Microsoft](https://developer.microsoft.com/microsoft-edge/webview2/).
3. **Rust:**
   ```powershell
   winget install --id Rustlang.Rustup
   rustup default stable-msvc
   ```
4. **Node.js:**
   ```powershell
   winget install OpenJS.NodeJS.LTS
   ```
5. **For `.msi` installers only:** turn on **VBSCRIPT** under Settings → System (or Apps on older builds) → Optional features → More Windows features.

Open a new terminal after installing, so `cargo` and `node` are on `PATH`.

## macOS

1. **Xcode Command Line Tools:**
   ```sh
   xcode-select --install
   ```
2. **Rust:**
   ```sh
   curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh
   ```
3. **Node.js:** use `brew install node`, or [nvm](https://github.com/nvm-sh/nvm) with `nvm install --lts`.
4. **Optional, for a universal build (Intel + Apple Silicon):**
   ```sh
   rustup target add aarch64-apple-darwin x86_64-apple-darwin
   ```

## Linux

The main target is Ubuntu 24.04 or newer, which ships GNOME 46+. The GNOME extension needs GNOME 45+.

1. **System packages (Ubuntu/Debian):**
   ```sh
   sudo apt update
   sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
   ```
   Fedora:
   ```sh
   sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file libappindicator-gtk3-devel librsvg2-devel libxdo-devel
   sudo dnf group install "c-development"
   ```
   Arch:
   ```sh
   sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl appmenu-gtk-module libappindicator-gtk3 librsvg xdotool
   ```
2. **Rust:**
   ```sh
   curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh
   ```
3. **Node.js:** use [nvm](https://github.com/nvm-sh/nvm) with `nvm install --lts`. Ubuntu's `apt` version is too old.

## Run and build (all OSes)

```sh
npm ci                  # install JS dependencies from the lockfile
npm run tauri dev       # run the app with live reload
npm test                # UI tests
cargo test --manifest-path src-tauri/Cargo.toml   # Rust tests
npm run tauri build     # release build + installers
```

The first Rust build takes a few minutes. Later builds are incremental.

`npm run tauri build` writes installers to `src-tauri/target/release/bundle/`:

| OS | Output |
|---|---|
| Windows | `msi/*.msi`, `nsis/*-setup.exe` |
| macOS | `macos/*.app`, `dmg/*.dmg` |
| Linux | `deb/*.deb`, `rpm/*.rpm`, `appimage/*.AppImage` |

For a macOS universal build, run `npm run tauri build -- --target universal-apple-darwin`.

## GNOME extension (from phase 6)

This is required on GNOME Wayland. Without it, the app can't see copies there.

```sh
gnome-extensions pack gnome-extension/clipboardmanager@nayan.dev --force
gnome-extensions install --force clipboardmanager@nayan.dev.shell-extension.zip
```

Log out and back in. Wayland can't reload GNOME Shell without a new session. Then enable the extension:

```sh
gnome-extensions enable clipboardmanager@nayan.dev
```

The `.deb` package installs the extension for you. You still have to enable it once.

To see the extension's logs, run `journalctl -f -o cat /usr/bin/gnome-shell`.

## Troubleshooting

| Problem | Fix |
|---|---|
| `cargo: command not found` right after installing | Open a new terminal. |
| `Port 1420 is already in use` | An old dev server is still running. Close it, or kill the `node` process. |
| Windows: `failed to run light.exe` | Turn on VBSCRIPT (see Windows step 5). |
| Linux: `webkit2gtk-4.1 was not found` | Install `libwebkit2gtk-4.1-dev`. It needs Ubuntu 22.04 or newer. |
| Linux: AppImage build fails while downloading | The AppImage bundler downloads `linuxdeploy` on first use, so it needs internet access. |
| macOS: "app is damaged" or "unidentified developer" | v1 builds are unsigned. Run `xattr -dr com.apple.quarantine /Applications/ClipboardManager.app`. |
