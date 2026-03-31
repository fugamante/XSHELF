# TurboQuant Baseline Report

Branch: `cx/turboquant-spike`
Artifact contract: `turboquant-baseline.v1`
Status: blocked pending model access

## Environment

- collection date: 2026-03-31
- backend: `llama.cpp`
- backend ref: `8590 (5ce013cd7)`
- model: pending concrete baseline selection
- quantization: pending
- hardware: current local Apple Silicon machine
- acceleration mode: `metal`

## Backend / Model Selection

Reason for choosing this backend/model pair:

- backend fixed by Phase 0: `llama.cpp`
- first dry-run candidate: `ggml-org/SmolLM2-360M-Instruct-GGUF:Q4_K_M`
- primary 7B-14B baseline candidate still pending concrete local GGUF or authenticated fetch

## Prompt Set

Baseline prompt set revision:

- `context_fill`
- `retrieval_check`
- `instruction_follow`
- `structured_smoke`

Notes:

- prompt set names fixed in `docs/TURBOQUANT_PHASE1.md`
- concrete prompt texts still need to be checked in before baseline is considered complete

## Results

| profile | context | kv-cache MB | prefill ms | decode tok/s | wall ms | quality | pass/fail |
| --- | --- | --- | --- | --- | --- | --- | --- |
| baseline_local_small | 8k | | | | | | |
| baseline_local_small | 16k | | | | | | |
| baseline_local_primary | 8k | | | | | | |
| baseline_local_primary | 16k | | | | | | |
| baseline_local_primary | 32k | | | | | | |
| baseline_local_stress | max practical | | | | | | |

## Observations

- local backend binary is available and versioned
- first unauthenticated `--hf-repo` smoke fetch failed with:
  - `GET failed (401): Invalid username or password.`
- no local GGUF path has been locked into this branch yet
- Phase 1 can proceed immediately after either:
  - supplying a local GGUF path, or
  - exporting a valid `HF_TOKEN`

## Go / No-Go For Phase 2

Decision:

- no-go for Phase 2 yet

Reason:

- backend harness is operational
- concrete model access is still blocked
- real measurements have not been collected yet
