# syntax=docker/dockerfile:1.7

FROM debian:trixie-slim AS ffmpeg-builder

ARG FFMPEG_COMMIT=f944afd04097178b7e3c0d6c7f4e524a9e8f6063
ARG FFMPEG_SOURCE_SHA256=8af9d494814124d2ad6eb2324f2d955e06a183242daee4d4c7e24df6d3b4ea0e
ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        gcc \
        libc6-dev \
        make \
        nasm \
    && rm -rf /var/lib/apt/lists/*

ADD --checksum=sha256:8af9d494814124d2ad6eb2324f2d955e06a183242daee4d4c7e24df6d3b4ea0e \
    https://codeload.github.com/FFmpeg/FFmpeg/tar.gz/${FFMPEG_COMMIT} /tmp/ffmpeg.tar.gz

RUN test "$FFMPEG_SOURCE_SHA256" = "8af9d494814124d2ad6eb2324f2d955e06a183242daee4d4c7e24df6d3b4ea0e" \
    && mkdir -p /usr/src/ffmpeg /opt/ffmpeg/bin /opt/ffmpeg/share \
    && tar --extract --gzip --file /tmp/ffmpeg.tar.gz \
        --directory /usr/src/ffmpeg --strip-components=1 \
    && cd /usr/src/ffmpeg \
    && ffmpeg_short_commit="$(printf '%.12s' "$FFMPEG_COMMIT")" \
    && ./configure \
        --prefix=/opt/ffmpeg \
        --disable-autodetect \
        --disable-debug \
        --disable-doc \
        --disable-network \
        --disable-shared \
        --enable-static \
        --disable-ffplay \
        --extra-version="mova-${ffmpeg_short_commit}" \
    && make -j"$(nproc)" ffmpeg ffprobe \
    && install -m 0755 ffmpeg ffprobe /opt/ffmpeg/bin/ \
    && strip /opt/ffmpeg/bin/ffmpeg /opt/ffmpeg/bin/ffprobe \
    && cp COPYING.LGPLv2.1 COPYING.LGPLv3 LICENSE.md /opt/ffmpeg/share/ \
    && { \
        printf '%s\n' \
            "Upstream project: FFmpeg" \
            "Repository: https://github.com/FFmpeg/FFmpeg" \
            "Commit: ${FFMPEG_COMMIT}" \
            "Source archive: https://codeload.github.com/FFmpeg/FFmpeg/tar.gz/${FFMPEG_COMMIT}" \
            "Source SHA-256: ${FFMPEG_SOURCE_SHA256}"; \
        /opt/ffmpeg/bin/ffmpeg -hide_banner -buildconf 2>&1; \
    } > /opt/ffmpeg/share/ffmpeg-source.txt

FROM debian:trixie-slim

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && mkdir -p /usr/share/mova/third-party \
    # Package-maintenance scripts are not used after this immutable image is built.
    && apt-get purge -y --allow-remove-essential perl-base \
    && ! command -v perl \
    && ! dpkg-query -W perl-base >/dev/null 2>&1 \
    && apt-get check \
    && test -z "$(dpkg --audit)" \
    && test -s /etc/ssl/certs/ca-certificates.crt \
    && dpkg-query -W -f='${binary:Package}\t${Version}\n' \
        | sort > /usr/share/mova/third-party/debian-packages.tsv \
    && rm -rf /var/lib/apt/lists/*

COPY --from=ffmpeg-builder /opt/ffmpeg/bin/ffmpeg /usr/local/bin/ffmpeg
COPY --from=ffmpeg-builder /opt/ffmpeg/bin/ffprobe /usr/local/bin/ffprobe
COPY --from=ffmpeg-builder /opt/ffmpeg/share/ /usr/share/mova/third-party/

WORKDIR /app
