#!/usr/bin/env bash

mova_release_subject_matches() {
  if [[ "$#" -ne 2 || -z "$2" ]]; then
    return 2
  fi

  local subject="$1"
  local expected_subject="$2"
  local suffix

  if [[ "$subject" == "$expected_subject" ]]; then
    return 0
  fi
  if [[ "$subject" != "$expected_subject"* ]]; then
    return 1
  fi

  suffix="${subject#"$expected_subject"}"
  [[ "$suffix" =~ ^\ \(#[1-9][0-9]*\)$ ]]
}
