#!/usr/bin/env bash

set -euo pipefail

REPOSITORY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="${REPOSITORY_ROOT}/scripts/cleanup-docker-candidate-tags.sh"
MOCK_CURL="${REPOSITORY_ROOT}/scripts/tests/mock-docker-hub-curl.sh"
TEST_DIRECTORY="$(mktemp -d)"
trap 'rm -rf "$TEST_DIRECTORY"' EXIT

export DOCKERHUB_USERNAME=test-user
export DOCKERHUB_TOKEN=test-secret
export MOVA_CURL_BIN="$MOCK_CURL"
export MOVA_DOCKER_HUB_API_BASE=https://hub.docker.com
export MOVA_DOCKER_REPOSITORY=richeschiu/mova
export MOVA_MOCK_CURL_LOG="${TEST_DIRECTORY}/requests.log"

if "$SCRIPT" exact latest >"${TEST_DIRECTORY}/unsafe.out" 2>"${TEST_DIRECTORY}/unsafe.err"; then
  echo "The cleanup script accepted an unsafe non-candidate tag." >&2
  exit 1
fi
grep -F 'Refusing to delete a non-candidate tag: latest' "${TEST_DIRECTORY}/unsafe.err" >/dev/null
test ! -e "$MOVA_MOCK_CURL_LOG"

"$SCRIPT" exact publish-release-candidate >/dev/null
grep -F $'POST\thttps://hub.docker.com/v2/auth/token' "$MOVA_MOCK_CURL_LOG" >/dev/null
grep -F $'DELETE\thttps://hub.docker.com/v2/namespaces/richeschiu/repositories/mova/tags/publish-release-candidate' "$MOVA_MOCK_CURL_LOG" >/dev/null
if grep -F 'test-secret' "$MOVA_MOCK_CURL_LOG" >/dev/null; then
  echo "The Docker Hub credential leaked into the request log." >&2
  exit 1
fi

: >"$MOVA_MOCK_CURL_LOG"
"$SCRIPT" prune 72 >/dev/null
grep -F $'GET\thttps://hub.docker.com/v2/namespaces/richeschiu/repositories/mova/tags?page_size=100&page=1' "$MOVA_MOCK_CURL_LOG" >/dev/null
grep -F $'GET\thttps://hub.docker.com/v2/namespaces/richeschiu/repositories/mova/tags?page_size=100&page=2' "$MOVA_MOCK_CURL_LOG" >/dev/null
grep -F $'DELETE\thttps://hub.docker.com/v2/namespaces/richeschiu/repositories/mova/tags/publish-old-first' "$MOVA_MOCK_CURL_LOG" >/dev/null
grep -F $'DELETE\thttps://hub.docker.com/v2/namespaces/richeschiu/repositories/mova/tags/publish-old-second' "$MOVA_MOCK_CURL_LOG" >/dev/null
if grep -E 'tags/(publish-recent|latest|1\.3\.1)$' "$MOVA_MOCK_CURL_LOG" >/dev/null; then
  echo "The cleanup script deleted a retained or protected tag." >&2
  exit 1
fi

echo "Docker candidate cleanup tests passed."
