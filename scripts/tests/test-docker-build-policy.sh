#!/usr/bin/env bash

set -euo pipefail

REPOSITORY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=scripts/lib/docker-build-policy.sh
source "$REPOSITORY_ROOT/scripts/lib/docker-build-policy.sh"

assert_action() {
  local expected="$1"
  local publish_mode="$2"
  local has_required_platforms="$3"
  local actual

  actual="$(resolve_base_image_action "$publish_mode" "$has_required_platforms")"
  if [[ "$actual" != "$expected" ]]; then
    echo "Expected $expected for mode=$publish_mode platforms=$has_required_platforms; got $actual." >&2
    exit 1
  fi
}

# Automatic publication reuses a complete multi-platform base and performs a
# clean refresh only when a required platform is absent.
assert_action reuse auto 1
assert_action refresh auto 0

# Explicit publication refreshes every base image without consulting registry
# availability; every accepted spelling has the same behavior.
assert_action refresh 1 0
assert_action refresh true 1
assert_action refresh yes 1

# Explicitly disabled publication always reuses the configured reference. A
# missing image is allowed to fail later at the consuming application build.
assert_action reuse 0 0
assert_action reuse false 0
assert_action reuse no 0

TEST_DIRECTORY="$(mktemp -d)"
trap 'rm -rf "$TEST_DIRECTORY"' EXIT

set +e
resolve_base_image_action sometimes 1 \
  >"$TEST_DIRECTORY/invalid-mode.out" 2>"$TEST_DIRECTORY/invalid-mode.err"
invalid_mode_status=$?
set -e
if [[ "$invalid_mode_status" -ne 2 ]]; then
  echo "An invalid base image publication mode was accepted." >&2
  exit 1
fi
grep -F 'Invalid MOVA_PUBLISH_BASE_IMAGES value: sometimes' \
  "$TEST_DIRECTORY/invalid-mode.err" >/dev/null

set +e
resolve_base_image_action auto unknown \
  >"$TEST_DIRECTORY/invalid-platform.out" 2>"$TEST_DIRECTORY/invalid-platform.err"
invalid_platform_status=$?
set -e
if [[ "$invalid_platform_status" -ne 2 ]]; then
  echo "An invalid platform availability value was accepted." >&2
  exit 1
fi
grep -F 'Base image platform availability must be 0 or 1.' \
  "$TEST_DIRECTORY/invalid-platform.err" >/dev/null

echo "Docker build policy tests passed."
