# Find Anything website

Static project site for [findanyth.ing](https://findanyth.ing), built with React and Vite.

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

The build fetches recent public GitHub releases. Until releases exist, it shows recent commits from `main`. The result is embedded in the static bundle, so browsers do not call GitHub at runtime.

## Docker

```sh
docker build -t findanything-web .
docker run --rm -p 8080:3000 findanything-web
```

The container serves the site on port `3000`. Railway's `RAILWAY_GIT_COMMIT_SHA` invalidates the GitHub-data build layer on every deployment.
