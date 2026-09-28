# Development

Find Anything has a shared Rust core in `crates/findanything-core`, an AppKit client in `apps/native/macos`, a shared `egui`/`wgpu` Windows/Linux frontend and model worker in `apps/native/shared`, thin package wrappers in `apps/native/windows` and `apps/native/linux`, and a static project website in `apps/web`. Platform adapters isolate shortcut, lifecycle, accessibility-theme, and launch integration. The small C ABI in `crates/findanything-ffi` is only for AppKit. There is no desktop WebView or JavaScript frontend.

## Requirements

- Current stable Rust, `rustfmt`, and `clippy`. The shared UI requires Rust 1.95
  or newer; other workspace crates retain their existing declared minimum.
- macOS 13+ and Xcode command-line tools for AppKit/SwiftPM.
- Windows 10/11 x64 with Visual Studio C++ build tools and Windows SDK.
- Linux x64 with X11 or Wayland development libraries, xkbcommon, OpenSSL 3,
  and GIO (`gio`). There is no GTK UI dependency. The release build targets
  Ubuntu 22.04 or compatible newer distributions, not every Linux distribution.
- Node.js 24+ for the website and optional `npm` convenience commands; the desktop itself does not need Node.

Debian/Ubuntu prerequisites: `build-essential pkg-config libssl-dev libx11-dev libxkbcommon-dev libwayland-dev libglib2.0-bin`. Native smoke checks also use `xvfb xauth xdotool x11-utils openbox imagemagick`. `.agents/setup` installs these for orbs; no GTK or WebKit packages are required. AppImage packaging additionally requires `squashfs-tools` and `libfuse2`.

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

macOS keeps AppKit's native editor and table. Windows and Linux use the shared
immediate-mode `egui` frontend rendered by `wgpu`, with a shared Model worker and
target-specific platform adapter. `eframe` is pinned to the same Captures revision,
`60d7caaea38a795618e842925061ad2210028a2a`, for its hidden-window logic wake
behavior. A working wgpu GPU backend or supported software renderer is required.
GIO remains a Linux launch integration, not a UI toolkit dependency. Global
shortcuts are available on macOS/Windows/X11; Wayland requires a
compositor-configured shortcut launching the executable. Re-running the executable
forwards focus to the existing instance. Escape/Close hide only with a registered
shortcut; otherwise Escape minimizes and Close quits (macOS retains its menu bar).
There is no auto-start-at-login registration yet.

The desktop shells share the [Graphite design contract](apps/native/design/README.md).
Change `apps/native/design/tokens.json`, run `python apps/native/design/generate.py`,
and inspect matching native fixture states on all platforms. Generated token
drift is checked in CI. Static Inter 4.1 Regular and SemiBold fonts are bundled
on every platform; native frames and font rasterization remain platform-owned.

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

`Native CI` builds/tests on macOS ARM64, macOS Intel, Windows x64 and Linux x64. The shared frontend enables AccessKit, but this work remains in progress and has not received final native verification. CI is not a substitute for physical input/IME, screen-reader, mixed-DPI or Wayland acceptance; those acceptance checks remain open. The Linux orb cannot compile AppKit or execute Windows binaries.

Successful CI also uploads `unsigned-test-packages-<runner>` artifacts for seven days. These are native test installers, not signed public releases: use disposable profiles, and expect operating-system trust warnings. CI packages locally without installing or publishing a release. All desktop clients use dark Graphite surfaces. Windows retains its high-contrast startup palette; it no longer uses native Edit/ListBox controls.

Native fixtures never initialize databases, updater hooks, model downloads, single-instance election or global shortcuts:

```sh
cargo build -p findanything-linux --no-default-features
python3 apps/native/smoke.py --binary target/debug/findanything --output /tmp/findanything-smoke
# Deterministic shared fixture and renderer capture (also writes /tmp/linux.json):
cargo run -p findanything-linux --no-default-features -- --fixture query --screenshot /tmp/linux.png
# Shared Windows fixture/input smoke (PowerShell):
cargo build -p findanything-windows
./apps/native/windows/smoke.ps1
# macOS:
apps/native/macos/.build/release/FindAnythingNative --fixture results --appearance dark --screenshot /tmp/native-mac.png
```

The fixture interface is `--fixture results|selected|query|minimum|empty|error`.
`--screenshot path.png` captures the renderer and writes JSON state beside it.
`--probe path` is fixture-only and is intended for tests that provide actual input,
not as a substitute for input. Review captures and state, not just process exit.
The existing Linux `smoke.py` uses private Xvfb/software rendering and real
keyboard input; the rewritten Windows `smoke.ps1` exercises the same shared UI.
Neither proves physical GPU/desktop acceptance.

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
