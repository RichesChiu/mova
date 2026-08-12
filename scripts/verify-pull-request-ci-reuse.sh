#!/usr/bin/env bash

set -euo pipefail

# This helper is intentionally fail-closed. It never decides whether CI should
# pass; it only proves that the exact tree now on master was already exercised
# by a successful pull_request run of this repository's CI workflow. Callers
# must run the full master CI whenever stdout is `false`.
readonly REPOSITORY_ROOT="${MOVA_REPOSITORY_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
readonly SOURCE_SHA="${MOVA_SOURCE_SHA:-${GITHUB_SHA:-}}"
readonly REPOSITORY="${MOVA_GITHUB_REPOSITORY:-${GITHUB_REPOSITORY:-}}"
readonly TARGET_BRANCH="${MOVA_TARGET_BRANCH:-${GITHUB_REF_NAME:-master}}"
readonly GH_COMMAND="${MOVA_GH_COMMAND:-gh}"

reject_reuse() {
  printf 'PR CI cannot be reused: %s\n' "$1" >&2
  printf 'false\n'
  exit 0
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || reject_reuse "required command is unavailable: $1"
}

require_command git
require_command jq
if [[ "$GH_COMMAND" == */* ]]; then
  [[ -x "$GH_COMMAND" ]] || reject_reuse "configured GitHub client is not executable"
else
  require_command "$GH_COMMAND"
fi

[[ "$SOURCE_SHA" =~ ^[0-9a-f]{40}$ ]] || reject_reuse "source revision is not a full commit SHA"
[[ "$REPOSITORY" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] \
  || reject_reuse "GitHub repository is invalid"
[[ "$TARGET_BRANCH" =~ ^[A-Za-z0-9._/-]+$ ]] || reject_reuse "target branch is invalid"
git -C "$REPOSITORY_ROOT" cat-file -e "${SOURCE_SHA}^{commit}" 2>/dev/null \
  || reject_reuse "source revision is unavailable in the checkout"
[[ "$(git -C "$REPOSITORY_ROOT" rev-parse HEAD)" == "$SOURCE_SHA" ]] \
  || reject_reuse "checked-out HEAD differs from the source revision"

read -r -a source_line <<<"$(
  git -C "$REPOSITORY_ROOT" rev-list --parents -n 1 "$SOURCE_SHA"
)"
[[ "${#source_line[@]}" -eq 2 ]] \
  || reject_reuse "source revision must have exactly one parent"
readonly BASE_SHA="${source_line[1]}"

github_api() {
  "$GH_COMMAND" api \
    --method GET \
    -H 'Accept: application/vnd.github+json' \
    -H 'X-GitHub-Api-Version: 2022-11-28' \
    "$1"
}

pulls_json="$(
  github_api "repos/${REPOSITORY}/commits/${SOURCE_SHA}/pulls?per_page=100"
)" || reject_reuse "associated pull requests could not be queried"
if ! jq -e 'type == "array"' <<<"$pulls_json" >/dev/null 2>&1; then
  reject_reuse "associated pull request response is invalid"
fi

matching_pulls="$(
  jq -c \
    --arg source "$SOURCE_SHA" \
    --arg base "$BASE_SHA" \
    --arg branch "$TARGET_BRANCH" \
    '[.[] | select(
      .merged_at != null and
      .merge_commit_sha == $source and
      .base.sha == $base and
      .base.ref == $branch and
      (.created_at | type == "string") and
      (.merged_at | type == "string") and
      (.merge_commit_sha | type == "string") and
      (.head.sha | type == "string") and
      (.head.ref | type == "string")
    )]' <<<"$pulls_json"
)" || reject_reuse "associated pull request response could not be evaluated"
[[ "$(jq 'length' <<<"$matching_pulls")" -eq 1 ]] \
  || reject_reuse "exactly one merged pull request must match the source revision"

readonly PR_NUMBER="$(jq -r '.[0].number' <<<"$matching_pulls")"
readonly HEAD_SHA="$(jq -r '.[0].head.sha' <<<"$matching_pulls")"
readonly HEAD_BRANCH="$(jq -r '.[0].head.ref' <<<"$matching_pulls")"
readonly PR_CREATED_AT="$(jq -r '.[0].created_at' <<<"$matching_pulls")"
[[ "$PR_NUMBER" =~ ^[1-9][0-9]*$ ]] || reject_reuse "pull request number is invalid"
[[ "$HEAD_SHA" =~ ^[0-9a-f]{40}$ ]] || reject_reuse "pull request head SHA is invalid"

head_commit_json="$(
  github_api "repos/${REPOSITORY}/git/commits/${HEAD_SHA}"
)" || reject_reuse "pull request head tree could not be queried"
readonly HEAD_TREE="$(jq -r '.tree.sha // empty' <<<"$head_commit_json" 2>/dev/null || true)"
readonly SOURCE_TREE="$(git -C "$REPOSITORY_ROOT" rev-parse "${SOURCE_SHA}^{tree}")"
[[ "$HEAD_TREE" =~ ^[0-9a-f]{40}$ ]] || reject_reuse "pull request head tree is invalid"
[[ "$HEAD_TREE" == "$SOURCE_TREE" ]] \
  || reject_reuse "pull request head and master source have different trees"

runs_json="$(
  github_api \
    "repos/${REPOSITORY}/actions/workflows/ci.yml/runs?event=pull_request&head_sha=${HEAD_SHA}&per_page=100"
)" || reject_reuse "pull request CI runs could not be queried"
if ! jq -e '.workflow_runs | type == "array"' <<<"$runs_json" >/dev/null 2>&1; then
  reject_reuse "pull request CI run response is invalid"
fi

# The run endpoint is scoped to ci.yml and the exact immutable PR head. Still,
# the same branch head can participate in more than one pull request. Select
# the latest candidate first; its dynamically named provenance job below must
# bind that run to the exact PR/base/head tuple. A missing or different binding
# fails closed instead of falling back to an older green run.
matching_runs="$(
  jq -c \
    --arg head "$HEAD_SHA" \
    --arg branch "$HEAD_BRANCH" \
    --arg created_at "$PR_CREATED_AT" \
    '[.workflow_runs[] | select(
      .path == ".github/workflows/ci.yml" and
      .event == "pull_request" and
      .head_sha == $head and
      .head_branch == $branch and
      (.created_at | type == "string") and
      .created_at >= $created_at
    )]' <<<"$runs_json"
)" || reject_reuse "pull request CI runs could not be evaluated"
readonly RUN_ID="$(
  jq -r \
    'sort_by(.created_at, .id)
      | last
      | select(.status == "completed" and .conclusion == "success")
      | .id // empty
    ' <<<"$matching_runs"
)" || reject_reuse "the latest exact pull request CI run did not succeed"
[[ "$RUN_ID" =~ ^[1-9][0-9]*$ ]] \
  || reject_reuse "the latest exact pull request CI run did not succeed"

# Workflow-level success alone is insufficient because GitHub also reports a
# run as successful when a component job is intentionally skipped. Require the
# provenance and aggregate gate plus one of the two valid component shapes:
# the full suite, or the deliberately reduced release-only suite.
jobs_json="$(
  github_api "repos/${REPOSITORY}/actions/runs/${RUN_ID}/jobs?per_page=100"
)" || reject_reuse "pull request CI jobs could not be queried"
if ! jq -e '.jobs | type == "array"' <<<"$jobs_json" >/dev/null 2>&1; then
  reject_reuse "pull request CI job response is invalid"
fi
readonly PROVENANCE_JOB_NAME="Source provenance [PR #${PR_NUMBER} base ${BASE_SHA} head ${HEAD_SHA}]"
required_jobs_ok="$(
  jq -e --arg provenance_job "$PROVENANCE_JOB_NAME" '
    def exactly_one($name; $conclusion):
      [.jobs[] | select(
        .name == $name and
        .status == "completed" and
        .conclusion == $conclusion
      )] | length == 1;

    (exactly_one($provenance_job; "success") and
     exactly_one("CI gate"; "success")) and
    (
      (exactly_one("Web"; "success") and
       exactly_one("Website"; "success") and
       exactly_one("Rust and PostgreSQL"; "success") and
       exactly_one("Docker image"; "success"))
      or
      (exactly_one("Web"; "skipped") and
       exactly_one("Website"; "skipped") and
       exactly_one("Rust and PostgreSQL"; "success") and
       exactly_one("Docker image"; "success"))
    )
  ' <<<"$jobs_json"
)" || reject_reuse "required pull request CI jobs did not all succeed"
[[ "$required_jobs_ok" == "true" ]] \
  || reject_reuse "required pull request CI jobs did not all succeed"

printf 'PR #%s CI is reusable for master revision %s.\n' "$PR_NUMBER" "$SOURCE_SHA" >&2
printf 'true\n'
