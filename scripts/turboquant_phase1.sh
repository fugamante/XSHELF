#!/usr/bin/env bash

set -euo pipefail

cx_tq_die() {
  printf 'turboquant_phase1: %s\n' "$*" >&2
  exit 1
}

cx_tq_backend_ref() {
  command -v llama-cli >/dev/null 2>&1 || cx_tq_die "llama-cli not found"
  llama-cli --version 2>&1 | tail -n 2
}

cx_tq_mem_gb() {
  local mem_bytes
  mem_bytes="$(sysctl -n hw.memsize 2>/dev/null || printf '0')"
  awk -v bytes="$mem_bytes" 'BEGIN { printf "%.0f\n", bytes / 1024 / 1024 / 1024 }'
}

cx_tq_accel() {
  if llama-cli --list-devices 2>&1 | grep -Eq 'MTL[0-9]|Apple M[0-9]'; then
    printf 'metal\n'
    return
  fi
  if [[ "$(uname -s)" == "Darwin" && "$(uname -m)" == "arm64" ]]; then
    printf 'metal\n'
    return
  fi
  printf 'unknown\n'
}

cx_tq_init_artifact() {
  local out="${1:-}"
  [[ -n "$out" ]] || cx_tq_die "init-artifact requires an output path"
  local backend_ref
  backend_ref="$(cx_tq_backend_ref | tail -n 2 | tr '\n' ' ' | sed 's/  */ /g; s/ $//')"
  cat >"$out" <<EOF
{
  "contract_version": "turboquant-baseline.v1",
  "phase": "phase1",
  "backend": {
    "name": "llama.cpp",
    "ref": "$(printf '%s' "$backend_ref" | sed 's/"/\\"/g')"
  },
  "hardware": {
    "label": "$(uname -m)-darwin",
    "memory_gb": $(cx_tq_mem_gb),
    "accel": "$(cx_tq_accel)"
  },
  "profiles": []
}
EOF
}

cx_tq_probe() {
  command -v llama-cli >/dev/null 2>&1 || cx_tq_die "llama-cli not found"
  local model_path=""
  local hf_repo=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --model)
        shift
        [[ $# -gt 0 ]] || cx_tq_die "--model requires a path"
        model_path="$1"
        ;;
      --hf-repo)
        shift
        [[ $# -gt 0 ]] || cx_tq_die "--hf-repo requires a repo spec"
        hf_repo="$1"
        ;;
      *)
        cx_tq_die "unknown probe arg: $1"
        ;;
    esac
    shift
  done

  if [[ -n "$model_path" && -n "$hf_repo" ]]; then
    cx_tq_die "choose either --model or --hf-repo"
  fi
  if [[ -z "$model_path" && -z "$hf_repo" ]]; then
    cx_tq_die "probe requires --model or --hf-repo"
  fi

  local -a cmd=(llama-cli -c 512 -n 8 --temp 0 --perf -p "Reply with exactly: OK")
  if [[ -n "$model_path" ]]; then
    cmd+=(--model "$model_path")
  else
    cmd+=(--hf-repo "$hf_repo")
  fi

  /usr/bin/time -p "${cmd[@]}"
}

cx_tq_help() {
  cat <<'EOF'
usage:
  turboquant_phase1.sh backend-ref
  turboquant_phase1.sh init-artifact <path>
  turboquant_phase1.sh probe (--model <path.gguf> | --hf-repo <repo[:quant]>)
EOF
}

main() {
  local sub="${1:-}"
  case "$sub" in
    backend-ref)
      shift
      cx_tq_backend_ref "$@"
      ;;
    init-artifact)
      shift
      cx_tq_init_artifact "$@"
      ;;
    probe)
      shift
      cx_tq_probe "$@"
      ;;
    ""|-h|--help|help)
      cx_tq_help
      ;;
    *)
      cx_tq_die "unknown subcommand: $sub"
      ;;
  esac
}

main "$@"
