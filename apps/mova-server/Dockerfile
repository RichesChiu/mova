# syntax=docker/dockerfile:1.7

FROM richeschiu/mova-web-build-base:node24-pnpm11 AS web-builder
ARG TARGETARCH
ARG TARGETOS
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
    all_proxy=$ALL_PROXY \
    npm_config_fetch_retries=10 \
    npm_config_fetch_retry_mintimeout=10000 \
    npm_config_fetch_retry_maxtimeout=120000 \
    npm_config_network_concurrency=1
WORKDIR /app

COPY apps/mova-web/package.json ./apps/mova-web/package.json
COPY apps/mova-web/pnpm-lock.yaml ./apps/mova-web/pnpm-lock.yaml
COPY apps/mova-web/pnpm-workspace.yaml ./apps/mova-web/pnpm-workspace.yaml

RUN --mount=type=cache,id=mova-pnpm-store-${TARGETOS}-${TARGETARCH},target=/root/.local/share/pnpm/store,sharing=locked \
    pnpm --dir apps/mova-web install --frozen-lockfile --fetch-retries=10 --fetch-retry-mintimeout=10000 --fetch-retry-maxtimeout=120000 --network-concurrency=1

COPY apps/mova-web ./apps/mova-web

RUN pnpm --dir apps/mova-web build

FROM richeschiu/mova-rust-build-base:1-bookworm AS builder
ARG TARGETARCH
ARG TARGETOS
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
    all_proxy=$ALL_PROXY \
    CARGO_HTTP_LOW_SPEED_LIMIT=1 \
    CARGO_HTTP_LOW_SPEED_TIME=600 \
    CARGO_HTTP_TIMEOUT=600 \
    CARGO_NET_RETRY=5
WORKDIR /app

COPY Cargo.toml ./
COPY Cargo.lock ./
COPY apps/mova-server ./apps/mova-server
COPY crates ./crates
COPY migrations ./migrations

RUN --mount=type=cache,id=mova-cargo-registry-${TARGETOS}-${TARGETARCH},target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=mova-cargo-git-${TARGETOS}-${TARGETARCH},target=/usr/local/cargo/git,sharing=locked \
    cargo build --locked --release -p mova-server

FROM richeschiu/mova-runtime-base:bookworm-ffmpeg-python3 AS runtime
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

COPY --from=builder /app/target/release/mova-server /usr/local/bin/mova-server
COPY --from=web-builder /app/apps/mova-web/dist /app/web
COPY scripts /app/scripts

EXPOSE 36080

CMD ["mova-server"]
