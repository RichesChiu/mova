# syntax=docker/dockerfile:1.7

FROM node:22-bookworm AS web-builder
WORKDIR /app

COPY apps/mova-web/package.json ./apps/mova-web/package.json
COPY apps/mova-web/pnpm-lock.yaml ./apps/mova-web/pnpm-lock.yaml

RUN corepack enable

RUN --mount=type=cache,target=/root/.local/share/pnpm/store \
    pnpm --dir apps/mova-web install --frozen-lockfile

COPY apps/mova-web ./apps/mova-web

RUN --mount=type=cache,target=/root/.local/share/pnpm/store \
    pnpm --dir apps/mova-web build

FROM rust:1-bookworm AS builder
WORKDIR /app

COPY Cargo.toml ./
COPY Cargo.lock ./
COPY apps/mova-server ./apps/mova-server
COPY crates ./crates
COPY migrations ./migrations

RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/app/target,sharing=locked \
    cargo build --locked --release -p mova-server

FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt/lists,sharing=locked \
    apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates ffmpeg

COPY --from=builder /app/target/release/mova-server /usr/local/bin/mova-server
COPY --from=web-builder /app/apps/mova-web/dist /app/web

EXPOSE 36080

CMD ["mova-server"]
