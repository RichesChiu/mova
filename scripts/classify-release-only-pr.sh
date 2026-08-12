#!/usr/bin/env bash

set -euo pipefail

# Prints exactly `true` only when a pull request is a mechanical workspace
# release-version change. Every malformed input, unexpected diff, or parser
# ambiguity prints `false`, so callers can safely fall back to the full CI
# suite. Whether the pull request originates from this repository must be
# checked by the workflow before invoking this helper.

readonly SCRIPT_DIRECTORY="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/release-subject.sh
source "$SCRIPT_DIRECTORY/lib/release-subject.sh"
# shellcheck source=scripts/lib/semver.sh
source "$SCRIPT_DIRECTORY/lib/semver.sh"

readonly REPOSITORY_ROOT="${MOVA_REPOSITORY_ROOT:-$(cd "$SCRIPT_DIRECTORY/.." && pwd)}"
readonly BASE_SHA="${MOVA_BASE_SHA:-}"
readonly HEAD_SHA="${MOVA_HEAD_SHA:-${GITHUB_SHA:-}}"
readonly PR_TITLE="${MOVA_PR_TITLE:-}"

reject_release_only() {
  printf 'Release-only classification rejected: %s\n' "$1" >&2
  printf 'false\n'
  exit 0
}

require_command() {
  command -v "$1" >/dev/null 2>&1 \
    || reject_release_only "required command is unavailable: $1"
}

require_command awk
require_command cmp
require_command git
require_command mktemp

[[ -d "$REPOSITORY_ROOT" ]] || reject_release_only "repository root is unavailable"
[[ "$BASE_SHA" =~ ^[0-9a-f]{40}$ ]] || reject_release_only "base revision is not a full commit SHA"
[[ "$HEAD_SHA" =~ ^[0-9a-f]{40}$ ]] || reject_release_only "head revision is not a full commit SHA"
[[ "$BASE_SHA" != "$HEAD_SHA" ]] || reject_release_only "base and head revisions are identical"
git -C "$REPOSITORY_ROOT" cat-file -e "${BASE_SHA}^{commit}" 2>/dev/null \
  || reject_release_only "base revision is unavailable"
git -C "$REPOSITORY_ROOT" cat-file -e "${HEAD_SHA}^{commit}" 2>/dev/null \
  || reject_release_only "head revision is unavailable"
git -C "$REPOSITORY_ROOT" merge-base --is-ancestor "$BASE_SHA" "$HEAD_SHA" 2>/dev/null \
  || reject_release_only "base revision is not an ancestor of head"

TEMPORARY_DIRECTORY="$(mktemp -d "${TMPDIR:-/tmp}/mova-release-only.XXXXXX")" \
  || reject_release_only "temporary directory could not be created"
trap 'rm -rf "$TEMPORARY_DIRECTORY"' EXIT

show_file() {
  local revision="$1"
  local path="$2"
  local destination="$3"

  git -C "$REPOSITORY_ROOT" show "${revision}:${path}" >"$destination" 2>/dev/null
}

require_regular_blob() {
  local revision="$1"
  local path="$2"
  local entry

  entry="$(git -C "$REPOSITORY_ROOT" ls-tree "$revision" -- "$path")" \
    || reject_release_only "could not inspect $path"
  [[ "$entry" =~ ^100644[[:space:]]blob[[:space:]][0-9a-f]{40}[[:space:]] ]] \
    || reject_release_only "$path must be a regular non-executable file"
}

extract_workspace_version() {
  awk '
    BEGIN {
      in_workspace_package = 0
      version_count = 0
      invalid = 0
    }
    /^\[workspace\.package\][[:space:]]*$/ {
      in_workspace_package = 1
      next
    }
    /^\[[^]]+\][[:space:]]*$/ {
      in_workspace_package = 0
    }
    in_workspace_package && /^[[:space:]]*version[[:space:]]*=/ {
      if ($0 !~ /^[[:space:]]*version[[:space:]]*=[[:space:]]*"[^"]+"[[:space:]]*(#.*)?$/) {
        invalid = 1
        next
      }
      value = $0
      sub(/^[^"]*"/, "", value)
      sub(/".*$/, "", value)
      version = value
      version_count++
    }
    END {
      if (invalid || version_count != 1) {
        exit 2
      }
      print version
    }
  ' "$1"
}

normalize_workspace_manifest() {
  awk '
    BEGIN {
      in_workspace_package = 0
      version_count = 0
      invalid = 0
    }
    /^\[workspace\.package\][[:space:]]*$/ {
      in_workspace_package = 1
      print
      next
    }
    /^\[[^]]+\][[:space:]]*$/ {
      in_workspace_package = 0
    }
    in_workspace_package && /^[[:space:]]*version[[:space:]]*=/ {
      if ($0 !~ /^[[:space:]]*version[[:space:]]*=[[:space:]]*"[^"]+"[[:space:]]*(#.*)?$/) {
        invalid = 1
        next
      }
      line = $0
      sub(/"[^"]+"/, "\"__MOVA_WORKSPACE_VERSION__\"", line)
      print line
      version_count++
      next
    }
    { print }
    END {
      if (invalid || version_count != 1) {
        exit 2
      }
    }
  ' "$1"
}

normalize_workspace_lock() {
  local input="$1"

  awk '
    function flush_package(    i) {
      if (!in_package) {
        return
      }
      if (package_name ~ /^mova-/) {
        mova_package_count++
        if (version_count != 1 || has_source) {
          invalid = 1
        }
      }
      for (i = 1; i <= package_line_count; i++) {
        if (package_name ~ /^mova-/ && package_lines[i] ~ /^version = /) {
          print "version = \"__MOVA_WORKSPACE_VERSION__\""
        } else {
          print package_lines[i]
        }
      }
      delete package_lines
      package_line_count = 0
    }
    BEGIN {
      in_package = 0
      package_name = ""
      package_version = ""
      seen_name = 0
      version_count = 0
      has_source = 0
      mova_package_count = 0
      invalid = 0
    }
    /^\[\[package\]\][[:space:]]*$/ {
      flush_package()
      in_package = 1
      package_name = ""
      package_version = ""
      seen_name = 0
      version_count = 0
      has_source = 0
      package_line_count = 1
      package_lines[package_line_count] = $0
      next
    }
    !in_package {
      print
      next
    }
    /^name = "[^"]+"[[:space:]]*$/ {
      package_line_count++
      package_lines[package_line_count] = $0
      if (seen_name) {
        invalid = 1
      }
      value = $0
      sub(/^name = "/, "", value)
      sub(/"[[:space:]]*$/, "", value)
      package_name = value
      seen_name = 1
      next
    }
    /^version = / {
      package_line_count++
      package_lines[package_line_count] = $0
      if ($0 !~ /^version = "[^"]+"[[:space:]]*$/ || version_count != 0) {
        invalid = 1
        next
      }
      value = $0
      sub(/^version = "/, "", value)
      sub(/"[[:space:]]*$/, "", value)
      package_version = value
      version_count++
      next
    }
    /^source = / {
      has_source = 1
      package_line_count++
      package_lines[package_line_count] = $0
      next
    }
    {
      package_line_count++
      package_lines[package_line_count] = $0
    }
    END {
      flush_package()
      if (invalid || mova_package_count == 0) {
        exit 2
      }
    }
  ' "$input"
}

validate_workspace_lock() {
  local expected_version="$1"
  local input="$2"

  awk -v expected_version="$expected_version" '
    function finish_package() {
      if (package_name ~ /^mova-/) {
        mova_package_count++
        if (version_count != 1 || package_version != expected_version || has_source) {
          invalid = 1
        }
      }
    }
    BEGIN {
      in_package = 0
      package_name = ""
      package_version = ""
      version_count = 0
      has_source = 0
      mova_package_count = 0
      invalid = 0
    }
    /^\[\[package\]\][[:space:]]*$/ {
      if (in_package) {
        finish_package()
      }
      in_package = 1
      package_name = ""
      package_version = ""
      version_count = 0
      has_source = 0
      next
    }
    !in_package { next }
    /^name = "[^"]+"[[:space:]]*$/ {
      value = $0
      sub(/^name = "/, "", value)
      sub(/"[[:space:]]*$/, "", value)
      package_name = value
      next
    }
    /^version = "[^"]+"[[:space:]]*$/ {
      value = $0
      sub(/^version = "/, "", value)
      sub(/"[[:space:]]*$/, "", value)
      package_version = value
      version_count++
      next
    }
    /^source = / { has_source = 1 }
    END {
      if (in_package) {
        finish_package()
      }
      if (invalid || mova_package_count == 0) {
        exit 2
      }
    }
  ' "$input" >/dev/null
}

BASE_MANIFEST="$TEMPORARY_DIRECTORY/base-Cargo.toml"
HEAD_MANIFEST="$TEMPORARY_DIRECTORY/head-Cargo.toml"
BASE_LOCK="$TEMPORARY_DIRECTORY/base-Cargo.lock"
HEAD_LOCK="$TEMPORARY_DIRECTORY/head-Cargo.lock"
show_file "$BASE_SHA" Cargo.toml "$BASE_MANIFEST" \
  || reject_release_only "base Cargo.toml is unavailable"
show_file "$HEAD_SHA" Cargo.toml "$HEAD_MANIFEST" \
  || reject_release_only "head Cargo.toml is unavailable"
show_file "$BASE_SHA" Cargo.lock "$BASE_LOCK" \
  || reject_release_only "base Cargo.lock is unavailable"
show_file "$HEAD_SHA" Cargo.lock "$HEAD_LOCK" \
  || reject_release_only "head Cargo.lock is unavailable"
require_regular_blob "$BASE_SHA" Cargo.toml
require_regular_blob "$HEAD_SHA" Cargo.toml
require_regular_blob "$BASE_SHA" Cargo.lock
require_regular_blob "$HEAD_SHA" Cargo.lock

BASE_VERSION="$(extract_workspace_version "$BASE_MANIFEST")" \
  || reject_release_only "base workspace version is ambiguous"
HEAD_VERSION="$(extract_workspace_version "$HEAD_MANIFEST")" \
  || reject_release_only "head workspace version is ambiguous"
readonly BASE_VERSION HEAD_VERSION
readonly SEMVER_PATTERN='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?(\+[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$'
[[ "$BASE_VERSION" =~ $SEMVER_PATTERN ]] || reject_release_only "base workspace version is invalid"
[[ "$HEAD_VERSION" =~ $SEMVER_PATTERN ]] || reject_release_only "head workspace version is invalid"
[[ "$BASE_VERSION" != "$HEAD_VERSION" ]] || reject_release_only "workspace version did not change"
[[ "$BASE_VERSION" != *+* && "$HEAD_VERSION" != *+* ]] \
  || reject_release_only "workspace release versions cannot contain build metadata"
readonly VERSION_ORDER="$(mova_semver_compare "$BASE_VERSION" "$HEAD_VERSION")" \
  || reject_release_only "workspace versions cannot be compared"
[[ "$VERSION_ORDER" == -1 ]] \
  || reject_release_only "workspace version must increase"

readonly EXPECTED_SUBJECT="chore(release): prepare ${HEAD_VERSION}"
mova_release_subject_matches "$PR_TITLE" "$EXPECTED_SUBJECT" \
  || reject_release_only "pull request title does not match $EXPECTED_SUBJECT"

DIFF_LIST="$TEMPORARY_DIRECTORY/diff-list"
git -C "$REPOSITORY_ROOT" diff --name-status --no-renames "$BASE_SHA" "$HEAD_SHA" \
  >"$DIFF_LIST" 2>/dev/null \
  || reject_release_only "release diff could not be read"

manifest_changed=false
lock_changed=false
while IFS=$'\t' read -r status path extra; do
  [[ -n "$status" && -n "$path" && -z "${extra:-}" ]] \
    || reject_release_only "release diff contains an ambiguous path"
  case "$path" in
    Cargo.toml)
      [[ "$status" == M && "$manifest_changed" == false ]] \
        || reject_release_only "Cargo.toml must be modified exactly once"
      manifest_changed=true
      ;;
    Cargo.lock)
      [[ "$status" == M && "$lock_changed" == false ]] \
        || reject_release_only "Cargo.lock must be modified exactly once"
      lock_changed=true
      ;;
    ".github/release-notes/${HEAD_VERSION}.md")
      [[ "$status" == A || "$status" == M ]] \
        || reject_release_only "release notes must be added or modified"
      require_regular_blob "$HEAD_SHA" "$path"
      ;;
    *)
      reject_release_only "unexpected changed path: $path"
      ;;
  esac
done <"$DIFF_LIST"
[[ "$manifest_changed" == true ]] || reject_release_only "Cargo.toml was not modified"
[[ "$lock_changed" == true ]] || reject_release_only "Cargo.lock was not modified"

normalize_workspace_manifest "$BASE_MANIFEST" \
  >"$TEMPORARY_DIRECTORY/base-manifest.normalized" \
  || reject_release_only "base Cargo.toml could not be normalized"
normalize_workspace_manifest "$HEAD_MANIFEST" \
  >"$TEMPORARY_DIRECTORY/head-manifest.normalized" \
  || reject_release_only "head Cargo.toml could not be normalized"
cmp -s \
  "$TEMPORARY_DIRECTORY/base-manifest.normalized" \
  "$TEMPORARY_DIRECTORY/head-manifest.normalized" \
  || reject_release_only "Cargo.toml contains changes beyond the workspace version"

normalize_workspace_lock "$BASE_LOCK" \
  >"$TEMPORARY_DIRECTORY/base-lock.normalized" \
  || reject_release_only "base Cargo.lock workspace packages are invalid"
normalize_workspace_lock "$HEAD_LOCK" \
  >"$TEMPORARY_DIRECTORY/head-lock.normalized" \
  || reject_release_only "head Cargo.lock workspace packages are invalid"
validate_workspace_lock "$BASE_VERSION" "$BASE_LOCK" \
  || reject_release_only "base Cargo.lock workspace versions do not match"
validate_workspace_lock "$HEAD_VERSION" "$HEAD_LOCK" \
  || reject_release_only "head Cargo.lock workspace versions do not match"
cmp -s \
  "$TEMPORARY_DIRECTORY/base-lock.normalized" \
  "$TEMPORARY_DIRECTORY/head-lock.normalized" \
  || reject_release_only "Cargo.lock contains changes beyond MOVA workspace versions"

printf 'Release-only change %s -> %s verified.\n' "$BASE_VERSION" "$HEAD_VERSION" >&2
printf 'true\n'
