#!/usr/bin/env bash

set -euo pipefail

method=GET
output_file=""
write_out=""
url=""

while (( $# > 0 )); do
  case "$1" in
    --request)
      method="$2"
      shift 2
      ;;
    --output)
      output_file="$2"
      shift 2
      ;;
    --write-out)
      write_out="$2"
      shift 2
      ;;
    --header|--connect-timeout|--max-time|--retry)
      shift 2
      ;;
    --data-binary)
      if [[ "$2" == "@-" ]]; then
        payload="$(cat)"
        if [[ "$payload" != *'"identifier":"test-user"'* || "$payload" != *'"secret":"test-secret"'* ]]; then
          echo "Unexpected authentication payload." >&2
          exit 1
        fi
      fi
      shift 2
      ;;
    --fail-with-body|--silent|--show-error|--retry-all-errors)
      shift
      ;;
    http://*|https://*)
      url="$1"
      shift
      ;;
    *)
      echo "Unexpected mock curl argument: $1" >&2
      exit 1
      ;;
  esac
done

printf '%s\t%s\n' "$method" "$url" >>"$MOVA_MOCK_CURL_LOG"

if [[ "$url" == */v2/auth/token ]]; then
  printf '{"access_token":"temporary-test-token"}\n'
  exit 0
fi

if [[ "$method" == DELETE ]]; then
  if [[ -n "$output_file" ]]; then
    : >"$output_file"
  fi
  if [[ "$write_out" == '%{http_code}' ]]; then
    printf '204'
  fi
  exit 0
fi

if [[ "$url" == *'&page=1' ]]; then
  cat <<'EOF'
{
  "next": "https://hub.docker.com/mock?page=2",
  "results": [
    {"name": "publish-old-first", "last_updated": "2000-01-01T00:00:00.000000Z"},
    {"name": "publish-recent", "last_updated": "2999-01-01T00:00:00Z"},
    {"name": "latest", "last_updated": "2000-01-01T00:00:00Z"},
    {"name": "1.3.1", "last_updated": "2000-01-01T00:00:00Z"}
  ]
}
EOF
  exit 0
fi

if [[ "$url" == *'&page=2' ]]; then
  cat <<'EOF'
{
  "next": null,
  "results": [
    {"name": "publish-old-second", "last_updated": "2001-01-01T00:00:00Z"}
  ]
}
EOF
  exit 0
fi

echo "Unexpected mock Docker Hub request: $method $url" >&2
exit 1
