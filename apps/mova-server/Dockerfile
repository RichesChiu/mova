FROM node:22-bookworm AS web-builder
WORKDIR /app

COPY apps/mova-web/package.json ./apps/mova-web/package.json
COPY apps/mova-web/pnpm-lock.yaml ./apps/mova-web/pnpm-lock.yaml

RUN corepack enable && pnpm --dir apps/mova-web install --frozen-lockfile

COPY apps/mova-web ./apps/mova-web

RUN pnpm --dir apps/mova-web build

FROM rust:1-bookworm AS builder
WORKDIR /app

COPY Cargo.toml ./
COPY Cargo.lock ./
COPY apps ./apps
COPY crates ./crates
COPY migrations ./migrations

RUN cargo build --locked --release -p mova-server

FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates ffmpeg \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/mova-server /usr/local/bin/mova-server
COPY --from=web-builder /app/apps/mova-web/dist /app/web

EXPOSE 36080

CMD ["mova-server"]
