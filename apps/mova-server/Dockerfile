# syntax=docker/dockerfile:1.7

ARG MOVA_WEB_BUILD_BASE=richeschiu/mova-web-build-base:node24-pnpm11
ARG MOVA_RUST_BUILD_BASE=richeschiu/mova-rust-build-base:1-bookworm
ARG MOVA_RUNTIME_BASE=richeschiu/mova-runtime-base:trixie-ffmpeg7-python3

FROM ${MOVA_WEB_BUILD_BASE} AS web-builder
ARG TARGETARCH
ARG TARGETOS
ENV npm_config_fetch_retries=10 \
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

FROM ${MOVA_RUST_BUILD_BASE} AS builder
ARG TARGETARCH
ARG TARGETOS
ARG MOVA_BUILD_VERSION=development
ENV MOVA_BUILD_VERSION=$MOVA_BUILD_VERSION \
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

FROM ${MOVA_RUNTIME_BASE} AS runtime
ARG MOVA_BUILD_VERSION=development
ARG MOVA_BUILD_REVISION=unknown
ENV MOVA_WEB_DIST_DIR=/app/web \
    MOVA_CACHE_DIR=/app/data/cache \
    MOVA_BUILD_VERSION=$MOVA_BUILD_VERSION
LABEL org.opencontainers.image.title="Mova" \
    org.opencontainers.image.description="Open-source media server built with Rust" \
    org.opencontainers.image.version=$MOVA_BUILD_VERSION \
    org.opencontainers.image.revision=$MOVA_BUILD_REVISION \
    org.opencontainers.image.source="https://github.com/RichesChiu/mova" \
    org.opencontainers.image.documentation="https://github.com/RichesChiu/mova/blob/master/docs/DEPLOYMENT.md" \
    org.opencontainers.image.licenses="AGPL-3.0-only"
WORKDIR /app

COPY --from=builder /app/target/release/mova-server /usr/local/bin/mova-server
COPY --from=web-builder /app/apps/mova-web/dist /app/web
COPY scripts/detect_intro.py /app/scripts/detect_intro.py

EXPOSE 36080

CMD ["mova-server"]
