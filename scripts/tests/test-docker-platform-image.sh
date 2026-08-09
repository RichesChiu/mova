#!/usr/bin/env bash

set -euo pipefail

# The test script doubles as the fake docker executable through a temporary
# PATH entry, so the repository needs only this one test file.
if [[ "${MOVA_DOCKER_PLATFORM_IMAGE_TEST_FAKE:-}" == 1 && "${0##*/}" == docker ]]; then
  if (( $# != 6 )) ||
    [[ "$1" != buildx || "$2" != imagetools || "$3" != inspect || "$5" != --format ]]; then
    echo "Unexpected fake docker invocation: $*" >&2
    exit 1
  fi

  image_ref="$4"
  format="$6"
  printf '%s\t%s\n' "$image_ref" "$format" >>"$MOVA_MOCK_DOCKER_LOG"

  case "$format" in
    '{{.Manifest.MediaType}}')
      printf '%s\n' "${MOVA_MOCK_MEDIA_TYPE:?Missing mock media type}"
      ;;
    '{{json .Image}}')
      printf '%s\n' "${MOVA_MOCK_IMAGE_METADATA:?Missing mock image metadata}"
      ;;
    *'.Manifest.Manifests'*)
      printf '%s' "${MOVA_MOCK_PLATFORM_DIGEST:-}"
      ;;
    *)
      echo "Unexpected fake docker format: $format" >&2
      exit 1
      ;;
  esac
  exit 0
fi

REPOSITORY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=scripts/lib/docker-platform-image.sh
source "$REPOSITORY_ROOT/scripts/lib/docker-platform-image.sh"

TEST_DIRECTORY="$(mktemp -d)"
trap 'rm -rf "$TEST_DIRECTORY"' EXIT
mkdir -p "$TEST_DIRECTORY/bin"
ln -s "$REPOSITORY_ROOT/scripts/tests/test-docker-platform-image.sh" "$TEST_DIRECTORY/bin/docker"

export PATH="$TEST_DIRECTORY/bin:$PATH"
export MOVA_DOCKER_PLATFORM_IMAGE_TEST_FAKE=1
export MOVA_MOCK_DOCKER_LOG="$TEST_DIRECTORY/docker.log"
export MOVA_MOCK_MEDIA_TYPE=""
export MOVA_MOCK_IMAGE_METADATA=""
export MOVA_MOCK_PLATFORM_DIGEST=""

IMAGE_REF='registry.example.test/mova@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
CHILD_DIGEST='sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'

fail() {
  echo "$1" >&2
  exit 1
}

run_case() {
  local case_name="$1"
  local image_ref="$2"
  local platform="$3"

  CASE_STDOUT="$TEST_DIRECTORY/$case_name.out"
  CASE_STDERR="$TEST_DIRECTORY/$case_name.err"
  : >"$MOVA_MOCK_DOCKER_LOG"

  set +e
  resolve_platform_image_ref "$image_ref" "$platform" >"$CASE_STDOUT" 2>"$CASE_STDERR"
  CASE_STATUS=$?
  set -e
}

assert_case() {
  local expected_status="$1"
  local expected_stdout="$2"
  local expected_stderr="$3"
  local actual_stdout
  local actual_stderr

  actual_stdout="$(<"$CASE_STDOUT")"
  actual_stderr="$(<"$CASE_STDERR")"

  [[ "$CASE_STATUS" == "$expected_status" ]] ||
    fail "Expected status $expected_status for ${CASE_STDOUT##*/}; got $CASE_STATUS."
  [[ "$actual_stdout" == "$expected_stdout" ]] ||
    fail "Unexpected stdout for ${CASE_STDOUT##*/}: $actual_stdout"
  [[ "$actual_stderr" == "$expected_stderr" ]] ||
    fail "Unexpected stderr for ${CASE_STDERR##*/}: $actual_stderr"
}

assert_docker_calls() {
  local expected="$1"
  local actual=0
  local ignored

  while IFS= read -r ignored; do
    actual=$((actual + 1))
  done <"$MOVA_MOCK_DOCKER_LOG"

  [[ "$actual" == "$expected" ]] ||
    fail "Expected $expected fake docker calls; got $actual."
}

# Local CI tags have no registry manifest and pass through without invoking
# Docker inspection.
run_case local-tag 'registry.example.test/mova:test' linux/amd64
assert_case 0 'registry.example.test/mova:test' ''
assert_docker_calls 0

# Digest-pinned references validate the requested platform before inspection.
run_case invalid-platform "$IMAGE_REF" 'linux-amd64'
assert_case 2 '' 'Invalid Docker platform: linux-amd64'
assert_docker_calls 0

# A direct OCI manifest is reusable only when its image metadata matches.
MOVA_MOCK_MEDIA_TYPE='application/vnd.oci.image.manifest.v1+json'
MOVA_MOCK_IMAGE_METADATA='{"os":"linux","architecture":"amd64"}'
run_case direct-manifest-match "$IMAGE_REF" linux/amd64
assert_case 0 "$IMAGE_REF" ''
assert_docker_calls 2

MOVA_MOCK_IMAGE_METADATA='{"os":"linux","architecture":"arm64"}'
run_case direct-manifest-mismatch "$IMAGE_REF" linux/amd64
assert_case 1 '' "$IMAGE_REF does not contain the requested platform linux/amd64."
assert_docker_calls 2

# An OCI index resolves the matching child digest and preserves the repository.
MOVA_MOCK_MEDIA_TYPE='application/vnd.oci.image.index.v1+json'
MOVA_MOCK_PLATFORM_DIGEST="$CHILD_DIGEST"
run_case index-child "$IMAGE_REF" linux/arm64/v8
assert_case 0 "registry.example.test/mova@$CHILD_DIGEST" ''
assert_docker_calls 2
grep -F '(eq .Platform.OS "linux")' "$MOVA_MOCK_DOCKER_LOG" >/dev/null
grep -F '(eq .Platform.Architecture "arm64")' "$MOVA_MOCK_DOCKER_LOG" >/dev/null
grep -F '(eq .Platform.Variant "v8")' "$MOVA_MOCK_DOCKER_LOG" >/dev/null

# Missing and malformed child digests both fail closed.
MOVA_MOCK_PLATFORM_DIGEST=''
run_case index-child-missing "$IMAGE_REF" linux/amd64
assert_case 1 '' "Could not resolve a manifest digest for $IMAGE_REF on linux/amd64."
assert_docker_calls 2

MOVA_MOCK_PLATFORM_DIGEST='sha256:deadbeef'
run_case index-child-invalid "$IMAGE_REF" linux/amd64
assert_case 1 '' "Could not resolve a manifest digest for $IMAGE_REF on linux/amd64."
assert_docker_calls 2

# Unknown manifest formats are rejected without attempting child resolution.
MOVA_MOCK_MEDIA_TYPE='application/vnd.example.unsupported+json'
run_case unsupported-media-type "$IMAGE_REF" linux/amd64
assert_case 1 '' \
  "Unsupported Docker manifest media type for $IMAGE_REF: application/vnd.example.unsupported+json"
assert_docker_calls 1

echo "Docker platform image tests passed."
