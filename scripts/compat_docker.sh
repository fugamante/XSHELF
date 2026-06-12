#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

IMAGE_TAG="xshelf-compat:local"
MODE="quick"
JSON_STDOUT=0
OUT_FILE=""
REBUILD=0
PASS_TTY=0
declare -a EXTRA_ARGS=()

usage() {
  cat >&2 <<'USAGE'
usage: ./scripts/compat_docker.sh [--quick|--full] [--json] [--out <path>] [--rebuild] [--tty] [--docker-arg <arg>]

Builds the local compat image when needed, bind-mounts the current repository,
and runs scripts/compat_local.sh inside Docker.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --quick)
      MODE="quick"
      shift
      ;;
    --full)
      MODE="full"
      shift
      ;;
    --json)
      JSON_STDOUT=1
      shift
      ;;
    --out)
      OUT_FILE="${2:-}"
      [[ -n "$OUT_FILE" ]] || { echo "compat-docker: --out requires a path" >&2; exit 2; }
      shift 2
      ;;
    --rebuild)
      REBUILD=1
      shift
      ;;
    --tty)
      PASS_TTY=1
      shift
      ;;
    --docker-arg)
      EXTRA_ARGS+=("${2:-}")
      [[ -n "${EXTRA_ARGS[-1]}" ]] || { echo "compat-docker: --docker-arg requires a value" >&2; exit 2; }
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "compat-docker: unknown argument '$1'" >&2
      usage
      exit 2
      ;;
  esac
done

command -v docker >/dev/null 2>&1 || {
  echo "compat-docker: docker is required" >&2
  exit 2
}

if [[ "$REBUILD" -eq 1 ]] || ! docker image inspect "$IMAGE_TAG" >/dev/null 2>&1; then
  echo "compat-docker: building $IMAGE_TAG" >&2
  docker build -t "$IMAGE_TAG" "$ROOT_DIR" >&2
fi

uid_gid="$(id -u):$(id -g)"
tty_args=()
if [[ "$PASS_TTY" -eq 1 ]] && [[ -t 0 ]] && [[ -t 1 ]]; then
  tty_args=(-it)
fi

container_cmd=(./scripts/compat_local.sh "--$MODE")
if [[ "$JSON_STDOUT" -eq 1 ]]; then
  container_cmd+=(--json)
fi
if [[ -n "$OUT_FILE" ]]; then
  container_cmd+=(--out "$OUT_FILE")
fi

docker run --rm \
  "${tty_args[@]}" \
  --user "$uid_gid" \
  --workdir /work \
  -e HOME=/tmp/cx-home \
  -e CARGO_TARGET_DIR=/work/.cx/compat/docker-target \
  -e PATH=/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
  -v "$ROOT_DIR":/work \
  "${EXTRA_ARGS[@]}" \
  "$IMAGE_TAG" \
  bash -c 'mkdir -p "$HOME" /work/.cx/compat/docker-target && "$@"' bash "${container_cmd[@]}"
