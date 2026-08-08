#!/usr/bin/env bash

set -euo pipefail

DOCKER_HUB_API_BASE="${MOVA_DOCKER_HUB_API_BASE:-https://hub.docker.com}"
DOCKER_REPOSITORY="${MOVA_DOCKER_REPOSITORY:-richeschiu/mova}"
CANDIDATE_PREFIX="${MOVA_DOCKER_CANDIDATE_PREFIX:-publish-}"
MAX_AGE_HOURS="${MOVA_DOCKER_CANDIDATE_MAX_AGE_HOURS:-72}"
CURL_BIN="${MOVA_CURL_BIN:-curl}"

usage() {
  cat >&2 <<'EOF'
Usage:
  cleanup-docker-candidate-tags.sh exact <candidate-tag>
  cleanup-docker-candidate-tags.sh prune [max-age-hours]

Only Docker Hub tags beginning with MOVA_DOCKER_CANDIDATE_PREFIX (publish- by default)
can be deleted. The script never deletes manifests by digest.
EOF
}

fail() {
  echo "$*" >&2
  exit 2
}

if [[ ! "$DOCKER_REPOSITORY" =~ ^[a-z0-9][a-z0-9._-]*/[a-z0-9][a-z0-9._-]*$ ]]; then
  fail "MOVA_DOCKER_REPOSITORY must contain exactly one lowercase namespace/repository pair."
fi
if [[ ! "$CANDIDATE_PREFIX" =~ ^[A-Za-z0-9_][A-Za-z0-9_.-]*$ ]]; then
  fail "MOVA_DOCKER_CANDIDATE_PREFIX is not a valid Docker tag prefix."
fi

IFS=/ read -r DOCKER_NAMESPACE DOCKER_REPOSITORY_NAME <<<"$DOCKER_REPOSITORY"
AUTH_HEADER_FILE=""
RESPONSE_FILE=""

cleanup() {
  if [[ -n "$AUTH_HEADER_FILE" ]]; then
    rm -f "$AUTH_HEADER_FILE"
  fi
  if [[ -n "$RESPONSE_FILE" ]]; then
    rm -f "$RESPONSE_FILE"
  fi
}
trap cleanup EXIT

require_credentials() {
  if [[ -z "${DOCKERHUB_USERNAME:-}" || -z "${DOCKERHUB_TOKEN:-}" ]]; then
    fail "DOCKERHUB_USERNAME and DOCKERHUB_TOKEN are required."
  fi
}

authenticate() {
  local response access_token

  require_credentials
  response="$({
    jq -nc \
      '{identifier: env.DOCKERHUB_USERNAME, secret: env.DOCKERHUB_TOKEN}'
  } | "$CURL_BIN" \
    --fail-with-body \
    --silent \
    --show-error \
    --connect-timeout 10 \
    --max-time 30 \
    --retry 3 \
    --retry-all-errors \
    --request POST \
    --header 'Content-Type: application/json' \
    --data-binary @- \
    "${DOCKER_HUB_API_BASE}/v2/auth/token")"
  access_token="$(jq -er '.access_token' <<<"$response")"

  AUTH_HEADER_FILE="$(mktemp)"
  chmod 600 "$AUTH_HEADER_FILE"
  printf 'Authorization: Bearer %s\n' "$access_token" >"$AUTH_HEADER_FILE"
}

validate_candidate_tag() {
  local tag="$1"

  if (( ${#tag} > 128 )) || [[ ! "$tag" =~ ^[A-Za-z0-9_][A-Za-z0-9_.-]*$ ]]; then
    fail "Candidate tag is not a valid Docker tag: $tag"
  fi
  if [[ "$tag" != "$CANDIDATE_PREFIX"* ]]; then
    fail "Refusing to delete a non-candidate tag: $tag"
  fi
}

delete_candidate_tag() {
  local tag="$1"
  local endpoint http_status

  validate_candidate_tag "$tag"
  endpoint="${DOCKER_HUB_API_BASE}/v2/namespaces/${DOCKER_NAMESPACE}/repositories/${DOCKER_REPOSITORY_NAME}/tags/${tag}"
  RESPONSE_FILE="$(mktemp)"
  if ! http_status="$("$CURL_BIN" \
    --silent \
    --show-error \
    --connect-timeout 10 \
    --max-time 30 \
    --retry 3 \
    --retry-all-errors \
    --request DELETE \
    --header @"$AUTH_HEADER_FILE" \
    --output "$RESPONSE_FILE" \
    --write-out '%{http_code}' \
    "$endpoint")"; then
    echo "Docker Hub request failed while deleting candidate tag $tag." >&2
    return 1
  fi

  case "$http_status" in
    204)
      echo "Deleted Docker candidate tag: ${DOCKER_REPOSITORY}:${tag}"
      ;;
    404)
      echo "Docker candidate tag is already absent: ${DOCKER_REPOSITORY}:${tag}"
      ;;
    *)
      echo "Docker Hub rejected deletion of candidate tag $tag (HTTP $http_status)." >&2
      if [[ -s "$RESPONSE_FILE" ]]; then
        cat "$RESPONSE_FILE" >&2
        echo >&2
      fi
      return 1
      ;;
  esac
  rm -f "$RESPONSE_FILE"
  RESPONSE_FILE=""
}

prune_candidate_tags() {
  local max_age_hours="$1"
  local cutoff page endpoint response next
  local candidate
  local -a candidates=()

  if [[ ! "$max_age_hours" =~ ^[0-9]+$ ]]; then
    fail "The candidate retention period must be a non-negative integer number of hours."
  fi

  cutoff="$(( $(date -u +%s) - max_age_hours * 3600 ))"
  page=1
  while true; do
    endpoint="${DOCKER_HUB_API_BASE}/v2/namespaces/${DOCKER_NAMESPACE}/repositories/${DOCKER_REPOSITORY_NAME}/tags?page_size=100&page=${page}"
    response="$("$CURL_BIN" \
      --fail-with-body \
      --silent \
      --show-error \
      --connect-timeout 10 \
      --max-time 30 \
      --retry 3 \
      --retry-all-errors \
      --header @"$AUTH_HEADER_FILE" \
      "$endpoint")"

    while IFS= read -r candidate; do
      candidates+=("$candidate")
    done < <(
      jq -r \
        --arg prefix "$CANDIDATE_PREFIX" \
        --argjson cutoff "$cutoff" \
        '.results[]? |
          select(.name | startswith($prefix)) |
          (.last_updated // "" | sub("\\.[0-9]+Z$"; "Z") | fromdateiso8601?) as $updated |
          select($updated != null and $updated <= $cutoff) |
          .name' <<<"$response"
    )
    next="$(jq -r '.next // empty' <<<"$response")"
    if [[ -z "$next" ]]; then
      break
    fi
    page="$((page + 1))"
    if (( page > 10000 )); then
      echo "Docker Hub tag pagination exceeded the safety limit." >&2
      return 1
    fi
  done

  for candidate in "${candidates[@]}"; do
    delete_candidate_tag "$candidate"
  done
}

mode="${1:-}"
case "$mode" in
  exact)
    if (( $# != 2 )); then
      usage
      exit 2
    fi
    validate_candidate_tag "$2"
    authenticate
    delete_candidate_tag "$2"
    ;;
  prune)
    if (( $# > 2 )); then
      usage
      exit 2
    fi
    MAX_AGE_HOURS="${2:-$MAX_AGE_HOURS}"
    authenticate
    prune_candidate_tags "$MAX_AGE_HOURS"
    ;;
  *)
    usage
    exit 2
    ;;
esac
