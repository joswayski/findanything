# Find Anything

Find Anything is a local-first desktop launcher that ranks applications, system actions, and filenames together. It combines exact and fuzzy text matching, a small on-device embedding model, and preferences learned from what you open.

The first vertical slice targets macOS while keeping discovery and launching behind a portable Rust boundary for Windows and Linux implementations.

## MVP behavior

- `brightness` surfaces **Displays**, even though the query is not its title.
- Installed application metadata is indexed, including background/menu-bar apps such as Numi.
- Choosing **Numi** for `calculator` teaches that exact query and boosts Numi on the next search.
- Filename matches are included without outranking strong app or system-action matches.
- Search, embeddings, and usage history stay on the device. If the embedding model is unavailable, lexical search and learned preferences continue working.
- <kbd>⌘/Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Space</kbd> toggles the launcher.

The semantic model is downloaded and cached by `fastembed` on first use. Production packaging will bundle the model so a fresh install is offline from its first launch.

## Development

Requirements: current Node.js/npm, Rust, and the platform prerequisites for Tauri 2.

```sh
npm install
npm run tauri dev
```

Checks:

```sh
npm run check
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

## Deliberately outside this MVP

Document contents, OCR, photo understanding, cross-device sync, third-party plugins, and cloud inference. Those can build on the same entity/index/ranker boundary after the launcher loop is fast and trustworthy.
