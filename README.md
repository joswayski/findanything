# Find Anything

Find Anything is a local-first desktop launcher for finding applications, system settings, and files.

[findanyth.ing](https://findanyth.ing)

> [!WARNING]
> Find Anything is experimental. The browser-free native migration is under development; signed installers and installed-update acceptance are not yet released.

The macOS app uses AppKit, including its native editor and results table. Windows
and Linux share a Rust `egui`/`wgpu` frontend, model worker, and a small platform
adapter; their existing package crates remain thin wrappers so the cargo commands,
`findanything` executable name, and Velopack identities do not change. Search,
ranking, local learning, and updates share the Rust core. The desktop app has no
Tauri, WebView, JavaScript runtime, or local web server. The website remains
separate.

## Features

- Search macOS applications (including menu-bar/background apps), Windows Start Menu shortcuts, and Linux XDG applications.
- Open system settings with natural queries such as `brightness` or `dark mode`.
- Learn which result you prefer for a query from what you open.
- Combine exact, typo-tolerant, and on-device semantic matching.
- Search personal filenames without prioritizing system and build artifacts.
- Keep search and usage history on your computer.

Use **⌘⇧Space** on macOS or **Ctrl+Shift+Space** on Windows/X11. On Wayland, configure a desktop shortcut to launch the executable; Escape minimizes rather than making the app unreachable. macOS has a menu-bar entry; Windows/Linux also retain a normal taskbar window.

Native packages are wired to check/download updates automatically and install them on the next launch, with a **Restart to update** action. Publishing is gated on signing setup and platform acceptance. Existing Tauri installations have no updater and need a **one-time native installation**; the local learned-preference database keeps the same location and schema. Model downloads and update requests require network access; searches and usage history are not uploaded.

Current limits: Windows Store-only apps without Start Menu shortcuts are not indexed. Windows/Linux filename search covers standard personal folders, not the whole disk. The shared frontend requires wgpu graphics (GPU-backed or a supported software implementation). Native validation is still in progress; physical screen-reader, IME, mixed-DPI, Windows, and Wayland acceptance remains open. See [development and release notes](DEVELOPMENT.md).

## Roadmap

- Complete native platform acceptance and enable signed Preview releases.
- Search inside documents with local embeddings and OCR.
- Search photos by their contents.
- Optional private sync across computers.

## Development

See [DEVELOPMENT.md](DEVELOPMENT.md) for local setup, validation, and website deployment.

## License

Licensed under the [Apache License 2.0](LICENSE).
