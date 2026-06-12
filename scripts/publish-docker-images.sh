#!/usr/bin/env bash
set -euo pipefail

PLATFORMS="${MOVA_DOCKER_PLATFORMS:-linux/amd64,linux/arm64}"
IMAGE_TAG="${MOVA_DOCKER_IMAGE_TAG:-richeschiu/mova:latest}"
PUBLISH_BASE_IMAGES="${MOVA_PUBLISH_BASE_IMAGES:-auto}"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

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

  build_command+=(.)
  "${build_command[@]}"
}

base_images=(
  "docker/base/web-build.Dockerfile|richeschiu/mova-web-build-base:node24-pnpm11"
  "docker/base/rust-build.Dockerfile|richeschiu/mova-rust-build-base:1-bookworm"
  "docker/base/runtime.Dockerfile|richeschiu/mova-runtime-base:bookworm-ffmpeg-python3"
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

if should_publish_base_images; then
  for image in "${base_images[@]}"; do
    dockerfile="${image%%|*}"
    tag="${image#*|}"
    build_and_push "$dockerfile" "$tag"
  done
fi

build_and_push apps/mova-server/Dockerfile "$IMAGE_TAG"
docker buildx imagetools inspect "$IMAGE_TAG"
