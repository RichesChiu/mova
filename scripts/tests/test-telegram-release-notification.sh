#!/usr/bin/env bash

set -euo pipefail

if [[ "${MOVA_TELEGRAM_TEST_FAKE:-}" == 1 && "${0##*/}" == curl ]]; then
  output_file=""
  printf '%s\n' "$@" >"$MOVA_MOCK_CURL_LOG"
  while (( $# > 0 )); do
    case "$1" in
      --output)
        output_file="$2"
        shift 2
        ;;
      *)
        shift
        ;;
    esac
  done
  if [[ -z "$output_file" ]]; then
    echo "Fake curl did not receive --output." >&2
    exit 1
  fi
  printf '%s' "$MOVA_MOCK_RESPONSE" >"$output_file"
  printf '%s' "$MOVA_MOCK_HTTP_STATUS"
  exit "${MOVA_MOCK_CURL_EXIT:-0}"
fi

REPOSITORY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$REPOSITORY_ROOT/scripts/notify-telegram-release.sh"
TEST_DIRECTORY="$(mktemp -d)"
trap 'rm -rf "$TEST_DIRECTORY"' EXIT
mkdir -p "$TEST_DIRECTORY/bin"
ln -s "$REPOSITORY_ROOT/scripts/tests/test-telegram-release-notification.sh" \
  "$TEST_DIRECTORY/bin/curl"

export MOVA_CURL_BIN="$TEST_DIRECTORY/bin/curl"
export MOVA_TELEGRAM_TEST_FAKE=1
export MOVA_MOCK_CURL_LOG="$TEST_DIRECTORY/curl.log"
export MOVA_MOCK_HTTP_STATUS=200
export MOVA_MOCK_RESPONSE='{"ok":true,"result":{"message_id":1}}'
export TELEGRAM_BOT_TOKEN='123456789:abcdefghijklmnopqrstuvwxyz_ABCDE'
export TELEGRAM_CHAT_ID='@mova_feedback'
export GITHUB_REPOSITORY='RichesChiu/mova'
export IMAGE_REPOSITORY='richeschiu/mova'

DIGEST='sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'

fail() {
  echo "$1" >&2
  exit 1
}

run_case() {
  local case_name="$1"
  shift
  CASE_STDOUT="$TEST_DIRECTORY/${case_name}.out"
  CASE_STDERR="$TEST_DIRECTORY/${case_name}.err"
  : >"$MOVA_MOCK_CURL_LOG"
  set +e
  "$SCRIPT" "$@" >"$CASE_STDOUT" 2>"$CASE_STDERR"
  CASE_STATUS=$?
  set -e
}

assert_status() {
  local expected="$1"
  [[ "$CASE_STATUS" == "$expected" ]] ||
    fail "Expected status $expected for ${CASE_STDOUT##*/}; got $CASE_STATUS."
}

run_case preview-success \
  1.4.0-preview.5 preview v1.4.0-preview.5 "$DIGEST" true
assert_status 0
grep -F 'Telegram release notification sent for v1.4.0-preview.5.' "$CASE_STDOUT" >/dev/null
grep -F '🧪 MOVA 1.4.0-preview.5 预览版已发布 / Preview released' "$MOVA_MOCK_CURL_LOG" >/dev/null
grep -F 'Docker: richeschiu/mova:preview' "$MOVA_MOCK_CURL_LOG" >/dev/null
grep -F 'https://github.com/RichesChiu/mova/releases/tag/v1.4.0-preview.5' \
  "$MOVA_MOCK_CURL_LOG" >/dev/null

run_case stable-success 1.4.0 latest v1.4.0 "$DIGEST" false
assert_status 0
grep -F '🚀 MOVA 1.4.0 正式版已发布 / Stable release' "$MOVA_MOCK_CURL_LOG" >/dev/null
grep -F 'Docker: richeschiu/mova:latest' "$MOVA_MOCK_CURL_LOG" >/dev/null

export MOVA_MOCK_HTTP_STATUS=400
export MOVA_MOCK_RESPONSE='{"ok":false,"description":"Bad Request: chat not found"}'
run_case telegram-rejection \
  1.4.0-preview.5 preview v1.4.0-preview.5 "$DIGEST" true
assert_status 1
grep -F 'Telegram rejected the release notification (HTTP 400): Bad Request: chat not found' \
  "$CASE_STDERR" >/dev/null

export MOVA_MOCK_HTTP_STATUS=200
export MOVA_MOCK_RESPONSE='{"ok":true}'
TELEGRAM_BOT_TOKEN='' run_case missing-token \
  1.4.0-preview.5 preview v1.4.0-preview.5 "$DIGEST" true
assert_status 2
grep -F 'TELEGRAM_BOT_TOKEN is missing or invalid.' "$CASE_STDERR" >/dev/null
[[ ! -s "$MOVA_MOCK_CURL_LOG" ]] || fail 'Missing token unexpectedly invoked curl.'

TELEGRAM_CHAT_ID='not-a-chat' run_case invalid-chat \
  1.4.0-preview.5 preview v1.4.0-preview.5 "$DIGEST" true
assert_status 2
grep -F 'TELEGRAM_CHAT_ID must be a numeric chat ID or public @username.' \
  "$CASE_STDERR" >/dev/null
[[ ! -s "$MOVA_MOCK_CURL_LOG" ]] || fail 'Invalid chat unexpectedly invoked curl.'

if grep -F "$TELEGRAM_BOT_TOKEN" "$TEST_DIRECTORY"/*.out "$TEST_DIRECTORY"/*.err >/dev/null; then
  fail 'Telegram bot token leaked into script output.'
fi

echo "Telegram release notification tests passed."
