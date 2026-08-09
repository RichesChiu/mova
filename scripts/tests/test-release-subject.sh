#!/usr/bin/env bash

set -euo pipefail

REPOSITORY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=scripts/lib/release-subject.sh
source "$REPOSITORY_ROOT/scripts/lib/release-subject.sh"

EXPECTED_SUBJECT="chore(release): prepare 1.4.0-preview.3"

assert_matches() {
  local subject="$1"

  if ! mova_release_subject_matches "$subject" "$EXPECTED_SUBJECT"; then
    echo "Expected release subject to match: $subject" >&2
    exit 1
  fi
}

assert_rejected() {
  local subject="$1"

  if mova_release_subject_matches "$subject" "$EXPECTED_SUBJECT"; then
    echo "Expected release subject to be rejected: $subject" >&2
    exit 1
  fi
}

assert_matches "$EXPECTED_SUBJECT"
assert_matches "$EXPECTED_SUBJECT (#1)"
assert_matches "$EXPECTED_SUBJECT (#54)"
assert_matches "$EXPECTED_SUBJECT (#123456)"

assert_rejected ""
assert_rejected "chore(release): prepare 1.4.0-preview.2"
assert_rejected "$EXPECTED_SUBJECT(#54)"
assert_rejected "$EXPECTED_SUBJECT  (#54)"
assert_rejected "$EXPECTED_SUBJECT (#0)"
assert_rejected "$EXPECTED_SUBJECT (#01)"
assert_rejected "$EXPECTED_SUBJECT (#abc)"
assert_rejected "$EXPECTED_SUBJECT #54"
assert_rejected "$EXPECTED_SUBJECT (#54) extra"
assert_rejected "$EXPECTED_SUBJECT (#54) (#55)"
assert_rejected "prefix $EXPECTED_SUBJECT (#54)"

set +e
mova_release_subject_matches "$EXPECTED_SUBJECT"
invalid_arity_status=$?
mova_release_subject_matches "$EXPECTED_SUBJECT" ""
empty_expected_status=$?
set -e
if [[ "$invalid_arity_status" -ne 2 || "$empty_expected_status" -ne 2 ]]; then
  echo "Invalid matcher input did not return status 2." >&2
  exit 1
fi

echo "Release subject tests passed."
