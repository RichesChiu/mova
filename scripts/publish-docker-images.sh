#!/usr/bin/env bash
set -euo pipefail

PLATFORMS="${MOVA_DOCKER_PLATFORMS:-linux/amd64,linux/arm64}"
IMAGE_TAG="${MOVA_DOCKER_IMAGE_TAG:-richeschiu/mova:latest}"
PUBLISH_BASE_IMAGES="${MOVA_PUBLISH_BASE_IMAGES:-auto}"
ALLOW_UNRELEASED="${MOVA_ALLOW_UNRELEASED:-0}"
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

  local build_command=(
    docker buildx build
    --platform "$PLATFORMS"
    -f "$dockerfile"
    -t "$tag"
    --push
  )

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
  "docker/base/web-build.Dockerfile|richeschiu/mova-web-build-base:node24-pnpm11"
  "docker/base/rust-build.Dockerfile|richeschiu/mova-rust-build-base:1-bookworm"
  "docker/base/runtime.Dockerfile|richeschiu/mova-runtime-base:trixie-ffmpeg7-python3"
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

should_publish_base_images() {
  case "$PUBLISH_BASE_IMAGES" in
    1|true|yes)
      return 0
      ;;
    0|false|no)
      return 1
      ;;
    auto)
      for image in "${base_images[@]}"; do
        local tag="${image#*|}"
        if ! image_has_required_platforms "$tag"; then
          echo "Base image missing required platform(s), publishing base images: $tag"
          return 0
        fi
      done

      echo "Base images already include required platform(s): $PLATFORMS"
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

  IFS="," read -r -a scan_platforms <<< "$PLATFORMS"
  for platform in "${scan_platforms[@]}"; do
    platform="${platform//[[:space:]]/}"
    if [[ -z "$platform" ]]; then
      continue
    fi

    echo "Scanning $image_ref for critical and high vulnerabilities on $platform"
    docker scout cves \
      --platform "$platform" \
      --only-severity critical,high \
      --exit-code \
      "registry://$image_ref"
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

if should_publish_base_images; then
  for image in "${base_images[@]}"; do
    dockerfile="${image%%|*}"
    tag="${image#*|}"
    build_and_push "$dockerfile" "$tag"
  done
fi

echo "Building an isolated release candidate: $CANDIDATE_TAG"
build_and_push apps/mova-server/Dockerfile "$CANDIDATE_TAG" true
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
