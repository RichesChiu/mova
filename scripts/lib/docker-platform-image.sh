#!/usr/bin/env bash

# Resolve one platform image from a digest-pinned OCI index, or validate that a
# digest-pinned single-platform manifest matches the requested platform.
resolve_platform_image_ref() {
  local image_ref="$1"
  local platform="$2"
  local platform_os
  local platform_arch
  local platform_variant
  local media_type
  local format
  local platform_digest
  local image_repository
  local image_metadata

  if [[ ! "$platform" =~ ^[a-z0-9][a-z0-9._-]*/[a-z0-9][a-z0-9._-]*(/[a-z0-9][a-z0-9._-]*)?$ ]]; then
    echo "Invalid Docker platform: $platform" >&2
    return 2
  fi

  # A local, single-platform CI tag does not have a registry manifest to inspect.
  # Published candidates and releases must always be pinned by digest.
  if [[ "$image_ref" != *@sha256:* ]]; then
    printf '%s\n' "$image_ref"
    return 0
  fi

  IFS="/" read -r platform_os platform_arch platform_variant <<< "$platform"
  media_type="$(
    docker buildx imagetools inspect "$image_ref" --format '{{.Manifest.MediaType}}'
  )"

  case "$media_type" in
    application/vnd.oci.image.manifest.v1+json|application/vnd.docker.distribution.manifest.v2+json)
      image_metadata="$(
        docker buildx imagetools inspect "$image_ref" --format '{{json .Image}}'
      )"
      if ! jq -e \
        --arg os "$platform_os" \
        --arg architecture "$platform_arch" \
        --arg variant "$platform_variant" \
        '.os == $os and
         .architecture == $architecture and
         ($variant == "" or (.variant // "") == $variant)' \
        <<<"$image_metadata" >/dev/null; then
        echo "$image_ref does not contain the requested platform $platform." >&2
        return 1
      fi
      printf '%s\n' "$image_ref"
      return 0
      ;;
    application/vnd.oci.image.index.v1+json|application/vnd.docker.distribution.manifest.list.v2+json)
      ;;
    *)
      echo "Unsupported Docker manifest media type for $image_ref: $media_type" >&2
      return 1
      ;;
  esac

  if [[ -n "$platform_variant" ]]; then
    format="{{range .Manifest.Manifests}}{{if and (eq .Platform.OS \"$platform_os\") (eq .Platform.Architecture \"$platform_arch\") (eq .Platform.Variant \"$platform_variant\")}}{{.Digest}}{{end}}{{end}}"
  else
    format="{{range .Manifest.Manifests}}{{if and (eq .Platform.OS \"$platform_os\") (eq .Platform.Architecture \"$platform_arch\")}}{{.Digest}}{{end}}{{end}}"
  fi

  platform_digest="$(docker buildx imagetools inspect "$image_ref" --format "$format")"
  if [[ ! "$platform_digest" =~ ^sha256:[0-9a-f]{64}$ ]]; then
    echo "Could not resolve a manifest digest for $image_ref on $platform." >&2
    return 1
  fi

  image_repository="${image_ref%@*}"
  printf '%s@%s\n' "$image_repository" "$platform_digest"
}
