# TurboQuant `MLX` Comparative Track

Branch: `cx/turboquant-spike`
Status: active
Scope: compare the closed `llama.cpp` vector result against an Apple-native `MLX` path

## Why This Track Exists

The `llama.cpp` path has already answered the representation question:

- scalar track: closed as a value no-go
- vector track: closed as a value go

That means the next high-value question is portability, not more `llama.cpp` rescue work.

Specifically:

- does the current vector result survive on an Apple-native backend?
- do the runtime and storage wins hold on the hardware that actually matters for this spike?
- does `MLX` make the implementation simpler, harder, or just different?

## Comparison Baseline

Reference artifacts:

- `docs/TURBOQUANT_PHASE3_CLOSE.json`
- `docs/TURBOQUANT_VEC_LADDER.json`
- `docs/TURBOQUANT_VEC_PROFILE.json`

Closed `llama.cpp` reference:

- preferred mode: `vec_bits6_bypass256`
- `8k`
  - decode `17.68 t/s`
  - wall `12655 ms`
  - raw ratio `3.12%`
- `16k`
  - decode `21.3 t/s`
  - wall `9632.5 ms`
  - raw ratio `6.23%`
- `32k`
  - decode `19.05 t/s`
  - wall `10267.5 ms`
  - raw ratio `6.23%`

## Objective

Answer one question cleanly:

- can an `MLX` implementation reproduce the Phase 3 vector win, or does backend reality make the `llama.cpp` result non-portable?

## Non-Goals

- no immediate `K`-cache work
- no production `MLX` integration in core CX
- no new representation family before the first `MLX` comparison exists
- no mainline merge proposal from this track alone

## Constraints

- keep the same prompt set:
  - `smoke`
  - `context_fill`
  - `retrieval`
  - `instruct`
- keep the same ladder:
  - `8k`
  - `16k`
  - `32k`
- keep the same comparison categories:
  - correctness
  - decode throughput
  - wall time
  - effective storage ratio or closest defensible memory proxy

## Required Deliverables

1. `MLX` backend contract note
2. benchmark/runbook note for the first `MLX` pass
3. one artifact matching the current ladder structure
4. one conclusion artifact comparing:
   - `llama.cpp` vector result
   - `MLX` result
   - implementation complexity notes

## Success Gates

The `MLX` track is a `go` only if all hold:

- the fixed suite stays exact at `8k`
- the `MLX` ladder is reproducible at `8k`, `16k`, and `32k`
- memory/runtime reporting is close enough in shape to compare honestly against `llama.cpp`
- the implementation path does not explode in complexity relative to the measured gain

## Decision Rule

At the end of the first `MLX` pass, decide one of:

- `mlx_portable_go`
  - the current vector result survives and is worth deeper backend follow-through
- `mlx_partial_go`
  - correctness holds, but the value curve or complexity is materially worse than `llama.cpp`
- `mlx_no_go`
  - the Phase 3 result does not survive on `MLX` cleanly enough to justify deeper work

## First Work Sequence

1. lock the exact `MLX` model/runtime surface
2. define the closest equivalent storage/memory metric to the `llama.cpp` raw-ratio field
3. port the fixed prompt ladder contract without changing prompt semantics
4. capture the first baseline `MLX` artifact before any `MLX`-specific optimization
5. compare against the closed `llama.cpp` vector reference

## Immediate Next Step

Start with contract and measurement parity, not backend optimization:

1. identify the practical `MLX` runtime entrypoint for the local model
2. determine what memory/accounting fields `MLX` can expose without kernel surgery
3. only then run the first `8k` parity pass

## Resolved Runtime Surface

Current local runtime:

- Python env:
  - `/tmp/cx_mlx_env`
- entrypoint:
  - `/tmp/cx_mlx_env/bin/python -m mlx_lm generate`
- package state:
  - `mlx_lm 0.31.1`

Current model surface:

- `mlx-community/Llama-3.2-3B-Instruct-4bit`

Why this is the current probe target:

- it runs on this machine now
- it exposes prompt TPS, generation TPS, and peak memory directly
- it is sufficient to establish the first backend-comparison shape without inventing a custom `MLX` kernel path yet

## Current Metrics Contract

Current measurable fields on the `MLX` path:

- prompt tokens/sec
- decode tokens/sec
- wall time
- peak memory in GB
- exact-task correctness on the fixed prompt set

Current limitation:

- `raw_ratio` is not yet directly available on the `MLX` path
- for now, the comparison memory proxy is:
  - `peak_memory_gb`

## First `8k` Checkpoint

Artifact:

- `docs/TURBOQUANT_MLX_8K.json`

Current result:

- `4/4` pass at `8k`
- `smoke`
  - decode `328.308 t/s`
  - wall `2252 ms`
  - peak memory `1.925 GB`
- `context_fill`
  - decode `121.816 t/s`
  - wall `3476 ms`
  - peak memory `2.707 GB`
- `retrieval`
  - decode `159.299 t/s`
  - wall `4398 ms`
  - peak memory `2.96 GB`
- `instruct`
  - decode `147.809 t/s`
  - wall `3550 ms`
  - peak memory `2.728 GB`

Reading:

- the fixed `8k` prompt suite survives the first `MLX` pass intact
- `MLX` is no longer a planning-only backend for this spike
- the next correct move is to extend the same measurement path to `16k` and `32k`

## First Ladder

Artifacts:

- `docs/TURBOQUANT_MLX_16K.json`
- `docs/TURBOQUANT_MLX_32K.json`
- `docs/TURBOQUANT_MLX_LADDER.json`

Current result:

- `8k`
  - `4/4`
  - mean decode `189.308 t/s`
  - mean wall `3419.0 ms`
  - peak memory up to `2.96 GB`
- `16k`
  - `4/4`
  - mean decode `138.389 t/s`
  - mean wall `4540.0 ms`
  - peak memory up to `2.96 GB`
- `32k`
  - `4/4`
  - mean decode `171.317 t/s`
  - mean wall `4540.0 ms`
  - peak memory up to `2.96 GB`

Reading:

- the fixed prompt ladder survives intact on `MLX`
- on correctness and runtime shape, the Phase 3 vector result appears portable
- memory comparison is still incomplete because this backend currently exposes only peak memory, not a direct `raw_ratio` analog

## Current Comparative Reading

Artifact:

- `docs/TURBOQUANT_MLX_COMPARE.json`

Current decision:

- `mlx_portable_go_provisional`

Why:

- `MLX` keeps `4/4` through `8k`, `16k`, and `32k`
- runtime is strong enough that portability is no longer speculative
- the only material comparison gap left is memory-accounting fidelity, not backend viability

Next constraint:

- do not overstate memory conclusions until the `MLX` proxy story is improved
