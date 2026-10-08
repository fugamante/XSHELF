#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cd "$TMP"
git init -q

t1="$($ROOT/bin/cx task add "cxnext" --role implementer)"
t2="$($ROOT/bin/cx task add "cxo git status" --role reviewer --parent "$t1")"
t3="$($ROOT/bin/cx task add "cxo git status" --role reviewer)"

# These reviewed synthetic objectives deliberately exercise command execution.
CX_TASK_TRUST_COMMANDS=1 $ROOT/bin/cx task run "$t1" >/dev/null || true
run_log=.cx/cxlogs/runs.jsonl
before_rows=0
if [[ -f "$run_log" ]]; then
  before_rows="$(wc -l < "$run_log")"
fi
# No selected provider is available: run-all must leave the remaining task pending.
if CX_TASK_TRUST_COMMANDS=1 CX_DISABLE_CODEX=1 CX_DISABLE_OLLAMA=1 \
  "$ROOT/bin/cx" task run-all --backend-pool primary,ollama >"$TMP/run-all.out" 2>"$TMP/run-all.err"; then
  echo "run-all unexpectedly accepted an unavailable backend pool" >&2
  exit 1
fi
grep -q 'no available backend from --backend-pool' "$TMP/run-all.err"

s1="$($ROOT/bin/cx task show "$t1" | jq -r '.status')"
s2="$($ROOT/bin/cx task show "$t2" | jq -r '.status')"
s3="$($ROOT/bin/cx task show "$t3" | jq -r '.status')"
[[ "$s1" == "failed" || "$s1" == "complete" ]]
[[ "$s2" == "pending" ]]
[[ "$s3" == "pending" ]]

after_rows=0
if [[ -f "$run_log" ]]; then
  after_rows="$(wc -l < "$run_log")"
fi
[[ "$after_rows" -eq "$before_rows" ]]

echo "task_run_ok"
