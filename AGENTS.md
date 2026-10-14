# Project guidance

- Treat desktop UI and lifecycle behavior as cross-platform. For every UI change, explicitly consider macOS, Windows, and Linux, keep shared behavior consistent, and isolate platform-specific APIs behind target-specific code.
- When fixing a UI bug reported on one desktop platform, check whether the same interaction can fail on the other two and validate their code paths where practical before publishing.
- Desktop UI follows `apps/native/design/README.md`. Change shared `tokens.json` and regenerate with `uv run --no-project python apps/native/design/generate.py`; do not fork palettes or layout scales per platform. Review equivalent native fixture states across all clients when adding components.

## Shipping

`main` is production: pushes deploy the website automatically. Speed beats ceremony: there are no real users yet, so ship small, watch it land, and fix forward.

- Push a branch and open a draft pull request instead of pushing to `main` when the work is unfinished and you have to stop (session ending, disk full, missing toolchain; say what is done and what is left), or when it needs a decision the owner hasn't made.
- Otherwise commit and push straight to `main`; no pull requests. Run the local checks for what you changed first (`DEVELOPMENT.md` → Validation), then `git pull --rebase origin main` and push. Never force-push `main` or rewrite other agents' commits.
- Never put literal CI-skip directives in commit messages, even to explain them.
- Pushes to `main` run `Native CI` (about 5 minutes; up to ~15 with cold caches) and Cloudflare `Workers Builds: findanything`. Don't start `Native CI` manually for routine changes. Find the runs for your pushed `SHA` with `gh run list --repo joswayski/findanything --commit "$SHA"` and watch them with `gh run watch <id> --exit-status`.
- Deployment is automatic; there is no `joswayski/infrastructure` deploy workflow for this repository. After pushing, confirm what changed shipped and report it:
  1. Website (`apps/web`): Cloudflare Workers Builds deploys `main` on push; check `gh api repos/joswayski/findanything/commits/$SHA/check-runs --jq '.check_runs[] | select(.app.slug == "cloudflare-workers-and-pages") | .name + " " + .conclusion'`. `npm run deploy:web` is the manual fallback.
  2. Native clients: pushes that change `apps/native`, `crates` or Cargo files run `Native Preview release`, which publishes a Preview that installed apps update to (signed per platform once signing is configured; see `DEVELOPMENT.md` → Publishing). Don't change signing secrets without the owner.
- If `Native CI`, Workers Builds or a release fails, fix it forward with another commit.
- Still ask the owner first for anything irreversible: deleting releases, data or infrastructure, or rotating secrets.
