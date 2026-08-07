#!/usr/bin/env bash
set -euo pipefail

PLATFORMS="${MOVA_DOCKER_PLATFORMS:-linux/amd64,linux/arm64}"
IMAGE_TAG="${MOVA_DOCKER_IMAGE_TAG:-richeschiu/mova:latest}"
PUBLISH_BASE_IMAGES="${MOVA_PUBLISH_BASE_IMAGES:-auto}"
ALLOW_UNRELEASED="${MOVA_ALLOW_UNRELEASED:-0}"
ACCEPT_UNFIXED_CVES="${MOVA_ACCEPT_UNFIXED_CVES:-}"
SCOUT_VEX_LOCATION="${MOVA_SCOUT_VEX_LOCATION:-}"
SCOUT_VEX_AUTHORS="${MOVA_SCOUT_VEX_AUTHORS:-}"
DEFAULT_BUILD_VERSION="development"
if [[ "$IMAGE_TAG" == *:* ]]; then
  DEFAULT_BUILD_VERSION="${IMAGE_TAG##*:}"
fi
BUILD_VERSION="${MOVA_BUILD_VERSION:-$DEFAULT_BUILD_VERSION}"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ "$IMAGE_TAG" == *@* || "$IMAGE_TAG" != *:* || -z "${IMAGE_TAG##*:}" || "${IMAGE_TAG##*:}" == */* ]]; then
  echo "MOVA_DOCKER_IMAGE_TAG must include an explicit tag: $IMAGE_TAG" >&2
  exit 2
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

  local build_command=(
    docker buildx build
    --platform "$PLATFORMS"
    -f "$dockerfile"
    -t "$tag"
    --push
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

  build_command+=(.)
  "${build_command[@]}"
}

base_images=(
  "docker/base/web-build.Dockerfile|richeschiu/mova-web-build-base:node24-pnpm11|missing"
  "docker/base/rust-build.Dockerfile|richeschiu/mova-rust-build-base:1-bookworm|missing"
  "docker/base/runtime.Dockerfile|richeschiu/mova-runtime-base:trixie-ffmpeg7-python3|release"
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
  local refresh_policy="$2"

  case "$PUBLISH_BASE_IMAGES" in
    1|true|yes)
      return 0
      ;;
    0|false|no)
      return 1
      ;;
    auto)
      if [[ "$ALLOW_UNRELEASED" == "0" && "$refresh_policy" == "release" ]]; then
        echo "Refreshing the runtime base image for the formal release: $tag"
        return 0
      fi
      if ! image_has_required_platforms "$tag"; then
        echo "Base image is missing required platform(s), publishing: $tag"
        return 0
      fi

      echo "Base image already includes required platform(s), reusing: $tag"
      return 1
      ;;
    *)
      echo "Invalid MOVA_PUBLISH_BASE_IMAGES value: $PUBLISH_BASE_IMAGES" >&2
      echo "Use auto, 1, true, yes, 0, false, or no." >&2
      exit 2
      ;;
  esac
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
    docker scout cves \
      --platform "$platform" \
      --only-severity critical,high \
      --only-unfixed \
      --exit-code \
      "${SCOUT_VEX_ARGS[@]}" \
      "registry://$image_ref" 2>&1 | tee "$report_file" || unfixed_status="${PIPESTATUS[0]}"

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
    docker scout cves \
      --platform "$platform" \
      --only-severity critical,high \
      --only-fixed \
      --exit-code \
      "${SCOUT_VEX_ARGS[@]}" \
      "registry://$image_ref"

    echo "Blocking vulnerabilities listed in the CISA Known Exploited Vulnerabilities catalog on $platform"
    docker scout cves \
      --platform "$platform" \
      --only-cisa-kev \
      --exit-code \
      "${SCOUT_VEX_ARGS[@]}" \
      "registry://$image_ref"

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

for image in "${base_images[@]}"; do
  IFS="|" read -r dockerfile tag refresh_policy <<< "$image"
  if should_publish_base_image "$tag" "$refresh_policy"; then
    cache_policy="pull"
    if [[ "$refresh_policy" == "release" ]]; then
      cache_policy="refresh"
    fi
    build_and_push "$dockerfile" "$tag" false "$cache_policy"
  fi
done

echo "Building an isolated release candidate: $CANDIDATE_TAG"
build_and_push apps/mova-server/Dockerfile "$CANDIDATE_TAG" true pull
CANDIDATE_DIGEST="$(resolve_manifest_digest "$CANDIDATE_TAG")"
CANDIDATE_REF="${CANDIDATE_REPOSITORY}@${CANDIDATE_DIGEST}"
echo "Pinned release candidate: $CANDIDATE_REF"
docker buildx imagetools inspect "$CANDIDATE_REF"

./scripts/smoke-test-runtime-image.sh "$CANDIDATE_REF" "$PLATFORMS"
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
