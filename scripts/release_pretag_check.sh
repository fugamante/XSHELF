#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

python3 rust/cxrs/tools/release_check.py \
  --repo-root "$ROOT_DIR" \
  --require-current-release-notes \
  --require-current-release-source \
  --require-published-status-docs
