#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

cargo_metadata="$(cargo metadata --locked --no-deps --format-version 1)"
version="$(
  jq -r '.packages[] | select(.name == "mova-server") | .version' \
    <<<"$cargo_metadata"
)"

if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z]+([.-][0-9A-Za-z]+)*)?$ ]]; then
  echo "Workspace version is not release SemVer: $version" >&2
  exit 1
fi

if ! jq -e --arg version "$version" \
  'all(.packages[]; .version == $version)' \
  <<<"$cargo_metadata" >/dev/null; then
  echo "All workspace packages must use the release version $version." >&2
  exit 1
fi

printf '%s\n' "$version"
