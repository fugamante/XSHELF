#!/usr/bin/env bash
set -euo pipefail

MODE="quick"
JSON=0
OUT_FILE=".cx/compat/all_latest.json"
OUT_EXPLICIT=0
declare -a REPOS=()

usage() {
  cat >&2 <<'USAGE'
usage: ./scripts/compat_all.sh [--quick|--full] [--json] [--out <path>] [--repo <path> ...]

Runs local compat checks for one or more repositories by calling each repo's
scripts/compat_local.sh and emits an aggregate report.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --quick) MODE="quick"; shift ;;
    --full) MODE="full"; shift ;;
    --json) JSON=1; shift ;;
    --out)
      OUT_FILE="${2:-}"
      [[ -n "$OUT_FILE" ]] || { echo "compat-all: --out requires a path" >&2; exit 2; }
      OUT_EXPLICIT=1
      shift 2
      ;;
    --repo)
      local_repo="${2:-}"
      [[ -n "$local_repo" ]] || { echo "compat-all: --repo requires a path" >&2; exit 2; }
      REPOS+=("$local_repo")
      shift 2
      ;;
    -h|--help) usage; exit 0 ;;
    *) echo "compat-all: unknown argument '$1'" >&2; usage; exit 2 ;;
  esac
done

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"

if [[ ${#REPOS[@]} -eq 0 ]]; then
  REPOS+=("$repo_root")
  parent_dir="$(cd "$repo_root/.." && pwd)"
  for sibling_name in cx cx-eval-lab; do
    sibling="$parent_dir/$sibling_name"
    [[ "$sibling" == "$repo_root" ]] && continue
    if [[ -x "$sibling/scripts/compat_local.sh" ]]; then
      REPOS+=("$sibling")
    fi
  done
fi

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

entries_jsonl="$tmpdir/repos.jsonl"
report_json="$tmpdir/all.json"
text_rows="$tmpdir/text_rows"
: > "$entries_jsonl"
: > "$text_rows"

overall_rc=0
failed_repos=0
generated_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
repo_index=0

for repo in "${REPOS[@]}"; do
  repo_abs="$(cd "$repo" 2>/dev/null && pwd || true)"
  [[ -n "$repo_abs" ]] || repo_abs="$repo"

  compat_script="$repo_abs/scripts/compat_local.sh"
  # Each invocation needs its own slot even when repository basenames match.
  result_file="$tmpdir/repo_${repo_index}_compat.json"
  repo_index=$((repo_index + 1))
  rc=0

  if [[ -x "$compat_script" ]]; then
    # Keep the child's status; negating this command would replace it with zero.
    "$compat_script" "--$MODE" --json --out "$result_file" >/dev/null 2>&1 || rc=$?
  else
    rc=127
    cat > "$result_file" <<JSON
{"status":"fail","mode":"$MODE","summary":{"steps_total":0,"steps_failed":1},"error":"missing compat_local.sh"}
JSON
  fi

  # A child can fail before writing its report. Keep a structured failure row
  # so one bad child cannot erase the aggregate and the other child results.
  if [[ -s "$result_file" ]] &&
    result_json="$(jq -c -s '
      if length == 1 and
         (.[0] | type) == "object" and
         (.[0].status | type) == "string" and
         (.[0].summary | type) == "object" and
         (.[0].summary.steps_failed | type) == "number"
      then .[0] else error("invalid child report") end
    ' "$result_file" 2>/dev/null)"; then
    :
  else
    result_json='{"status":"fail","summary":{"steps_total":0,"steps_failed":1},"error":"missing or invalid compat report"}'
  fi

  if [[ "$rc" -eq 0 ]] &&
    jq -e '.status == "ok" and .summary.steps_failed == 0' <<< "$result_json" >/dev/null; then
    row_state="ok"
  else
    row_state="fail"
    overall_rc=1
    failed_repos=$((failed_repos + 1))
  fi
  printf ' - [%s] %s\n' "$row_state" "$repo_abs" >> "$text_rows"

  jq -n \
    --arg path "$repo_abs" \
    --argjson exit_code "$rc" \
    --argjson result "$result_json" \
    '{path:$path, exit_code:$exit_code, result:$result}' >> "$entries_jsonl"
done

repos_total="${#REPOS[@]}"
status_final="PASS"
if [[ "$overall_rc" -ne 0 ]]; then
  status_final="FAIL"
fi

repos_arr='[]'
if [[ -s "$entries_jsonl" ]]; then
  repos_arr="$(jq -s '.' "$entries_jsonl")"
fi

jq -n \
  --arg status "ok" \
  --arg mode "$MODE" \
  --arg generated_at "$generated_at" \
  --argjson repos "$repos_arr" \
  --argjson repos_total "$repos_total" \
  --argjson repos_failed "$failed_repos" \
  --arg status_final "$status_final" \
  '{
    status:$status,
    mode:$mode,
    generated_at:$generated_at,
    repos:$repos,
    summary:{repos_total:$repos_total,repos_failed:$repos_failed},
    status_final:$status_final
  }' > "$report_json"

if [[ "$OUT_EXPLICIT" -eq 1 ]]; then
  python3 "$script_dir/compat_report.py" --out "$OUT_FILE" < "$report_json"
else
  python3 "$script_dir/compat_report.py" --default-root "$(pwd -P)" --leaf all_latest.json < "$report_json"
fi

if [[ "$JSON" -eq 1 ]]; then
  cat "$report_json"
else
  echo "compat-all: mode=$MODE status=$(jq -r '.status_final' "$report_json")"
  echo "compat-all: report=$OUT_FILE"
  cat "$text_rows"
fi

if [[ "$overall_rc" -eq 0 ]]; then
  exit 0
fi
exit 1
