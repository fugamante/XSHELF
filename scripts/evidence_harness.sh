#!/usr/bin/env bash
# Rebuild and evaluate the opt-in evidence lane against the synthetic corpus.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

python_bin=${EVIDENCE_PYTHON:-python3}
cargo build --locked --quiet --manifest-path rust/cxrs/Cargo.toml
cargo test --locked --quiet --manifest-path rust/cxrs/Cargo.toml --test evidence_tests -- --test-threads=1 1>&2
"$python_bin" -m unittest discover -s test -p evidence_tokens_test.py
"$python_bin" scripts/evidence_tokens.py \
  --binary rust/cxrs/target/debug/cxrs \
  --manifest rust/cxrs/tests/fixtures/evidence_study.json \
  --encoding o200k_base
