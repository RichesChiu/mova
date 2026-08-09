#!/usr/bin/env bash

set -euo pipefail

# This test script also serves as the fake docker executable through a temporary
# PATH entry, keeping the policy test self-contained.
if [[ "${MOVA_DOCKER_ATTESTATION_TEST_FAKE:-}" == 1 && "${0##*/}" == docker ]]; then
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
    '{{json .Manifest}}')
      printf '%s\n' "${MOVA_MOCK_MANIFEST_METADATA:?Missing mock manifest metadata}"
      ;;
    *)
      echo "Unexpected fake docker format: $format" >&2
      exit 1
      ;;
  esac
  exit 0
fi

REPOSITORY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=scripts/lib/docker-attestation-policy.sh
source "$REPOSITORY_ROOT/scripts/lib/docker-attestation-policy.sh"

TEST_DIRECTORY="$(mktemp -d)"
trap 'rm -rf "$TEST_DIRECTORY"' EXIT
mkdir -p "$TEST_DIRECTORY/bin"
ln -s "$REPOSITORY_ROOT/scripts/tests/test-docker-attestation-policy.sh" \
  "$TEST_DIRECTORY/bin/docker"

export PATH="$TEST_DIRECTORY/bin:$PATH"
export MOVA_DOCKER_ATTESTATION_TEST_FAKE=1
export MOVA_MOCK_DOCKER_LOG="$TEST_DIRECTORY/docker.log"
export MOVA_MOCK_MEDIA_TYPE='application/vnd.oci.image.index.v1+json'

IMAGE_REF='registry.example.test/mova@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
AMD64_DIGEST='sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'
ARM64_DIGEST='sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc'
AMD64_ATTESTATION_DIGEST='sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd'
ARM64_ATTESTATION_DIGEST='sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee'

VALID_IMAGE_METADATA='{
  "linux/amd64": {"os": "linux", "architecture": "amd64"},
  "linux/arm64": {"os": "linux", "architecture": "arm64"}
}'
VALID_MANIFEST_METADATA="$(
  jq -nc \
    --arg amd64 "$AMD64_DIGEST" \
    --arg arm64 "$ARM64_DIGEST" \
    --arg amd64_attestation "$AMD64_ATTESTATION_DIGEST" \
    --arg arm64_attestation "$ARM64_ATTESTATION_DIGEST" '
    {
      manifests: [
        {
          digest: $amd64,
          platform: {os: "linux", architecture: "amd64"}
        },
        {
          digest: $arm64,
          platform: {os: "linux", architecture: "arm64"}
        },
        {
          digest: $amd64_attestation,
          platform: {os: "unknown", architecture: "unknown"},
          annotations: {
            "vnd.docker.reference.type": "attestation-manifest",
            "vnd.docker.reference.digest": $amd64
          }
        },
        {
          digest: $arm64_attestation,
          platform: {os: "unknown", architecture: "unknown"},
          annotations: {
            "vnd.docker.reference.type": "attestation-manifest",
            "vnd.docker.reference.digest": $arm64
          }
        }
      ]
    }'
)"

export MOVA_MOCK_IMAGE_METADATA="$VALID_IMAGE_METADATA"
export MOVA_MOCK_MANIFEST_METADATA="$VALID_MANIFEST_METADATA"

fail() {
  echo "$1" >&2
  exit 1
}

run_case() {
  local case_name="$1"
  local image_ref="$2"
  local platforms="$3"

  CASE_STDOUT="$TEST_DIRECTORY/$case_name.out"
  CASE_STDERR="$TEST_DIRECTORY/$case_name.err"
  : >"$MOVA_MOCK_DOCKER_LOG"

  set +e
  validate_attested_image_index "$image_ref" "$platforms" \
    >"$CASE_STDOUT" 2>"$CASE_STDERR"
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

# A valid two-platform index has exactly one attestation referencing each image.
run_case valid-index "$IMAGE_REF" 'linux/amd64,linux/arm64'
assert_case 0 '' ''
assert_docker_calls 3

# The index fails closed when one platform has no attached attestation.
MOVA_MOCK_MANIFEST_METADATA="$(
  jq --arg arm64_attestation "$ARM64_ATTESTATION_DIGEST" '
    .manifests |= map(select(.digest != $arm64_attestation))
  ' <<<"$VALID_MANIFEST_METADATA"
)"
run_case missing-arm64-attestation "$IMAGE_REF" 'linux/amd64,linux/arm64'
assert_case 1 '' \
  "Image manifests and attestation references are incomplete or invalid: $IMAGE_REF"
assert_docker_calls 3

# An extra or wrong image platform is rejected before manifest references matter.
MOVA_MOCK_IMAGE_METADATA='{
  "linux/amd64": {"os": "linux", "architecture": "amd64"},
  "linux/arm64": {"os": "linux", "architecture": "arm64"},
  "linux/s390x": {"os": "linux", "architecture": "s390x"}
}'
run_case extra-platform "$IMAGE_REF" 'linux/amd64,linux/arm64'
assert_case 1 '' \
  "Attested image platform set does not match the expected platforms: $IMAGE_REF"
assert_docker_calls 2

MOVA_MOCK_IMAGE_METADATA='{
  "linux/amd64": {"os": "linux", "architecture": "amd64"},
  "linux/s390x": {"os": "linux", "architecture": "s390x"}
}'
run_case wrong-platform "$IMAGE_REF" 'linux/amd64,linux/arm64'
assert_case 1 '' \
  "Attested image platform set does not match the expected platforms: $IMAGE_REF"
assert_docker_calls 2

# Unsupported manifest media types are rejected without inspecting image data.
MOVA_MOCK_MEDIA_TYPE='application/vnd.oci.image.manifest.v1+json'
run_case unsupported-media-type "$IMAGE_REF" 'linux/amd64,linux/arm64'
assert_case 1 '' \
  "Attested image must be an OCI index or Docker manifest list: $IMAGE_REF (application/vnd.oci.image.manifest.v1+json)"
assert_docker_calls 1

# Invalid or mutable inputs fail before any registry inspection.
run_case unpinned-input 'registry.example.test/mova:preview' 'linux/amd64,linux/arm64'
assert_case 2 '' \
  'Attestation validation requires a digest-pinned image reference: registry.example.test/mova:preview'
assert_docker_calls 0

run_case malformed-digest \
  'registry.example.test/mova@sha256:deadbeef' 'linux/amd64,linux/arm64'
assert_case 2 '' \
  'Attestation validation requires a digest-pinned image reference: registry.example.test/mova@sha256:deadbeef'
assert_docker_calls 0

run_case invalid-platform "$IMAGE_REF" 'linux-amd64'
assert_case 2 '' 'Invalid Docker platform for attestation validation: linux-amd64'
assert_docker_calls 0

echo "Docker attestation policy tests passed."
