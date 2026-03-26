FROM rust:1-bookworm AS builder
WORKDIR /app

COPY Cargo.toml ./
COPY Cargo.lock ./
COPY apps ./apps
COPY crates ./crates
COPY migrations ./migrations

RUN cargo build --release -p mova-server

FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates ffmpeg \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/mova-server /usr/local/bin/mova-server

EXPOSE 36080

CMD ["mova-server"]
