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

`wrangler.jsonc` points Cloudflare at `dist/client`. It intentionally has no `main` entry point: every request is served as a static asset, without an SSR Worker or Node server.

For a repository-connected Cloudflare Workers build, keep the repository root as the build root and use:

- Build command: `npm run build:web`
- Deploy command: `npm run deploy --workspace @findanything/web`
