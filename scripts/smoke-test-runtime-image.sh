#!/usr/bin/env bash
set -euo pipefail

IMAGE_REF="${1:-}"
PLATFORMS="${2:-linux/amd64}"

if [[ -z "$IMAGE_REF" ]]; then
  echo "Usage: $0 <image-reference> [comma-separated-platforms]" >&2
  exit 2
fi

IFS="," read -r -a smoke_platforms <<< "$PLATFORMS"
tested_platforms=0
for platform in "${smoke_platforms[@]}"; do
  platform="${platform//[[:space:]]/}"
  if [[ -z "$platform" ]]; then
    continue
  fi

  tested_platforms=$((tested_platforms + 1))
  echo "Smoke-testing $IMAGE_REF on $platform"
  # The single-quoted program is intentionally evaluated by /bin/sh inside the container.
  # shellcheck disable=SC2016
  docker run --rm \
    --platform "$platform" \
    --entrypoint /bin/sh \
    "$IMAGE_REF" \
    -ec '
      ! command -v perl
      ! dpkg-query -W perl-base >/dev/null 2>&1
      ! command -v python3
      for absent_package in ffmpeg python3 librist4 libcjson1 libssh-4; do
        ! dpkg-query -W "$absent_package" >/dev/null 2>&1
      done
      apt-get check
      test -z "$(dpkg --audit)"
      test -s /etc/ssl/certs/ca-certificates.crt
      test -s /usr/share/mova/third-party/debian-packages.tsv
      test -s /usr/share/mova/third-party/ffmpeg-source.txt
      test -s /usr/share/mova/third-party/COPYING.LGPLv2.1
      test -s /usr/share/mova/third-party/COPYING.LGPLv3
      ! grep -q "^perl-base[[:space:]]" /usr/share/mova/third-party/debian-packages.tsv
      test ! -e /app/scripts/detect_intro.py
      test ! -e /app/scripts/publish-docker-images.sh
      test ! -e /app/scripts/smoke-test-runtime-image.sh
      test -x /usr/local/bin/mova-server
      if /usr/local/bin/mova-server > /tmp/server.out 2> /tmp/server.err; then
        echo "mova-server unexpectedly started without MOVA_DATABASE_URL" >&2
        exit 1
      fi
      grep -q "missing MOVA_DATABASE_URL" /tmp/server.err
      ffmpeg -version | grep -q "f944afd04097"
      ffprobe -version
      ffmpeg -hide_banner -protocols > /tmp/protocols.txt
      grep -Eq "^[[:space:]]+file$" /tmp/protocols.txt
      grep -Eq "^[[:space:]]+pipe$" /tmp/protocols.txt
      ! grep -Eq "^[[:space:]]+(http|https|rist|rtmp|rtmps|tcp|udp)$" /tmp/protocols.txt
      ffmpeg -hide_banner -loglevel error -nostdin -y \
        -f lavfi -i testsrc2=size=32x32:rate=1 \
        -f lavfi -i sine=frequency=440 \
        -t 1 -c:v mpeg4 -c:a aac /tmp/sample.mp4
      ffprobe -v error -show_format -show_streams -of json \
        /tmp/sample.mp4 > /tmp/probe.json
      test -s /tmp/probe.json
      ffmpeg -hide_banner -loglevel error -nostdin -y \
        -i /tmp/sample.mp4 -map 0:v:0 -map 0:a:0 -c copy /tmp/remux.mp4
      test -s /tmp/remux.mp4
      printf "1\n00:00:00,000 --> 00:00:00,500\nMova\n" |
        ffmpeg -hide_banner -loglevel error -nostdin -y \
          -f srt -i pipe:0 -f webvtt pipe:1 > /tmp/subtitle.vtt
      test -s /tmp/subtitle.vtt
      ffmpeg -hide_banner -loglevel error -nostdin -y \
        -i /tmp/sample.mp4 -vn -ac 1 -ar 8000 -t 1 -f s16le /tmp/intro.pcm
      test "$(wc -c < /tmp/intro.pcm | tr -d " ")" -eq 16000
    '
done

if ((tested_platforms == 0)); then
  echo "At least one Docker platform is required for runtime smoke testing." >&2
  exit 2
fi
