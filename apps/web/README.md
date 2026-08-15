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

The build fetches recent public GitHub releases. Until releases exist, it shows recent commits from `main`. TanStack Start prerenders the route into `dist/client/index.html`, and the result is also embedded in the client assets, so browsers do not call GitHub at runtime.

## Cloudflare

```sh
npm run preview:web
npm run deploy:web
```

The root `wrangler.jsonc` points Cloudflare at `apps/web/dist/client`. It intentionally has no `main` entry point: every request is served as a static asset, without an SSR Worker or Node server.

For a repository-connected Cloudflare Workers build, use:

- Root directory: `/`
- Build command: `npm run build:web`
- Deploy command: `npx wrangler deploy`

The desktop app uses `npm run build`. The website uses `npm run build:web` and produces its prerendered assets in `apps/web/dist/client`, which Cloudflare deploys directly. The root Dockerfile also runs `npm run build:web` and serves only this website for the existing Railway service until the custom domain is moved to Cloudflare.
