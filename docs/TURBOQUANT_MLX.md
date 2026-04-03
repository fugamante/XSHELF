# TurboQuant `MLX` Comparative Track

Branch: `cx/turboquant-spike`
Status: planned
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
