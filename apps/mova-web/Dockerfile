FROM node:22-bookworm AS builder
WORKDIR /app

COPY apps/mova-web/package.json ./
COPY apps/mova-web/pnpm-lock.yaml ./

RUN corepack enable && pnpm install --frozen-lockfile

COPY apps/mova-web/ ./

RUN pnpm build

FROM nginx:1.27-alpine AS runtime
WORKDIR /usr/share/nginx/html

COPY apps/mova-web/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=builder /app/dist ./

EXPOSE 80
