# Build from the repository root:
#   docker build -t findanything-web .
#   docker run --rm -p 8080:3000 findanything-web

FROM node:24-alpine AS build

WORKDIR /app

COPY package.json package-lock.json ./
COPY apps/web/package.json apps/web/

RUN npm ci

COPY apps/web apps/web

# Railway supplies a new commit SHA for each GitHub deployment. Referencing it
# here refreshes the build-time GitHub release/activity data on every deploy.
ARG RAILWAY_GIT_COMMIT_SHA=local
RUN RAILWAY_GIT_COMMIT_SHA="$RAILWAY_GIT_COMMIT_SHA" npm run build:web

FROM node:24-alpine

WORKDIR /app

ENV NODE_ENV=production
ENV PORT=3000

COPY apps/web/package.json ./
RUN npm install --omit=dev

COPY --from=build /app/apps/web/dist ./dist

EXPOSE 3000

CMD ["npm", "start"]
