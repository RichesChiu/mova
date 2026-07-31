FROM debian:trixie-slim

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates ffmpeg python3 \
    && mkdir -p /usr/share/mova/third-party \
    && ffmpeg_version="$(dpkg-query -W -f='${Version}' ffmpeg)" \
    && printf '%s\n' \
        "Debian binary package: ffmpeg ${ffmpeg_version}" \
        "Corresponding Debian source package and patches:" \
        "https://packages.debian.org/source/trixie/ffmpeg" \
        "https://sources.debian.org/src/ffmpeg/" \
        > /usr/share/mova/third-party/ffmpeg-source.txt \
    # Mova invokes only Python, FFmpeg, and FFprobe. Debian keeps perl-base in
    # its minimal rootfs for package-maintenance scripts, which are not used in
    # this immutable runtime image after the packages above have been installed.
    && apt-get purge -y --allow-remove-essential perl-base \
    && ! command -v perl \
    && ! dpkg-query -W perl-base >/dev/null 2>&1 \
    && apt-get check \
    && test -z "$(dpkg --audit)" \
    && test -s /etc/ssl/certs/ca-certificates.crt \
    && dpkg-query -W -f='${binary:Package}\t${Version}\n' \
        | sort > /usr/share/mova/third-party/debian-packages.tsv \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
