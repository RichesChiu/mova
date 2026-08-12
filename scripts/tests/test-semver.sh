#!/usr/bin/env bash

set -euo pipefail

REPOSITORY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=scripts/lib/semver.sh
source "$REPOSITORY_ROOT/scripts/lib/semver.sh"

assert_compare() {
  local expected="$1"
  local left="$2"
  local right="$3"
  local actual

  actual="$(mova_semver_compare "$left" "$right")"
  [[ "$actual" == "$expected" ]] || {
    printf 'Expected %s for %s vs %s, got %s.\n' "$expected" "$left" "$right" "$actual" >&2
    exit 1
  }
}

assert_compare 0 1.5.0 1.5.0
assert_compare -1 1.4.9 1.5.0
assert_compare 1 2.0.0 1.99.99
assert_compare -1 1.5.0-preview.1 1.5.0-preview.2
assert_compare -1 1.5.0-preview.2 1.5.0-preview.10
assert_compare -1 1.5.0-preview.10 1.5.0
assert_compare 1 1.5.0 1.5.0-preview.10
assert_compare -1 1.0.0-alpha 1.0.0-alpha.1
assert_compare -1 1.0.0-alpha.1 1.0.0-alpha.beta
assert_compare -1 1.0.0-beta.11 1.0.0-rc.1
assert_compare 1 999999999999999999999.0.0 2.0.0
assert_compare -1 1.0.0-preview.999999999999999999999 1.0.0-preview.1000000000000000000000

set +e
mova_semver_compare 1.0 1.0.0 >/dev/null
invalid_status=$?
mova_semver_compare 1.0.0+build 1.0.1 >/dev/null
build_status=$?
mova_semver_compare 1.0.0 1.1.0-01 >/dev/null
leading_zero_status=$?
mova_semver_compare 1.0.0-alpha.1 1.0.0-alpha.01 >/dev/null
nested_leading_zero_status=$?
set -e
[[ "$invalid_status" -eq 2 && "$build_status" -eq 2 &&
  "$leading_zero_status" -eq 2 && "$nested_leading_zero_status" -eq 2 ]] || {
  echo 'Invalid SemVer input did not return status 2.' >&2
  exit 1
}

echo 'SemVer comparison tests passed.'
