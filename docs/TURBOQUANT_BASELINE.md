# TurboQuant Baseline Report

Branch: `cx/turboquant-spike`
Artifact contract: `turboquant-baseline.v1`
Status: blocked pending model access

## Environment

- collection date: 2026-03-31
- backend: `llama.cpp`
- backend ref: `8590 (5ce013cd7)`
- model: `llama3.1:latest` local Ollama asset
- quantization: `Q4_K_M`
- hardware: current local Apple Silicon machine
- acceleration mode: `metal`

## Backend / Model Selection

Reason for choosing this backend/model pair:

- backend fixed by Phase 0: `llama.cpp`
- first dry-run candidate: `ggml-org/SmolLM2-360M-Instruct-GGUF:Q4_K_M`
- primary baseline candidate selected: local `llama3.1:latest` Ollama asset
- selected model blob:
  - `~/.ollama/models/blobs/sha256-667b0c1932bc6ffc593ed1d03f895bf2dc8dc6df21db3042284a6f4416b06a29`
- selection basis:
  - local and immediately available
  - confirmed `GGUF` v3
  - confirmed `Q4_K_M`
  - practical size for first long-context baseline pass on this machine

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
- local baseline model candidate is available and format-validated:
  - `llama3.1:latest`
  - `GGUF` v3
  - `Q4_K_M`
- first unauthenticated `--hf-repo` smoke fetch failed with:
  - `GET failed (401): Invalid username or password.`
- first local GGUF path is now locked for the baseline candidate
- Phase 1 can proceed immediately after either:
  - running against the selected local GGUF path, or
  - exporting a valid `HF_TOKEN` for alternate fetch-based probes

## Go / No-Go For Phase 2

Decision:

- no-go for Phase 2 yet

Reason:

- backend harness is operational
- concrete model candidate is now selected
- real measurements have not been collected yet
