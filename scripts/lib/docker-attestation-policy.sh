#!/usr/bin/env bash

# Validate that a digest-pinned OCI index contains exactly the expected image
# platforms and that every image manifest has at least one attached attestation
# manifest. The attestation manifest can contain both provenance and SBOM blobs.
validate_attested_image_index() {
  local image_ref="$1"
  local platforms="$2"
  local media_type
  local manifest_metadata
  local expected_platforms_json
  local platform
  local -a expected_platforms=()
  local -a requested_platforms=()

  if [[ ! "$image_ref" =~ ^.+@sha256:[0-9a-f]{64}$ ]]; then
    echo "Attestation validation requires a digest-pinned image reference: $image_ref" >&2
    return 2
  fi

  IFS="," read -r -a requested_platforms <<<"$platforms"
  for platform in "${requested_platforms[@]}"; do
    platform="${platform//[[:space:]]/}"
    if [[ ! "$platform" =~ ^[a-z0-9][a-z0-9._-]*/[a-z0-9][a-z0-9._-]*$ ]]; then
      echo "Invalid Docker platform for attestation validation: $platform" >&2
      return 2
    fi
    expected_platforms+=("$platform")
  done
  expected_platforms_json="$(
    printf '%s\n' "${expected_platforms[@]}" \
      | jq -Rsc 'split("\n") | map(select(length > 0)) | sort'
  )"
  if ! jq -e 'length > 0 and length == (unique | length)' \
    <<<"$expected_platforms_json" >/dev/null; then
    echo "Attestation validation requires one or more unique Docker platforms." >&2
    return 2
  fi

  media_type="$(
    docker buildx imagetools inspect "$image_ref" --format '{{.Manifest.MediaType}}'
  )"
  case "$media_type" in
    application/vnd.oci.image.index.v1+json|application/vnd.docker.distribution.manifest.list.v2+json)
      ;;
    *)
      echo "Attested image must be an OCI index or Docker manifest list: $image_ref ($media_type)" >&2
      return 1
      ;;
  esac

  manifest_metadata="$(
    docker buildx imagetools inspect "$image_ref" --format '{{json .Manifest}}'
  )"
  if ! jq -e --argjson expected "$expected_platforms_json" '
    .manifests as $manifests |
    ([$manifests[] |
      select(.platform.os != "unknown" or .platform.architecture != "unknown") |
      {
        platform: (.platform.os + "/" + .platform.architecture),
        digest: .digest
      }]) as $images |
    ($images | map(.platform) | sort) == $expected and
    ($expected | all(. as $platform |
      ([$images[] | select(.platform == $platform)] | length) == 1)) and
    ($images | all(. as $image |
      any($manifests[];
        .platform.os == "unknown" and
        .platform.architecture == "unknown" and
        .annotations["vnd.docker.reference.type"] == "attestation-manifest" and
        .annotations["vnd.docker.reference.digest"] == $image.digest))) and
    ($manifests | all(. as $manifest |
      (($images | map(.digest) | index($manifest.digest)) != null) or
      (.platform.os == "unknown" and
       .platform.architecture == "unknown" and
       .annotations["vnd.docker.reference.type"] == "attestation-manifest" and
       (($images | map(.digest) |
         index($manifest.annotations["vnd.docker.reference.digest"])) != null))))
  ' <<<"$manifest_metadata" >/dev/null; then
    echo "Image platform set or attestation references are incomplete or invalid: $image_ref" >&2
    return 1
  fi
}
