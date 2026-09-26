# Development

Find Anything has a shared Rust core in `crates/findanything-core`, an AppKit shell in `apps/native/macos`, a Rust/Win32 shell in `apps/native/windows`, a Rust/GTK4 shell in `apps/native/linux`, and a static project website in `apps/web`. The small C ABI in `crates/findanything-ffi` is only for AppKit. No desktop web frontend or custom-drawn UI framework remains.

## Requirements

- Current stable Rust (minimum 1.92), `rustfmt`, and `clippy`.
- macOS 13+ and Xcode command-line tools for AppKit/SwiftPM.
- Windows 10/11 x64 with Visual Studio C++ build tools and Windows SDK.
- Linux x64 with X11 or Wayland, GTK 4.6+, OpenSSL 3, and GIO (`gio`). The release build targets Ubuntu 22.04 or compatible newer distributions, not every Linux distribution.
- Node.js 24+ for the website and optional `npm` convenience commands; the desktop itself does not need Node.

Debian/Ubuntu prerequisites: `build-essential pkg-config libssl-dev libgtk-4-dev libx11-dev libglib2.0-bin`. Native smoke checks also use `xvfb xauth xdotool x11-utils openbox imagemagick`. `.agents/setup` installs these for orbs; no WebKit packages are required. AppImage packaging additionally requires `squashfs-tools` and `libfuse2`.

## Desktop app

```sh
npm install
npm run dev
# Build only; never installs, registers login items, or publishes:
npm run build
```

Without npm, run `cargo run -p findanything-windows` on Windows, `cargo run -p findanything-linux` on Linux, or `bash apps/native/macos/build.sh` then `apps/native/macos/.build/release/FindAnythingNative` on macOS. The latter always rebuilds the Rust static library; `FINDANYTHING_LIB_DIR` opts into a prebuilt library explicitly. Build the matching shell, not all workspace binaries: the platform crates intentionally share the installed executable name.

The semantic model downloads into the existing `Find Anything/models` cache on first launch. Keyword matching and learned preferences keep working while it downloads or when it is unavailable. Current native packages do not bundle the model. Use `--no-default-features` for keyword-only development/tests without ONNX or model downloads, not as the release build.

`fastembed` is pinned to 5.8.0 with ONNX Runtime 1.22 to retain the Ubuntu 22.04 runtime baseline. Newer prebuilt ONNX archives require newer system libraries; validate the oldest supported OS before upgrading this dependency.

Search/activation run on serial workers. Generation checks discard superseded responses and prevent opening an old result for a new query. The SQLite data location remains `dirs::data_local_dir()/Find Anything/findanything.sqlite3`, with no schema migration. Quit the old Tauri app before running native: its separate single-instance mechanism cannot hand off to this one. Back up that directory before testing installation/rollback against real data.

macOS uses the existing application metadata and Spotlight paths. Windows indexes `.lnk` shortcuts from user/common Start Menu roots (not Store-only AppsFolder applications). Linux follows XDG desktop-file precedence and visibility and asks GIO to launch files; it does not execute desktop-file command strings through a shell. Windows/Linux filename search caches standard known personal folders for 60 seconds, stops at depth 6 / 20,000 entries / 300 ms between filesystem operations, and skips hidden/build folders and symlinks. These are intentional initial limits, not a whole-disk index. App discovery refreshes at process launch.

All platforms use native text editing, selection, lists and menus: AppKit, Win32 and GTK4. Captures informed the shared Rust/AppKit split; Find Anything goes further by using native Windows/Linux widgets instead of its experimental wgpu direction. Global shortcuts are available on macOS/Windows/X11; Wayland requires a compositor-configured shortcut launching the executable. Re-running the executable forwards focus to the existing instance. Escape/Close hide only with a registered shortcut; otherwise Escape minimizes and Close quits (macOS retains its menu bar). There is no auto-start-at-login registration yet.

## Website

```sh
npm run dev:web
```

The website runs at [http://localhost:5174](http://localhost:5174). It uses TanStack Start, but every route is prerendered at build time. The production build fetches recent public releases from GitHub, falling back to recent `main` changes until releases exist, and embeds that data into the generated HTML and client assets.

## Validation

```sh
npm run check
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 -m unittest discover -s apps/native -p 'test_*.py'
```

`Native CI` builds/tests on macOS ARM64, macOS Intel, Windows x64 and Linux x64. It is not a substitute for physical input/IME, screen-reader, mixed-DPI or Wayland acceptance. The Linux orb cannot compile AppKit or execute Windows binaries.

Successful CI also uploads `unsigned-test-packages-<runner>` artifacts for seven days. These are native test installers, not signed public releases: use disposable profiles, and expect operating-system trust warnings. CI packages locally without installing or publishing a release. Windows uses standard system-colored Win32 controls; custom dark-mode styling is not part of this migration.

Native fixtures never initialize databases, updater hooks, model downloads, single-instance election or global shortcuts:

```sh
cargo build -p findanything-linux --no-default-features
python3 apps/native/smoke.py --binary target/debug/findanything --output /tmp/findanything-smoke
# Interactive GTK fixtures:
cargo run -p findanything-linux --no-default-features -- --fixture empty --theme light
# Win32 fixtures and native-control assertions (PowerShell):
cargo build -p findanything-windows
./apps/native/windows/smoke.ps1
# macOS:
apps/native/macos/.build/release/FindAnythingNative --fixture results --appearance dark --screenshot /tmp/native-mac.png
```

Fixture states are results (omit the state on Windows/Linux), `empty`, and `error`. Review the captured images, not just the process exit code. The Linux smoke uses private Xvfb/software rendering and actual keyboard input at normal/minimum size. It does not prove physical GPU/desktop acceptance.

## Native packages and automatic updates

The shared updater uses pinned **Velopack 1.2.158**, not Tauri's updater. Installed builds check GitHub on launch and every six hours, download a newer package, and apply it at the next process launch. They do not interrupt active searches to restart. The native menu exposes Check for updates and Restart to update. Bare `cargo`/SwiftPM builds have updates disabled. Model caches and SQLite data live outside the package replacement directory.

Install .NET SDK 8 and `dotnet tool install --global vpk --version 1.2.158` on the target OS. After a release build, stage an **unsigned local test package** with:

```sh
python apps/native/package.py --version 0.2.1 --stage /path/to/new-stage --output /path/to/releases
```

The script refuses an existing staging directory. It does not install or publish. It produces macOS `.pkg`/portable bundles, Windows setup/portable packages, and a Linux AppImage. Preserve the original AppImage file and its `APPIMAGE` environment when launching it; running an extracted bare executable is not an installed update test. Linux packages still depend on compatible system graphics/GIO/OpenSSL libraries and require clean-machine validation. Installing to a user-writable location avoids elevation during replacement.

Each target has a distinct package ID and channel: `preview-osx-arm64`, `preview-osx-x64`, `preview-win-x64`, `preview-linux-x64`. Never combine architectures in one feed. All `releases.<channel>.json` files and their referenced `.nupkg` assets are published together into one GitHub prerelease. The SDK considers the ten newest GitHub releases; do not publish unrelated releases that displace all native feeds.

**Trust boundary:** HTTPS/GitHub repository release permissions protect the feed. Velopack verifies package size/hash, but those hashes are not independent publisher signatures. macOS packages must be Developer ID signed/notarized; Windows uses Authenticode. Linux Velopack packages have no independent publisher signature. Protect release permissions and use the `native-release` GitHub environment; a signed-feed trust root would be a separate hardening project.

### Enablement gate (not enabled by this migration)

1. Configure the `native-release` GitHub environment, restrict it to trusted `main`, and set the signing secrets through GitHub's secret UI (never commit values): `APPLE_CERTIFICATES_BASE64` (P12 with Developer ID Application **and Installer** identities), `APPLE_CERTIFICATES_PASSWORD`, `APPLE_API_KEY_BASE64` (App Store Connect P8), `APPLE_API_KEY_ID`, `APPLE_API_ISSUER`, `WINDOWS_CERTIFICATE_BASE64`, `WINDOWS_CERTIFICATE_PASSWORD`. Hardware-backed Windows identities may require replacing the PFX import with your signing provider.
2. Set nonsecret environment variables `APPLE_APP_IDENTITY` and `APPLE_INSTALLER_IDENTITY` to the exact certificate names. `package.py --signed` uses those identities/notary profile on macOS and `WINDOWS_CERT_THUMBPRINT` from the imported Windows certificate. Never distribute the unsigned local test artifacts as trusted releases.
3. On disposable macOS/Windows/X11/Wayland profiles, verify discovery, launch failures, keyboard/IME, accessibility, light/dark, hide/reopen and mixed-DPI behavior. Install version A then deliver version B using a disposable feed/package identity: verify automatic download, no forced mid-search restart, next-launch and explicit-restart application, offline retry, corrupt/missing package rejection, unchanged learned preferences, and recovery by reinstalling a previous package. Neither fixture tests nor compilation close this gate.
4. Only after acceptance, set the **repository variable** `NATIVE_RELEASES_ENABLED=true`. The release workflow then reacts to successful `Native CI` runs on `main`, signs/packages all four targets and publishes one Preview (`0.2.<CI run number>`). It creates a draft and uploads every asset before exposing any feed. There is no stable release channel. An interrupted upload leaves a draft; inspect/delete that draft before rerunning rather than uploading a partial public feed.

GitHub signing secrets and release-environment access could not be inspected from the development orb. No credentials, repository variables, release environments, releases, or installed applications are changed by the source migration itself. Existing Tauri installations never shipped an updater: users must quit that app and install a native package once. Future native versions use the updater above.

## Website preview and deployment

```sh
npm run preview:web
npm run deploy:web
```

The preview command builds the site and serves it with Wrangler's local Cloudflare runtime. The deploy command rebuilds the site and uploads only `apps/web/dist/client` through Cloudflare Workers Static Assets. There is no request-time Worker or Node server.
