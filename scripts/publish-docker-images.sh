#!/usr/bin/env bash
set -euo pipefail

PLATFORMS="${MOVA_DOCKER_PLATFORMS:-linux/amd64,linux/arm64}"
IMAGE_TAG="${MOVA_DOCKER_IMAGE_TAG:-richeschiu/mova:latest}"
PUBLISH_BASE_IMAGES="${MOVA_PUBLISH_BASE_IMAGES:-auto}"
ALLOW_UNRELEASED="${MOVA_ALLOW_UNRELEASED:-0}"
ACCEPT_UNFIXED_CVES="${MOVA_ACCEPT_UNFIXED_CVES:-}"
SCOUT_VEX_LOCATION="${MOVA_SCOUT_VEX_LOCATION:-}"
SCOUT_VEX_AUTHORS="${MOVA_SCOUT_VEX_AUTHORS:-}"
VERIFY_IMAGE_REF="${MOVA_VERIFY_IMAGE_REF:-}"
SMOKE_TEST_SCRIPT="${MOVA_SMOKE_TEST_SCRIPT:-scripts/smoke-test-runtime-image.sh}"
DEFAULT_BUILD_VERSION="development"
if [[ "$IMAGE_TAG" == *:* ]]; then
  DEFAULT_BUILD_VERSION="${IMAGE_TAG##*:}"
fi
BUILD_VERSION="${MOVA_BUILD_VERSION:-$DEFAULT_BUILD_VERSION}"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# shellcheck source=scripts/lib/docker-build-policy.sh
source "$ROOT_DIR/scripts/lib/docker-build-policy.sh"
# shellcheck source=scripts/lib/docker-attestation-policy.sh
source "$ROOT_DIR/scripts/lib/docker-attestation-policy.sh"

if [[ "$SMOKE_TEST_SCRIPT" != /* ]]; then
  SMOKE_TEST_SCRIPT="$ROOT_DIR/$SMOKE_TEST_SCRIPT"
fi
if [[ ! -f "$SMOKE_TEST_SCRIPT" || ! -x "$SMOKE_TEST_SCRIPT" || -L "$SMOKE_TEST_SCRIPT" ]]; then
  echo "MOVA_SMOKE_TEST_SCRIPT must identify an executable, non-symlinked repository script." >&2
  exit 2
fi
SMOKE_TEST_DIRECTORY="$(cd "$(dirname "$SMOKE_TEST_SCRIPT")" && pwd -P)"
SMOKE_TEST_SCRIPT="$SMOKE_TEST_DIRECTORY/$(basename "$SMOKE_TEST_SCRIPT")"
case "$SMOKE_TEST_SCRIPT" in
  "$ROOT_DIR"/scripts/*)
    ;;
  *)
    echo "MOVA_SMOKE_TEST_SCRIPT must remain inside the repository scripts directory." >&2
    exit 2
    ;;
esac

if [[ "$IMAGE_TAG" == *@* || "$IMAGE_TAG" != *:* || -z "${IMAGE_TAG##*:}" || "${IMAGE_TAG##*:}" == */* ]]; then
  echo "MOVA_DOCKER_IMAGE_TAG must include an explicit tag: $IMAGE_TAG" >&2
  exit 2
fi

if [[ -n "$VERIFY_IMAGE_REF" ]]; then
  if [[ ! "$VERIFY_IMAGE_REF" =~ ^.+@sha256:[0-9a-f]{64}$ ]]; then
    echo "MOVA_VERIFY_IMAGE_REF must be pinned by a sha256 digest: $VERIFY_IMAGE_REF" >&2
    exit 2
  fi
  if [[ "${VERIFY_IMAGE_REF%@*}" != "${IMAGE_TAG%:*}" ]]; then
    echo "MOVA_VERIFY_IMAGE_REF must use the release image repository: ${IMAGE_TAG%:*}" >&2
    exit 2
  fi
fi

case "$ALLOW_UNRELEASED" in
  0|1)
    ;;
  *)
    echo "MOVA_ALLOW_UNRELEASED must be 0 or 1." >&2
    exit 2
    ;;
esac

SCOUT_VEX_ARGS=()
if [[ -n "$SCOUT_VEX_LOCATION" ]]; then
  if [[ ! -e "$SCOUT_VEX_LOCATION" ]]; then
    echo "MOVA_SCOUT_VEX_LOCATION does not exist: $SCOUT_VEX_LOCATION" >&2
    exit 2
  fi
  SCOUT_VEX_ARGS+=(--vex-location "$SCOUT_VEX_LOCATION" --ignore-suppressed)
fi

if [[ -n "$SCOUT_VEX_AUTHORS" ]]; then
  IFS="," read -r -a scout_vex_authors <<< "$SCOUT_VEX_AUTHORS"
  for author in "${scout_vex_authors[@]}"; do
    author="$(printf '%s' "$author" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
    if [[ -n "$author" ]]; then
      SCOUT_VEX_ARGS+=(--vex-author "$author")
    fi
  done
fi

normalize_cve_list() {
  printf '%s' "$1" \
    | tr ',[:space:]' '\n' \
    | sed '/^$/d' \
    | LC_ALL=C sort -u \
    | paste -sd, -
}

NORMALIZED_ACCEPTED_UNFIXED_CVES="$(normalize_cve_list "$ACCEPT_UNFIXED_CVES")"
if [[ -n "$NORMALIZED_ACCEPTED_UNFIXED_CVES" ]]; then
  IFS="," read -r -a accepted_unfixed_cves <<< "$NORMALIZED_ACCEPTED_UNFIXED_CVES"
  for cve_id in "${accepted_unfixed_cves[@]}"; do
    if [[ ! "$cve_id" =~ ^CVE-[0-9]{4}-[0-9]+$ ]]; then
      echo "MOVA_ACCEPT_UNFIXED_CVES contains an invalid CVE identifier: $cve_id" >&2
      exit 2
    fi
  done
fi

IMAGE_REPOSITORY="${IMAGE_TAG%:*}"
IMAGE_VERSION="${IMAGE_TAG##*:}"
SOURCE_REVISION="$(git rev-parse HEAD 2>/dev/null || true)"
if [[ ! "$SOURCE_REVISION" =~ ^[0-9a-f]{40}$ ]]; then
  echo "A Git checkout with a resolvable HEAD is required to publish Mova images." >&2
  exit 2
fi
SOURCE_REVISION_SHORT="${SOURCE_REVISION:0:12}"

SEMVER_PATTERN='^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z]+([.-][0-9A-Za-z]+)*)?$'
if [[ "$ALLOW_UNRELEASED" == "0" ]]; then
  if [[ ! "$IMAGE_VERSION" =~ $SEMVER_PATTERN ]]; then
    echo "Release image tags must be SemVer; use MOVA_ALLOW_UNRELEASED=1 only for deliberate development publishes." >&2
    exit 2
  fi
  if [[ "$BUILD_VERSION" != "$IMAGE_VERSION" ]]; then
    echo "MOVA_BUILD_VERSION must match the release image tag: $IMAGE_VERSION" >&2
    exit 2
  fi
  if [[ -n "$(git status --porcelain --untracked-files=normal)" ]]; then
    echo "Formal releases require a clean Git worktree and index." >&2
    exit 2
  fi

  RELEASE_GIT_TAG="v${BUILD_VERSION}"
  if [[ "$(git cat-file -t "refs/tags/${RELEASE_GIT_TAG}" 2>/dev/null || true)" != "tag" ]]; then
    echo "Formal releases require the annotated Git tag ${RELEASE_GIT_TAG}." >&2
    exit 2
  fi
  RELEASE_TAG_REVISION="$(git rev-parse "${RELEASE_GIT_TAG}^{commit}")"
  if [[ "$RELEASE_TAG_REVISION" != "$SOURCE_REVISION" ]]; then
    echo "Git tag ${RELEASE_GIT_TAG} does not point to HEAD (${SOURCE_REVISION})." >&2
    exit 2
  fi
else
  echo "MOVA_ALLOW_UNRELEASED=1: publishing an unreleased or dirty development build." >&2
fi

CANDIDATE_VERSION="${BUILD_VERSION//[^a-zA-Z0-9_.-]/-}"
CANDIDATE_VERSION="${CANDIDATE_VERSION:0:48}"
CANDIDATE_RUN_ID="${GITHUB_RUN_ID:-$(date -u +%Y%m%d%H%M%S)}"
CANDIDATE_RUN_ID="${CANDIDATE_RUN_ID//[^a-zA-Z0-9_.-]/-}"
CANDIDATE_RUN_ID="${CANDIDATE_RUN_ID:0:32}"
CANDIDATE_RUN_ATTEMPT="${GITHUB_RUN_ATTEMPT:-1}"
CANDIDATE_RUN_ATTEMPT="${CANDIDATE_RUN_ATTEMPT//[^a-zA-Z0-9_.-]/-}"
CANDIDATE_RUN_ATTEMPT="${CANDIDATE_RUN_ATTEMPT:0:8}"
CANDIDATE_NONCE="$(LC_ALL=C od -An -N4 -tx1 /dev/urandom | tr -d '[:space:]')"
CANDIDATE_TAG="${MOVA_DOCKER_CANDIDATE_TAG:-${IMAGE_REPOSITORY}:publish-${CANDIDATE_VERSION}-${SOURCE_REVISION_SHORT}-${CANDIDATE_RUN_ID}-${CANDIDATE_RUN_ATTEMPT}-${CANDIDATE_NONCE}}"
if [[ "$CANDIDATE_TAG" == "$IMAGE_TAG" ]]; then
  echo "The candidate tag must differ from the release tag." >&2
  exit 2
fi
if [[ "$CANDIDATE_TAG" == *@* || "$CANDIDATE_TAG" != *:* || -z "${CANDIDATE_TAG##*:}" || "${CANDIDATE_TAG##*:}" == */* ]]; then
  echo "MOVA_DOCKER_CANDIDATE_TAG must include an explicit tag: $CANDIDATE_TAG" >&2
  exit 2
fi
CANDIDATE_REPOSITORY="${CANDIDATE_TAG%:*}"

if ! docker scout version >/dev/null 2>&1; then
  echo "Docker Scout is required to publish Mova images." >&2
  exit 1
fi

BUILD_ARGS=()
for arg_name in HTTP_PROXY HTTPS_PROXY NO_PROXY ALL_PROXY; do
  arg_value="${!arg_name:-}"
  if [[ -n "$arg_value" ]]; then
    BUILD_ARGS+=(--build-arg "$arg_name=$arg_value")
  fi
done

build_and_push() {
  local dockerfile="$1"
  local tag="$2"
  local include_build_version="${3:-false}"
  local cache_policy="${4:-pull}"
  shift 4
  local extra_build_args=("$@")

  local build_command=(
    docker buildx build
    --platform "$PLATFORMS"
    -f "$dockerfile"
    -t "$tag"
    --push
    --provenance=mode=max
    --sbom=true
  )

  case "$cache_policy" in
    pull)
      build_command+=(--pull)
      ;;
    refresh)
      build_command+=(--pull --no-cache)
      ;;
    *)
      echo "Unknown Docker build cache policy: $cache_policy" >&2
      return 2
      ;;
  esac

  if ((${#BUILD_ARGS[@]} > 0)); then
    build_command+=("${BUILD_ARGS[@]}")
  fi

  if [[ "$include_build_version" == "true" ]]; then
    build_command+=(
      --build-arg "MOVA_BUILD_VERSION=$BUILD_VERSION"
      --build-arg "MOVA_BUILD_REVISION=$SOURCE_REVISION"
    )
  fi

  if ((${#extra_build_args[@]} > 0)); then
    build_command+=("${extra_build_args[@]}")
  fi

  build_command+=(.)
  "${build_command[@]}"
}

WEB_BUILD_BASE_TAG="richeschiu/mova-web-build-base:node24-pnpm11"
RUST_BUILD_BASE_TAG="richeschiu/mova-rust-build-base:1-bookworm"
RUNTIME_BASE_TAG="richeschiu/mova-runtime-base:trixie-ffmpeg-f944afd"
base_images=(
  "docker/base/web-build.Dockerfile|$WEB_BUILD_BASE_TAG"
  "docker/base/rust-build.Dockerfile|$RUST_BUILD_BASE_TAG"
  "docker/base/runtime.Dockerfile|$RUNTIME_BASE_TAG"
)

image_has_required_platforms() {
  local tag="$1"
  local inspect_output

  if ! inspect_output="$(docker buildx imagetools inspect "$tag" 2>/dev/null)"; then
    return 1
  fi

  IFS="," read -r -a required_platforms <<< "$PLATFORMS"
  for platform in "${required_platforms[@]}"; do
    platform="${platform//[[:space:]]/}"
    if [[ -z "$platform" ]]; then
      continue
    fi

    if [[ "$inspect_output" != *"Platform:    $platform"* && "$inspect_output" != *"Platform: $platform"* ]]; then
      return 1
    fi
  done
}

should_publish_base_image() {
  local tag="$1"
  local has_required_platforms=0
  local action

  # Forced refresh and disabled publication are intentional overrides. Avoid a
  # registry lookup in both cases; only auto mode needs platform availability.
  if [[ "$PUBLISH_BASE_IMAGES" == "auto" ]] && image_has_required_platforms "$tag"; then
    has_required_platforms=1
  fi

  action="$(resolve_base_image_action "$PUBLISH_BASE_IMAGES" "$has_required_platforms")"
  case "$action" in
    refresh)
      if [[ "$PUBLISH_BASE_IMAGES" == "auto" ]]; then
        echo "Base image is missing required platform(s), refreshing: $tag"
      else
        echo "Forcing a clean base image refresh: $tag"
      fi
      return 0
      ;;
    reuse)
      if [[ "$PUBLISH_BASE_IMAGES" == "auto" ]]; then
        echo "Base image already includes required platform(s), reusing: $tag"
      else
        echo "Base image publication is disabled, reusing the configured reference: $tag"
      fi
      return 1
      ;;
  esac

  echo "Unknown base image action: $action" >&2
  exit 2
}

run_scout_cves() {
  local image_ref="$1"
  shift

  if (( ${#SCOUT_VEX_ARGS[@]} > 0 )); then
    docker scout cves "$@" "${SCOUT_VEX_ARGS[@]}" "registry://$image_ref"
  else
    docker scout cves "$@" "registry://$image_ref"
  fi
}

verify_image_vulnerabilities() {
  local image_ref="$1"
  local platform
  local unfixed_status
  local report_file
  local observed_unfixed_cves
  local unaccepted_unfixed_cves

  IFS="," read -r -a scan_platforms <<< "$PLATFORMS"
  for platform in "${scan_platforms[@]}"; do
    platform="${platform//[[:space:]]/}"
    if [[ -z "$platform" ]]; then
      continue
    fi

    report_file="$(mktemp)"
    echo "Reporting unfixed critical and high vulnerabilities on $platform"
    unfixed_status=0
    run_scout_cves "$image_ref" \
      --platform "$platform" \
      --only-severity critical,high \
      --only-unfixed \
      --exit-code 2>&1 | tee "$report_file" || unfixed_status="${PIPESTATUS[0]}"

    case "$unfixed_status" in
      0|2)
        ;;
      *)
        rm -f "$report_file"
        echo "Docker Scout failed while reporting unfixed vulnerabilities on $platform." >&2
        return "$unfixed_status"
        ;;
    esac

    echo "Blocking fixable critical and high vulnerabilities on $platform"
    run_scout_cves "$image_ref" \
      --platform "$platform" \
      --only-severity critical,high \
      --only-fixed \
      --exit-code

    echo "Blocking vulnerabilities listed in the CISA Known Exploited Vulnerabilities catalog on $platform"
    run_scout_cves "$image_ref" \
      --platform "$platform" \
      --only-cisa-kev \
      --exit-code

    if [[ "$unfixed_status" == "2" ]]; then
      observed_unfixed_cves="$(grep -Eo 'CVE-[0-9]{4}-[0-9]+' "$report_file" | LC_ALL=C sort -u | paste -sd, -)"
      if [[ -z "$observed_unfixed_cves" ]]; then
        rm -f "$report_file"
        echo "Docker Scout reported unfixed findings but their CVE identifiers could not be extracted." >&2
        return 1
      fi

      unaccepted_unfixed_cves=""
      IFS="," read -r -a observed_cves <<< "$observed_unfixed_cves"
      for cve_id in "${observed_cves[@]}"; do
        case ",${NORMALIZED_ACCEPTED_UNFIXED_CVES}," in
          *",${cve_id},"*)
            ;;
          *)
            if [[ -n "$unaccepted_unfixed_cves" ]]; then
              unaccepted_unfixed_cves+=","
            fi
            unaccepted_unfixed_cves+="$cve_id"
            ;;
        esac
      done

      if [[ -n "$unaccepted_unfixed_cves" ]]; then
        rm -f "$report_file"
        echo "Unfixed critical or high vulnerabilities require an explicit release risk review." >&2
        echo "Unaccepted findings on $platform: $unaccepted_unfixed_cves" >&2
        echo "Use reviewed VEX statements for proven non-affected findings, or list only reviewed" >&2
        echo "residual findings in MOVA_ACCEPT_UNFIXED_CVES." >&2
        return 1
      fi
      echo "Continuing with the explicitly reviewed unfixed CVEs on $platform: $observed_unfixed_cves" >&2
    fi
    rm -f "$report_file"
  done
}

resolve_manifest_digest() {
  local image_ref="$1"
  local digest

  digest="$(docker buildx imagetools inspect "$image_ref" --format '{{.Manifest.Digest}}')"
  if [[ ! "$digest" =~ ^sha256:[0-9a-f]{64}$ ]]; then
    echo "Could not resolve a valid manifest digest for $image_ref: $digest" >&2
    return 1
  fi
  printf '%s\n' "$digest"
}

if [[ -n "$VERIFY_IMAGE_REF" ]]; then
  echo "Reverifying existing immutable image: $VERIFY_IMAGE_REF"
  docker buildx imagetools inspect "$VERIFY_IMAGE_REF"
  validate_attested_image_index "$VERIFY_IMAGE_REF" "$PLATFORMS"
  "$SMOKE_TEST_SCRIPT" "$VERIFY_IMAGE_REF" "$PLATFORMS"
  verify_image_vulnerabilities "$VERIFY_IMAGE_REF"
  echo "Existing immutable image passed runtime and security verification: $VERIFY_IMAGE_REF"
  exit 0
fi

for image in "${base_images[@]}"; do
  IFS="|" read -r dockerfile tag <<< "$image"
  if should_publish_base_image "$tag"; then
    build_and_push "$dockerfile" "$tag" false refresh
  fi
done

WEB_BUILD_BASE_DIGEST="$(resolve_manifest_digest "$WEB_BUILD_BASE_TAG")"
RUST_BUILD_BASE_DIGEST="$(resolve_manifest_digest "$RUST_BUILD_BASE_TAG")"
RUNTIME_BASE_DIGEST="$(resolve_manifest_digest "$RUNTIME_BASE_TAG")"
WEB_BUILD_BASE_REF="${WEB_BUILD_BASE_TAG%:*}@${WEB_BUILD_BASE_DIGEST}"
RUST_BUILD_BASE_REF="${RUST_BUILD_BASE_TAG%:*}@${RUST_BUILD_BASE_DIGEST}"
RUNTIME_BASE_REF="${RUNTIME_BASE_TAG%:*}@${RUNTIME_BASE_DIGEST}"
echo "Pinned Web build base for the application build: $WEB_BUILD_BASE_REF"
echo "Pinned Rust build base for the application build: $RUST_BUILD_BASE_REF"
echo "Pinned runtime base for the application build: $RUNTIME_BASE_REF"

echo "Building an isolated release candidate: $CANDIDATE_TAG"
build_and_push \
  apps/mova-server/Dockerfile \
  "$CANDIDATE_TAG" \
  true \
  pull \
  --build-arg "MOVA_WEB_BUILD_BASE=$WEB_BUILD_BASE_REF" \
  --build-arg "MOVA_RUST_BUILD_BASE=$RUST_BUILD_BASE_REF" \
  --build-arg "MOVA_RUNTIME_BASE=$RUNTIME_BASE_REF"
CANDIDATE_DIGEST="$(resolve_manifest_digest "$CANDIDATE_TAG")"
CANDIDATE_REF="${CANDIDATE_REPOSITORY}@${CANDIDATE_DIGEST}"
echo "Pinned release candidate: $CANDIDATE_REF"
docker buildx imagetools inspect "$CANDIDATE_REF"

validate_attested_image_index "$CANDIDATE_REF" "$PLATFORMS"
"$SMOKE_TEST_SCRIPT" "$CANDIDATE_REF" "$PLATFORMS"
verify_image_vulnerabilities "$CANDIDATE_REF"

echo "Security gate passed; promoting the exact candidate manifest to $IMAGE_TAG"
docker buildx imagetools create --prefer-index=false --tag "$IMAGE_TAG" "$CANDIDATE_REF"
docker buildx imagetools inspect "$IMAGE_TAG"
PUBLISHED_DIGEST="$(resolve_manifest_digest "$IMAGE_TAG")"
if [[ "$PUBLISHED_DIGEST" != "$CANDIDATE_DIGEST" ]]; then
  echo "Published digest mismatch: candidate=$CANDIDATE_DIGEST target=$PUBLISHED_DIGEST" >&2
  exit 1
fi
echo "Published $IMAGE_TAG at verified digest $PUBLISHED_DIGEST"
