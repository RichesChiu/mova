#!/usr/bin/env bash

mova_semver_compare() {
  if [[ "$#" -ne 2 ]]; then
    return 2
  fi

  local LC_ALL=C
  local left="$1"
  local right="$2"
  local pattern='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-([0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*))?$'
  if [[ ! "$left" =~ $pattern ]]; then
    return 2
  fi
  local left_major="${BASH_REMATCH[1]}"
  local left_minor="${BASH_REMATCH[2]}"
  local left_patch="${BASH_REMATCH[3]}"
  local left_prerelease="${BASH_REMATCH[5]:-}"
  if [[ ! "$right" =~ $pattern ]]; then
    return 2
  fi
  local right_major="${BASH_REMATCH[1]}"
  local right_minor="${BASH_REMATCH[2]}"
  local right_patch="${BASH_REMATCH[3]}"
  local right_prerelease="${BASH_REMATCH[5]:-}"
  local identifier
  local -a left_identifiers=() right_identifiers=()
  if [[ -n "$left_prerelease" ]]; then
    IFS=. read -r -a left_identifiers <<<"$left_prerelease"
  fi
  if [[ -n "$right_prerelease" ]]; then
    IFS=. read -r -a right_identifiers <<<"$right_prerelease"
  fi
  local -a all_identifiers=("${left_identifiers[@]-}" "${right_identifiers[@]-}")
  for identifier in "${all_identifiers[@]}"; do
    [[ -n "$identifier" ]] || continue
    if [[ "$identifier" =~ ^[0-9]+$ && ! "$identifier" =~ ^(0|[1-9][0-9]*)$ ]]; then
      return 2
    fi
  done
  local left_value right_value
  local -a left_core=("$left_major" "$left_minor" "$left_patch")
  local -a right_core=("$right_major" "$right_minor" "$right_patch")

  for index in 0 1 2; do
    left_value="${left_core[index]}"
    right_value="${right_core[index]}"
    if ((${#left_value} < ${#right_value})); then
      printf '%s\n' -1
      return 0
    fi
    if ((${#left_value} > ${#right_value})); then
      printf '%s\n' 1
      return 0
    fi
    if [[ "$left_value" < "$right_value" ]]; then
      printf '%s\n' -1
      return 0
    fi
    if [[ "$left_value" > "$right_value" ]]; then
      printf '%s\n' 1
      return 0
    fi
  done

  if [[ -z "$left_prerelease" && -z "$right_prerelease" ]]; then
    printf '%s\n' 0
    return 0
  fi
  if [[ -z "$left_prerelease" ]]; then
    printf '%s\n' 1
    return 0
  fi
  if [[ -z "$right_prerelease" ]]; then
    printf '%s\n' -1
    return 0
  fi

  local index=0
  local maximum="${#left_identifiers[@]}"
  if ((${#right_identifiers[@]} > maximum)); then
    maximum="${#right_identifiers[@]}"
  fi
  while ((index < maximum)); do
    if ((index >= ${#left_identifiers[@]})); then
      printf '%s\n' -1
      return 0
    fi
    if ((index >= ${#right_identifiers[@]})); then
      printf '%s\n' 1
      return 0
    fi
    left_value="${left_identifiers[index]}"
    right_value="${right_identifiers[index]}"
    if [[ "$left_value" == "$right_value" ]]; then
      ((index += 1))
      continue
    fi
    if [[ "$left_value" =~ ^(0|[1-9][0-9]*)$ && "$right_value" =~ ^(0|[1-9][0-9]*)$ ]]; then
      if ((${#left_value} < ${#right_value})) ||
        { ((${#left_value} == ${#right_value})) && [[ "$left_value" < "$right_value" ]]; }; then
        printf '%s\n' -1
      else
        printf '%s\n' 1
      fi
      return 0
    fi
    if [[ "$left_value" =~ ^(0|[1-9][0-9]*)$ ]]; then
      printf '%s\n' -1
      return 0
    fi
    if [[ "$right_value" =~ ^(0|[1-9][0-9]*)$ ]]; then
      printf '%s\n' 1
      return 0
    fi
    if [[ "$left_value" < "$right_value" ]]; then
      printf '%s\n' -1
    else
      printf '%s\n' 1
    fi
    return 0
  done

  printf '%s\n' 0
}
