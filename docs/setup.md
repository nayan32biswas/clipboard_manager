# Setup and build

How to install the tools, run the app, and build installers on Windows, macOS and Linux.
Build each OS's installer on that OS. Cross-compiling is not supported. CI builds Linux and Windows installers (see [Release builds (CI)](#release-builds-ci)).

## What you need

| Tool | Version | Why |
|---|---|---|
| Rust (via `rustup`) | stable | Backend |
| Node.js | 24 or newer (CI uses 26) | UI build and the Tauri CLI |
| OS build tools | see below | C/C++ compiler, and the webview on Linux |

The Tauri CLI comes in with `npm ci`. You don't need a global install.

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

## Release builds (CI)

`.github/workflows/release.yml` builds the installers on GitHub and puts them on a **draft** GitHub Release:

| File | For |
|---|---|
| `.deb` | Ubuntu, Debian, Mint, Pop!_OS |
| `.rpm` | Fedora, RHEL, Rocky, Alma, openSUSE |
| `.AppImage` | Any distro, including Arch |
| `aur-clipboardmanager-bin-<version>.tar.gz` | AUR recipe (`PKGBUILD` + `.SRCINFO`) for Arch |
| `.msi`, `-setup.exe` | Windows 10/11 (unsigned, so SmartScreen warns on first run) |

The version comes from `package.json` (`tauri.conf.json` points to it). The version in `src-tauri/Cargo.toml` is unused;
in Rust, use `app.package_info().version`, not `CARGO_PKG_VERSION`.

To release:

1. Bump the version. Numbers only (`1.2.3`): Windows `.msi` rejects suffixes like `-beta`.
   ```sh
   npm version patch --no-git-tag-version   # or minor / major
   ```
2. Commit and push.
3. GitHub → **Actions** → **Release** → **Run workflow**.
4. When all jobs are green, open the draft under **Releases** and click **Publish**. Publishing creates the tag,
   so don't create `v*` tags by hand.

The run stops if the version is already released. Re-running the same version replaces its draft.

### AUR (Arch), once per release

One-time setup: create an account on [aur.archlinux.org](https://aur.archlinux.org) and add your SSH key there.

After the GitHub Release is **published** (the recipe downloads the `.deb` from the public release):

```sh
git clone ssh://aur@aur.archlinux.org/clipboardmanager-bin.git   # first time only; an empty repo is fine
cd clipboardmanager-bin
tar -xzf ~/Downloads/aur-clipboardmanager-bin-1.2.3.tar.gz       # PKGBUILD + .SRCINFO from the release
git add PKGBUILD .SRCINFO
git commit -m "Update to 1.2.3"
git push
```

Arch users then install with `yay -S clipboardmanager-bin`.

The template is `packaging/aur/PKGBUILD`. CI fills in `pkgver` and `sha256sums`; don't edit them by hand.

## Troubleshooting

| Problem | Fix |
|---|---|
| `cargo: command not found` right after installing | Open a new terminal. |
| `Port 1420 is already in use` | An old dev server is still running. Close it, or kill the `node` process. |
| Windows: `failed to run light.exe` | Turn on VBSCRIPT (see Windows step 5). |
| Linux: `webkit2gtk-4.1 was not found` | Install `libwebkit2gtk-4.1-dev`. It needs Ubuntu 22.04 or newer. |
| Linux: AppImage build fails while downloading | The AppImage bundler downloads `linuxdeploy` on first use, so it needs internet access. |
| Windows: "Windows protected your PC" | The installer is unsigned. Click **More info → Run anyway**. |
| AppImage: blank window | Start it with `WEBKIT_DISABLE_DMABUF_RENDERER=1`. |
| macOS: "app is damaged" or "unidentified developer" | Builds are unsigned. Run `xattr -dr com.apple.quarantine /Applications/ClipboardManager.app`. |
