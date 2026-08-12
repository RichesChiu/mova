#!/usr/bin/env bash

set -euo pipefail

REPOSITORY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERIFIER="$REPOSITORY_ROOT/scripts/verify-pull-request-ci-reuse.sh"
TEST_DIRECTORY="$(mktemp -d)"
trap 'rm -rf "$TEST_DIRECTORY"' EXIT

TEST_REPOSITORY="$TEST_DIRECTORY/repository"
mkdir -p "$TEST_REPOSITORY/.github/workflows" "$TEST_REPOSITORY/scripts/lib"
git -C "$TEST_REPOSITORY" init --quiet
git -C "$TEST_REPOSITORY" config user.name test
git -C "$TEST_REPOSITORY" config user.email test@example.com
printf 'name: CI\n' >"$TEST_REPOSITORY/.github/workflows/ci.yml"
printf '# stable verifier boundary\n' >"$TEST_REPOSITORY/scripts/verify-pull-request-ci-reuse.sh"
printf '# stable classifier boundary\n' >"$TEST_REPOSITORY/scripts/classify-release-only-pr.sh"
printf '# stable subject boundary\n' >"$TEST_REPOSITORY/scripts/lib/release-subject.sh"
printf 'base\n' >"$TEST_REPOSITORY/content.txt"
git -C "$TEST_REPOSITORY" add .
git -C "$TEST_REPOSITORY" commit --quiet -m base
BASE_SHA="$(git -C "$TEST_REPOSITORY" rev-parse HEAD)"

printf 'feature\n' >"$TEST_REPOSITORY/content.txt"
git -C "$TEST_REPOSITORY" commit --quiet -am feature
HEAD_SHA="$(git -C "$TEST_REPOSITORY" rev-parse HEAD)"
HEAD_TREE="$(git -C "$TEST_REPOSITORY" rev-parse 'HEAD^{tree}')"
SOURCE_SHA="$(
  printf 'squash merge\n' \
    | git -C "$TEST_REPOSITORY" commit-tree "$HEAD_TREE" -p "$BASE_SHA"
)"

FAKE_GH="$TEST_DIRECTORY/gh"
cat >"$FAKE_GH" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

endpoint="${*: -1}"
case "$endpoint" in
  "repos/example/mova/commits/${FAKE_SOURCE_SHA}/pulls?per_page=100")
    if [[ "$FAKE_SCENARIO" == api_failure ]]; then
      exit 1
    fi
    merge_sha="$FAKE_SOURCE_SHA"
    base_sha="$BASE_SHA"
    merged_at="2026-08-12T01:00:00Z"
    if [[ "$FAKE_SCENARIO" == wrong_merge ]]; then
      merge_sha="0000000000000000000000000000000000000000"
    elif [[ "$FAKE_SCENARIO" == wrong_base ]]; then
      base_sha="0000000000000000000000000000000000000000"
    elif [[ "$FAKE_SCENARIO" == unmerged ]]; then
      merged_at=""
    fi
    jq -n \
      --arg merge "$merge_sha" \
      --arg base "$base_sha" \
      --arg head "$FAKE_HEAD_SHA" \
      --arg merged_at "$merged_at" \
      '[{
        number: 42,
        created_at: "2026-08-11T23:00:00Z",
        merged_at: (if $merged_at == "" then null else $merged_at end),
        merge_commit_sha: $merge,
        base: {sha: $base, ref: "master"},
        head: {sha: $head, ref: "feat/example"}
      }]'
    ;;
  "repos/example/mova/git/commits/${FAKE_HEAD_SHA}")
    tree="$FAKE_HEAD_TREE"
    if [[ "$FAKE_SCENARIO" == wrong_tree ]]; then
      tree="0000000000000000000000000000000000000000"
    fi
    jq -n --arg tree "$tree" '{tree: {sha: $tree}}'
    ;;
  "repos/example/mova/actions/workflows/ci.yml/runs?event=pull_request&head_sha=${FAKE_HEAD_SHA}&per_page=100")
    conclusion=success
    second_run=false
    if [[ "$FAKE_SCENARIO" == failed_ci ]]; then
      conclusion=failure
    elif [[ "$FAKE_SCENARIO" == stale_success ]]; then
      second_run=true
    fi
    jq -n \
      --arg head "$FAKE_HEAD_SHA" \
      --arg conclusion "$conclusion" \
      --arg base "$BASE_SHA" \
      --argjson second_run "$second_run" \
      '{workflow_runs: ([{
        id: 1234,
        run_attempt: (if $second_run then 9 else 1 end),
        created_at: "2026-08-12T00:00:00Z",
        path: ".github/workflows/ci.yml",
        event: "pull_request",
        status: "completed",
        conclusion: $conclusion,
        head_sha: $head,
        head_branch: "feat/example",
        pull_requests: []
      }] + (if $second_run then [{
        id: 1235,
        run_attempt: 1,
        created_at: "2026-08-12T00:01:00Z",
        path: ".github/workflows/ci.yml",
        event: "pull_request",
        status: "completed",
        conclusion: "failure",
        head_sha: $head,
        head_branch: "feat/example",
        pull_requests: []
      }] else [] end))}'
    ;;
  "repos/example/mova/actions/runs/1234/jobs?per_page=100")
    docker_conclusion=success
    web_conclusion=success
    website_conclusion=success
    if [[ "$FAKE_SCENARIO" == skipped_job ]]; then
      docker_conclusion=skipped
    elif [[ "$FAKE_SCENARIO" == release_only ]]; then
      web_conclusion=skipped
      website_conclusion=skipped
    fi
    jq -n \
      --arg docker "$docker_conclusion" \
      --arg web "$web_conclusion" \
      --arg website "$website_conclusion" \
      --arg provenance "Source provenance [PR #42 base ${BASE_SHA} head ${FAKE_HEAD_SHA}]" \
      '{jobs: [
      {name: $provenance, status: "completed", conclusion: "success"},
      {name: "Web", status: "completed", conclusion: $web},
      {name: "Website", status: "completed", conclusion: $website},
      {name: "Rust and PostgreSQL", status: "completed", conclusion: "success"},
      {name: "Docker image", status: "completed", conclusion: $docker},
      {name: "CI gate", status: "completed", conclusion: "success"}
    ]}'
    ;;
  *)
    printf 'Unexpected fake GitHub endpoint: %s\n' "$endpoint" >&2
    exit 1
    ;;
esac
EOF
chmod +x "$FAKE_GH"

export BASE_SHA

run_case() {
  local expected="$1"
  local scenario="$2"
  local source_sha="${3:-$SOURCE_SHA}"
  local head_sha="${4:-$HEAD_SHA}"
  local head_tree="${5:-$HEAD_TREE}"
  local output

  git -C "$TEST_REPOSITORY" checkout --quiet --detach "$source_sha"
  output="$(
    MOVA_REPOSITORY_ROOT="$TEST_REPOSITORY" \
    MOVA_SOURCE_SHA="$source_sha" \
    MOVA_GITHUB_REPOSITORY=example/mova \
    MOVA_TARGET_BRANCH=master \
    MOVA_GH_COMMAND="$FAKE_GH" \
    FAKE_SCENARIO="$scenario" \
    FAKE_SOURCE_SHA="$source_sha" \
    FAKE_HEAD_SHA="$head_sha" \
    FAKE_HEAD_TREE="$head_tree" \
      "$VERIFIER" 2>"$TEST_DIRECTORY/${scenario}.err"
  )"
  if [[ "${MOVA_KEEP_TEST_OUTPUT:-false}" == "true" ]]; then
    printf 'Verifier stderr (%s):\n' "$scenario" >&2
    sed 's/^/  /' "$TEST_DIRECTORY/${scenario}.err" >&2
  fi
  if [[ "$output" != "$expected" ]]; then
    printf 'Expected %s for scenario %s; got %s.\n' "$expected" "$scenario" "$output" >&2
    exit 1
  fi
}

run_case true success
grep -F "PR #42 CI is reusable for master revision $SOURCE_SHA." \
  "$TEST_DIRECTORY/success.err" >/dev/null
run_case false api_failure
run_case false wrong_merge
run_case false wrong_base
run_case false unmerged
run_case false wrong_tree
run_case false failed_ci
run_case false stale_success
run_case false skipped_job
run_case true release_only

invalid_output="$(
  MOVA_REPOSITORY_ROOT="$TEST_REPOSITORY" \
  MOVA_SOURCE_SHA=invalid \
  MOVA_GITHUB_REPOSITORY=example/mova \
  MOVA_GH_COMMAND="$FAKE_GH" \
    "$VERIFIER" 2>"$TEST_DIRECTORY/invalid.err"
)"
[[ "$invalid_output" == false ]] || {
  echo "Invalid input did not fail closed." >&2
  exit 1
}

echo "Pull request CI reuse verifier tests passed."
