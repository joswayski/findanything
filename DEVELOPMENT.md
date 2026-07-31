# Development

Find Anything contains a Tauri desktop launcher at the repository root and a static project website in `apps/web`.

## Requirements

- Node.js 24 or newer
- Rust 1.88 or newer
- The [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform

## Desktop app

```sh
npm install
npm run tauri dev
```

The semantic model is downloaded into the user's cache on first development launch. Keyword matching and learned preferences continue working while it downloads or if it is unavailable. Production installers should bundle the model so a fresh installation is offline from first launch.

## Website

```sh
npm run dev:web
```

The website runs at [http://localhost:5174](http://localhost:5174). Its production build fetches recent public releases from GitHub, falling back to recent `main` changes until releases exist, and embeds that data into the static bundle.

## Validation

```sh
npm run check
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

## Website container

```sh
docker build -t findanything-web .
docker run --rm -p 8080:3000 findanything-web
```

The container serves the static site on port `3000`. Railway's `RAILWAY_GIT_COMMIT_SHA` build argument refreshes the GitHub activity data on each deployment.
