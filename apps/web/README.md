# Find Anything website

Static project site for [findanyth.ing](https://findanyth.ing), built with TanStack Start and deployed through Cloudflare Workers Static Assets.

## Development

From the repository root:

```sh
npm run dev:web
```

The site runs at [http://localhost:5174](http://localhost:5174).

## Build

```sh
npm run build:web
```

This command runs Oxlint correctness checks and Oxfmt's formatting check for the
website and native JavaScript launcher before building. Run `npm run format` to
apply formatting; generated routes, native/generated assets and vendored files
are not reformatted. Native CI and Cloudflare's existing build command use these
same gates. There are no JavaScript test suites or unused JS test runner.

Vite 8.3.3 and React plugin 6.1.2 use Rolldown/Oxc. The explicit browser target
retains Vite 7's Chrome/Edge 107, Firefox 104 and Safari 16 floor. TypeScript
7.0.2 is the stable native Go compiler, exposed through the usual `tsc` command;
`npm run typecheck --workspace @findanything/web` checks both source and build
configuration. Node 24+ and npm remain the runtime and package manager.

The build fetches recent public GitHub releases. Until releases exist, it shows recent commits from `main`. TanStack Start prerenders the route into `dist/client/index.html`, and the result is also embedded in the client assets, so browsers do not call GitHub at runtime.

## Cloudflare

```sh
npm run preview:web
npm run deploy:web
```

The root `wrangler.jsonc` points Cloudflare at `apps/web/dist/client`. It intentionally has no `main` entry point: every request is served as a static asset, without an SSR Worker or Node server.

For a repository-connected Cloudflare Workers build, use:

- Root directory: `/`
- Build command: `npm run build:web` (production and preview)
- Deploy command: `npx wrangler deploy`
- Non-production deploy command: `npx wrangler versions upload`

`npm run build` is the desktop UI. Preview builds must not use it: that writes `dist/` at the repo root, and Wrangler then fails looking for `apps/web/dist/client`.

The website uses `npm run build:web` and produces its prerendered assets in `apps/web/dist/client`, which Cloudflare deploys directly.
