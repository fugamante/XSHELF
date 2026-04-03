# TurboQuant Vector Hardening Plan

Branch: `cx/turboquant-spike`
Status: active
Scope: harden the current `tq_v1_vec` path before opening a second vector family

## Why This Exists

Phase 3 now has enough evidence to stop broad exploration and tighten the current path.

What is already proven:

- correctness holds at `8k`, `16k`, and `32k`
- storage ratio stays near `1.76%` of raw
- the first vector path beats the closed scalar reference decisively on value

What is not yet stable enough:

- decode throughput becomes strongly prompt-shape dependent at longer contexts

Artifacts:

- `docs/TURBOQUANT_VEC_VALUE.json`
- `docs/TURBOQUANT_VEC_LADDER.json`
- `docs/TURBOQUANT_VEC_PROFILE.json`

## Hotspots

From the current ladder/profile:

- `instruct`
  - risk: `critical`
  - long-context decode: about `1.5 t/s`
  - max wall: about `30910 ms`
- `context_fill`
  - risk: `high`
  - long-context decode: about `2.5` to `3.2 t/s`
  - max wall: about `17760 ms`
- `retrieval`
  - risk: `high`
  - unstable decode behavior across the ladder
- `smoke`
  - risk: `stable`

## Hardening Order

1. Read-path accounting
2. Prompt-shape profiling
3. Decode-path allocation control
4. Codebook/payload access locality
5. Re-run ladder

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

### H2 Prompt-Shape Profiling

Objective:

- separate prompt-length effects from generation-shape effects

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

### H3 Decode-Path Allocation Control

Objective:

- remove obvious per-group allocation churn from the current vector decoder

Tasks:

- replace per-group dynamic vectors with reusable scratch buffers where possible
- avoid repeated temporary construction inside the hottest loop

Success:

- measurable decode gain on the hotspot prompts without correctness drift

### H4 Access Locality

Objective:

- reduce avoidable indirection in centroid lookup and payload unpack

Tasks:

- inspect bit-unpack and centroid fetch order on the active path
- tighten contiguous access where straightforward

Success:

- no representation change, but better decode efficiency on the same `tq_v1_vec` path

### H5 Re-run Ladder

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

- the current path cannot be materially improved by hardening, or
- hardening shows the slowdown is intrinsic to this exact representation layout
