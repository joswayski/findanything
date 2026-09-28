# Project guidance

- Treat desktop UI and lifecycle behavior as cross-platform. For every UI change, explicitly consider macOS, Windows, and Linux, keep shared behavior consistent, and isolate platform-specific APIs behind target-specific code.
- When fixing a UI bug reported on one desktop platform, check whether the same interaction can fail on the other two and validate their code paths where practical before publishing.
- Desktop UI follows `apps/native/design/README.md`. Change shared `tokens.json` and regenerate with `python apps/native/design/generate.py`; do not fork palettes or layout scales per platform. Review equivalent native fixture states across all clients when adding components.
