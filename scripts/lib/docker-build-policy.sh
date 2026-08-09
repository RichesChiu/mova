#!/usr/bin/env bash

# Resolve whether a reusable Docker base image should be rebuilt.
#
# Arguments:
#   $1: publication mode (auto, 1/true/yes, or 0/false/no)
#   $2: whether the image contains every required platform (1 or 0)
#
# Output:
#   reuse or refresh
resolve_base_image_action() {
  local publish_mode="${1:-}"
  local has_required_platforms="${2:-}"

  case "$publish_mode" in
    1|true|yes)
      printf 'refresh\n'
      ;;
    0|false|no)
      printf 'reuse\n'
      ;;
    auto)
      case "$has_required_platforms" in
        1)
          printf 'reuse\n'
          ;;
        0)
          printf 'refresh\n'
          ;;
        *)
          echo "Base image platform availability must be 0 or 1." >&2
          return 2
          ;;
      esac
      ;;
    *)
      echo "Invalid MOVA_PUBLISH_BASE_IMAGES value: $publish_mode" >&2
      echo "Use auto, 1, true, yes, 0, false, or no." >&2
      return 2
      ;;
  esac
}
