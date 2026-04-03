# TurboQuant Vector Hardening Plan

Branch: `cx/turboquant-spike`
Status: active
Scope: harden the current `tq_v1_vec` path before opening a second vector family

## Why This Exists

Phase 3 still looks promising, but the corrected branch state is narrower than the earlier provisional verdict.

What is already proven:

- vector payload capture is materially compact
- the branch can now surface real read-side decode work in artifacts
- `retrieval` and `context_fill` still pass on the corrected real replay path at `8k`

What is not yet stable enough:

- exact `smoke` fails on the corrected real replay path
- exact strict JSON `instruct` fails on the corrected real replay path
- the earlier ladder/value artifacts are provisional until real replay correctness is restored

Artifacts:

- `docs/TURBOQUANT_VEC_CHECK.json`
- `docs/TURBOQUANT_VEC_VALUE.json`
- `docs/TURBOQUANT_VEC_LADDER.json`
- `docs/TURBOQUANT_VEC_PROFILE.json`

## Hardening Order

1. Read-path accounting
2. Replay-fidelity correction
3. Prompt-shape profiling
4. Decode-path allocation control
5. Codebook/payload access locality
6. Re-run ladder

## Work Items

### H1 Read-Path Accounting

Objective:

- make read-side work visible per prompt and per context

Tasks:

- ensure vector decode updates report fields that actually surface in artifacts
- capture per-run:
  - decode calls
  - decode rows
  - decoded groups
  - codec mode

Success:

- `docs/TURBOQUANT_VEC_*` artifacts show real read-side activity, not only write-side storage

Status:

- complete via `patches/tq_p3_slice4.patch`
- real replay counters are now visible in `docs/TURBOQUANT_VEC_CHECK.json`

### H2 Replay-Fidelity Correction

Objective:

- restore exact-task correctness on the real `vec` replay path

Tasks:

- isolate why `smoke` drifts from exact `OK`
- isolate why `instruct` drifts from the strict JSON contract
- keep `retrieval` and `context_fill` as regression guards while correcting replay fidelity

Success:

- corrected `8k` suite returns to `4/4` pass under actual replay, not capture-only accounting

Current checkpoint:

- `8` bits restores `4/4` correctness on the real replay path
- `6` bits restores `3/4` and misses only `smoke`
- `4` bits remains too lossy
- next work should target `6` bits smoke recovery rather than push `8` bits up the ladder

### H3 Prompt-Shape Profiling

Objective:

- separate prompt-length effects from generation-shape effects on the corrected replay path

Tasks:

- record prompt token counts beside the current validation runs
- group prompt classes:
  - short exact
  - long summarize
  - retrieval exact
  - strict JSON
- identify whether low decode throughput correlates more with:
  - longer generations
  - stricter answer shape
  - larger read reuse volume

Success:

- one artifact explains why `instruct` and `context_fill` are slower than `smoke`

### H4 Decode-Path Allocation Control

Objective:

- remove obvious per-group allocation churn from the current vector decoder

Tasks:

- replace per-group dynamic vectors with reusable scratch buffers where possible
- avoid repeated temporary construction inside the hottest loop

Success:

- measurable decode gain on the hotspot prompts without correctness drift

### H5 Access Locality

Objective:

- reduce avoidable indirection in centroid lookup and payload unpack

Tasks:

- inspect bit-unpack and centroid fetch order on the active path
- tighten contiguous access where straightforward

Success:

- no representation change, but better decode efficiency on the same `tq_v1_vec` path

### H6 Re-run Ladder

Objective:

- confirm any hardening change survives the fixed ladder

Tasks:

- rerun:
  - `8k`
  - `16k`
  - `32k`
- regenerate:
  - `docs/TURBOQUANT_VEC_CHECK.json`
  - `docs/TURBOQUANT_VEC_16K.json`
  - `docs/TURBOQUANT_VEC_32K.json`
  - `docs/TURBOQUANT_VEC_LADDER.json`
  - `docs/TURBOQUANT_VEC_PROFILE.json`

Success:

- hotspot prompts improve without losing exact-task correctness

## Non-Goals

- no second vector family yet
- no `K`-cache work yet
- no `MLX` follow-through yet
- no production claim yet

## Decision Rule

Only open a second vector/codebook variant if:

- the current path cannot restore exact `smoke` and strict JSON under real replay, or
- hardening shows the slowdown or drift is intrinsic to this exact representation layout
