FROM node:24-alpine AS build

WORKDIR /app

COPY package.json package-lock.json ./
COPY apps/web/package.json apps/web/

RUN npm ci

COPY apps/web apps/web

RUN npm run build:web

FROM nginx:1.29-alpine

ENV PORT=3000

COPY nginx.conf /etc/nginx/templates/default.conf.template
COPY --from=build /app/apps/web/dist/client /usr/share/nginx/html

EXPOSE 3000
