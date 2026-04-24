FROM node:22-bookworm AS web-builder
ARG HTTP_PROXY
ARG HTTPS_PROXY
ARG NO_PROXY
ARG ALL_PROXY
ENV HTTP_PROXY=$HTTP_PROXY \
    HTTPS_PROXY=$HTTPS_PROXY \
    NO_PROXY=$NO_PROXY \
    ALL_PROXY=$ALL_PROXY \
    http_proxy=$HTTP_PROXY \
    https_proxy=$HTTPS_PROXY \
    no_proxy=$NO_PROXY \
    all_proxy=$ALL_PROXY
WORKDIR /app

COPY apps/mova-web/package.json ./apps/mova-web/package.json
COPY apps/mova-web/pnpm-lock.yaml ./apps/mova-web/pnpm-lock.yaml

RUN corepack enable && pnpm --dir apps/mova-web install --frozen-lockfile

COPY apps/mova-web ./apps/mova-web

RUN pnpm --dir apps/mova-web build

FROM rust:1-bookworm AS builder
ARG HTTP_PROXY
ARG HTTPS_PROXY
ARG NO_PROXY
ARG ALL_PROXY
ENV HTTP_PROXY=$HTTP_PROXY \
    HTTPS_PROXY=$HTTPS_PROXY \
    NO_PROXY=$NO_PROXY \
    ALL_PROXY=$ALL_PROXY \
    http_proxy=$HTTP_PROXY \
    https_proxy=$HTTPS_PROXY \
    no_proxy=$NO_PROXY \
    all_proxy=$ALL_PROXY
WORKDIR /app

COPY Cargo.toml ./
COPY Cargo.lock ./
COPY apps/mova-server ./apps/mova-server
COPY crates ./crates
COPY migrations ./migrations

RUN cargo build --locked --release -p mova-server

FROM debian:bookworm-slim AS runtime
ARG HTTP_PROXY
ARG HTTPS_PROXY
ARG NO_PROXY
ARG ALL_PROXY
ENV HTTP_PROXY=$HTTP_PROXY \
    HTTPS_PROXY=$HTTPS_PROXY \
    NO_PROXY=$NO_PROXY \
    ALL_PROXY=$ALL_PROXY \
    http_proxy=$HTTP_PROXY \
    https_proxy=$HTTPS_PROXY \
    no_proxy=$NO_PROXY \
    all_proxy=$ALL_PROXY
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates ffmpeg python3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/mova-server /usr/local/bin/mova-server
COPY --from=web-builder /app/apps/mova-web/dist /app/web
COPY scripts /app/scripts

EXPOSE 36080

CMD ["mova-server"]
