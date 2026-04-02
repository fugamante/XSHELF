#!/usr/bin/env bash

set -euo pipefail

cx_tqv_die() {
  printf 'turboquant_phase2_validate: %s\n' "$*" >&2
  exit 1
}

cx_tqv_parse() {
  local out_file="$1"
  local prompt_label="$2"
  local context_size="$3"
  local predict_n="$4"
  local mode="$5"
  local prompt_name="$6"

  python3 - "$out_file" "$prompt_label" "$context_size" "$predict_n" "$mode" "$prompt_name" <<'PY'
import json
import pathlib
import re
import sys

out_path = pathlib.Path(sys.argv[1])
prompt_label = sys.argv[2]
context_target = int(sys.argv[3])
predict_n = int(sys.argv[4])
mode = sys.argv[5]
prompt_name = sys.argv[6]
text = out_path.read_text()

def cap(pattern, cast=float):
    m = re.search(pattern, text, re.M)
    if not m:
        return None
    return cast(m.group(1))

prompt_tps = cap(r"\[ Prompt: ([0-9.]+) t/s \|")
decode_tps = cap(r"\| Generation: ([0-9.]+) t/s \]")
wall_s = cap(r"^real ([0-9.]+)$", float)

response = text
if "llama_memory_breakdown_print:" in response:
    response = response.split("llama_memory_breakdown_print:", 1)[0]
lines = []
capture = False
for raw in response.splitlines():
    line = raw.rstrip()
    if line.startswith("> "):
        capture = True
        continue
    if capture:
        lines.append(line)
clean = "\n".join(lines).strip()
if "\n\n" in clean:
    clean = clean.split("\n\n")[-1].strip()

quality = "observe"
passed = None
if prompt_name == "smoke":
    quality = "exact_OK"
    passed = clean == "OK"
elif prompt_name == "retrieval":
    quality = "exact_TURBO-314159"
    passed = clean == "TURBO-314159"
elif prompt_name == "instruct":
    quality = "exact_JSON_contract"
    try:
        obj = json.loads(clean)
        passed = obj == {
            "status": "ready",
            "focus": "turboquant-baseline",
            "next_step": "phase2-v-cache",
        }
    except Exception:
        passed = False
elif prompt_name == "context_fill":
    quality = "non_empty_summary"
    passed = bool(clean)

result = {
    "mode": mode,
    "prompt_name": prompt_name,
    "prompt_file": prompt_label,
    "context_target": context_target,
    "predict_n": predict_n,
    "prompt_tokens_per_sec": prompt_tps,
    "decode_tokens_per_sec": decode_tps,
    "wall_ms": None if wall_s is None else round(wall_s * 1000),
    "quality_rule": quality,
    "passed": passed,
    "response_text": clean,
}
print(json.dumps(result))
PY
}

cx_tqv_run_one() {
  local binary="$1"
  local model="$2"
  local prompt_file="$3"
  local prompt_label="$4"
  local context_size="$5"
  local predict_n="$6"
  local mode="$7"
  local prompt_name="$8"
  local out_file
  out_file="$(mktemp)"

  local -a cmd=(
    /usr/bin/time -p
    "$binary"
    --model "$model"
    -c "$context_size"
    -n "$predict_n"
    --temp 0
    --perf
    --single-turn
    --simple-io
    --no-display-prompt
    -f "$prompt_file"
  )

  if [[ "$mode" == "turboquant" ]]; then
    cmd+=(--turboquant-enable --turboquant-group-size 64 --turboquant-codebook-bits 8)
  fi

  "${cmd[@]}" >"$out_file" 2>&1
  cx_tqv_parse "$out_file" "$prompt_label" "$context_size" "$predict_n" "$mode" "$prompt_name"
  rm -f "$out_file"
}

cx_tqv_run() {
  local binary="${CX_TURBOQUANT_LLAMA_CLI:-}"
  local model=""
  local out=""
  local context_size="8192"
  local predict_n="64"

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --binary)
        shift
        binary="${1:-}"
        ;;
      --model)
        shift
        model="${1:-}"
        ;;
      --out)
        shift
        out="${1:-}"
        ;;
      --ctx)
        shift
        context_size="${1:-}"
        ;;
      --predict)
        shift
        predict_n="${1:-}"
        ;;
      *)
        cx_tqv_die "unknown arg: $1"
        ;;
    esac
    shift
  done

  [[ -n "$binary" ]] || cx_tqv_die "--binary is required (or set CX_TURBOQUANT_LLAMA_CLI)"
  [[ -x "$binary" ]] || cx_tqv_die "binary not executable: $binary"
  [[ -n "$model" ]] || cx_tqv_die "--model is required"
  [[ -f "$model" ]] || cx_tqv_die "model not found: $model"
  [[ -n "$out" ]] || cx_tqv_die "--out is required"

  local repo_root
  repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  local prompts_root="$repo_root/docs/tq_prompts"
  local prompts=(
    "smoke:docs/tq_prompts/smoke.txt:$prompts_root/smoke.txt:8"
    "context_fill:docs/tq_prompts/context_fill.txt:$prompts_root/context_fill.txt:$predict_n"
    "retrieval:docs/tq_prompts/retrieval.txt:$prompts_root/retrieval.txt:16"
    "instruct:docs/tq_prompts/instruct.txt:$prompts_root/instruct.txt:48"
  )

  python3 - "$out" "$(basename "$binary")" "$(basename "$model")" "$context_size" <<'PY'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
data = {
    "contract_version": "turboquant-phase2-validate.v1",
    "binary": sys.argv[2],
    "model": sys.argv[3],
    "context_target": int(sys.argv[4]),
    "runs": [],
}
path.write_text(json.dumps(data, indent=2) + "\n")
PY

  local entry
  for entry in "${prompts[@]}"; do
    IFS=: read -r prompt_name prompt_label prompt_file prompt_predict <<<"$entry"
    local base_json tq_json
    base_json="$(cx_tqv_run_one "$binary" "$model" "$prompt_file" "$prompt_label" "$context_size" "$prompt_predict" baseline "$prompt_name")"
    tq_json="$(cx_tqv_run_one "$binary" "$model" "$prompt_file" "$prompt_label" "$context_size" "$prompt_predict" turboquant "$prompt_name")"
    python3 - "$out" "$base_json" "$tq_json" <<'PY'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
data = json.loads(path.read_text())
data["runs"].append(json.loads(sys.argv[2]))
data["runs"].append(json.loads(sys.argv[3]))
path.write_text(json.dumps(data, indent=2) + "\n")
PY
  done

  cat "$out"
}

cx_tqv_help() {
  cat <<'EOF'
usage:
  turboquant_phase2_validate.sh run --binary <llama-cli> --model <path.gguf> --out <path.json> [--ctx <n>]

env:
  CX_TURBOQUANT_LLAMA_CLI   optional default for --binary
EOF
}

main() {
  local sub="${1:-}"
  case "$sub" in
    run)
      shift
      cx_tqv_run "$@"
      ;;
    ""|-h|--help|help)
      cx_tqv_help
      ;;
    *)
      cx_tqv_die "unknown subcommand: $sub"
      ;;
  esac
}

main "$@"
