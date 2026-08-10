#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat >&2 <<'EOF'
Usage:
  notify-telegram-release.sh <version> <channel> <tag> <digest> <is-prerelease>

TELEGRAM_BOT_TOKEN and TELEGRAM_CHAT_ID must be provided through the environment.
EOF
}

if (( $# != 5 )); then
  usage
  exit 2
fi

VERSION="$1"
CHANNEL="$2"
TAG="$3"
DIGEST="$4"
IS_PRERELEASE="$5"
CURL_BIN="${MOVA_CURL_BIN:-curl}"
GITHUB_REPOSITORY="${GITHUB_REPOSITORY:-RichesChiu/mova}"
IMAGE_REPOSITORY="${IMAGE_REPOSITORY:-richeschiu/mova}"
TELEGRAM_BOT_TOKEN="${TELEGRAM_BOT_TOKEN:-}"
TELEGRAM_CHAT_ID="${TELEGRAM_CHAT_ID:-}"

if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
  echo "Release version is invalid." >&2
  exit 2
fi
if [[ "$TAG" != "v${VERSION}" ]]; then
  echo "Release tag must match the version." >&2
  exit 2
fi
if [[ "$CHANNEL" != "preview" && "$CHANNEL" != "latest" ]]; then
  echo "Release channel must be preview or latest." >&2
  exit 2
fi
if [[ "$IS_PRERELEASE" != "true" && "$IS_PRERELEASE" != "false" ]]; then
  echo "Prerelease state must be true or false." >&2
  exit 2
fi
if [[ "$IS_PRERELEASE" == "true" && "$CHANNEL" != "preview" ]] ||
  [[ "$IS_PRERELEASE" == "false" && "$CHANNEL" != "latest" ]]; then
  echo "Release channel does not match the prerelease state." >&2
  exit 2
fi
if [[ ! "$DIGEST" =~ ^sha256:[0-9a-f]{64}$ ]]; then
  echo "Published image digest is invalid." >&2
  exit 2
fi
if [[ ! "$GITHUB_REPOSITORY" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]]; then
  echo "GitHub repository is invalid." >&2
  exit 2
fi
if [[ ! "$IMAGE_REPOSITORY" =~ ^[a-z0-9][a-z0-9._-]*/[a-z0-9][a-z0-9._-]*$ ]]; then
  echo "Docker image repository is invalid." >&2
  exit 2
fi
if [[ ! "$TELEGRAM_BOT_TOKEN" =~ ^[0-9]+:[A-Za-z0-9_-]{20,}$ ]]; then
  echo "TELEGRAM_BOT_TOKEN is missing or invalid." >&2
  exit 2
fi
if [[ ! "$TELEGRAM_CHAT_ID" =~ ^-?[0-9]+$ &&
  ! "$TELEGRAM_CHAT_ID" =~ ^@[A-Za-z][A-Za-z0-9_]{4,31}$ ]]; then
  echo "TELEGRAM_CHAT_ID must be a numeric chat ID or public @username." >&2
  exit 2
fi

if [[ "$IS_PRERELEASE" == "true" ]]; then
  release_heading="🧪 MOVA ${VERSION} 预览版已发布 / Preview released"
else
  release_heading="🚀 MOVA ${VERSION} 正式版已发布 / Stable release"
fi

release_url="https://github.com/${GITHUB_REPOSITORY}/releases/tag/${TAG}"
message="$(printf '%s\n\nDocker: %s:%s\n固定版本 / Immutable: %s:%s\n平台 / Platforms: linux/amd64, linux/arm64\nDigest: %s\n\n更新说明 / Release notes:\n%s' \
  "$release_heading" \
  "$IMAGE_REPOSITORY" \
  "$CHANNEL" \
  "$IMAGE_REPOSITORY" \
  "$VERSION" \
  "$DIGEST" \
  "$release_url")"

response_file="$(mktemp)"
cleanup() {
  rm -f "$response_file"
}
trap cleanup EXIT

http_status="$(
  "$CURL_BIN" \
    --silent \
    --show-error \
    --connect-timeout 10 \
    --max-time 30 \
    --retry 3 \
    --retry-all-errors \
    --request POST \
    --data-urlencode "chat_id=${TELEGRAM_CHAT_ID}" \
    --data-urlencode "text=${message}" \
    --output "$response_file" \
    --write-out '%{http_code}' \
    "https://api.telegram.org/bot${TELEGRAM_BOT_TOKEN}/sendMessage"
)"

if [[ "$http_status" != "200" ]] ||
  ! jq -e '.ok == true' "$response_file" >/dev/null; then
  description="$(jq -r '.description // "unknown Telegram API error"' "$response_file" 2>/dev/null || true)"
  echo "Telegram rejected the release notification (HTTP ${http_status}): ${description}" >&2
  exit 1
fi

echo "Telegram release notification sent for ${TAG}."
