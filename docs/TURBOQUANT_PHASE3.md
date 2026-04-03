# TurboQuant Phase 3 Representation Spike

Branch: `cx/turboquant-spike`
Status: in progress
Scope: new representation-class experiment after Phase 2 scalar closeout

## Why Phase 3 Exists

Phase 2 closed the scalar-family question.

What Phase 2 established:

- scalar-family correctness can be restored under snapshot-backed replay
- scalar-family value is still poor on the validated host path
- residual scalar is a quality no-go
- grouped/projection scalar are correctness-viable but operationally unattractive

So Phase 3 must not be "more scalar tuning".
It must test a different representation class.

## Objective

Answer one question cleanly:

- can a non-scalar representation deliver a meaningfully better memory/runtime tradeoff than the closed Phase 2 scalar track while preserving the same correctness gates?

## Entry Rule

Phase 3 only begins if the representation class is one of:

- vector/codebook quantization
- stronger structured residual path materially closer to the paper
- another clearly different non-scalar representation with a defensible rationale

Rejected Phase 3 starting points:

- more scalar tuning
- more scalar residual tuning
- more simple transform-assisted scalar tuning
- `K`-cache work before `V`-side value is proven

## Baseline To Beat

The new representation must beat the closed scalar track, not just baseline raw correctness.

Reference artifacts:

- `docs/TURBOQUANT_SCALAR_VALUE.json`
- `docs/TURBOQUANT_SCALAR_COMPARE.json`

Current scalar reference facts:

- sidecar storage: `26.56%` of raw
- bytes saved: `73.44%`
- decode throughput: roughly `~2.7 t/s`
- wall time: roughly `~3x` baseline on the fixed `8k` suite

## Required Gates

The new representation is only a Phase 3 `go` if all hold:

- passes the fixed `8k` suite under snapshot-backed replay
- preserves:
  - exact `smoke`
  - exact `retrieval`
  - exact strict JSON `instruct`
- improves runtime/value versus the scalar reference
- keeps implementation scope narrow enough to remain an experiment rather than a backend fork explosion

## Minimal Deliverables

1. representation contract note
2. one patch artifact for the first compile-clean slice
3. one validation artifact on the fixed `8k` suite
4. one value artifact comparing against:
   - baseline
   - scalar reference

Initial contract files:

- `docs/TURBOQUANT_VEC.md`
- `docs/TURBOQUANT_PHASE3_WORK.json`
- `docs/TURBOQUANT_VEC_LAYOUT.md`

## Current Checkpoint

Phase 3 slice 1 is now in place:

- patch artifact: `patches/tq_p3_slice1.patch`
- backend target: pinned `llama.cpp` analysis checkout
- status: compile-clean scaffold only

What slice 1 adds:

- explicit vector codebook state per layer
- deterministic `fp16` centroid residency for a fixed `[16][8]` codebook
- explicit vector payload/codebook byte counters in TurboQuant layer state
- reset/init wiring so the codebook exists before any vector read/write slice

What slice 1 does not add:

- no vector write-side encoding
- no vector read-side decode
- no runtime path switch away from the closed scalar baseline
- no value claim yet

Phase 3 slice 2 is now in place:

- patch artifact: `patches/tq_p3_slice2.patch`
- backend target: pinned `llama.cpp` analysis checkout
- status: compile-clean vector capture only

What slice 2 adds:

- `vec` codec-mode parsing in the analysis checkout
- deterministic nearest-centroid scoring against the fixed per-layer codebook
- packed vector payload capture into explicit `vec_payload` storage
- vector byte accounting for:
  - payload bytes
  - codebook bytes
- explicit guard in `get_v()` so vector mode remains capture-only until a decoder exists

What slice 2 does not add:

- no vector snapshot decode
- no fixed `8k` validation artifact yet
- no value comparison yet

Phase 3 slice 3 is now in place:

- patch artifact: `patches/tq_p3_slice3.patch`
- validation artifact: `docs/TURBOQUANT_VEC_CHECK.json`
- status: first vector snapshot decoder validated at `8k`

What slice 3 adds:

- vector snapshot replay on the host-backed `V` path
- fixed `8k` suite validation under:
  - `LLAMA_TQ_CODEC_MODE=vec`
  - `LLAMA_TQ_SNAPSHOT_READ=1`
  - `--flash-attn on`
  - `--no-kv-offload`

Observed checkpoint:

- `smoke`: pass
- `context_fill`: pass
- `retrieval`: pass
- `instruct`: pass
- reported sidecar ratio on the validated `8k` runs: roughly `1.6%` to `2.1%` of raw

What slice 3 does not decide yet:

- no value verdict versus the closed scalar reference yet
- no production-worthiness claim

## Value Checkpoint

Artifact:

- `docs/TURBOQUANT_VEC_VALUE.json`

Vector result at `8k` under snapshot replay:

- passes: `4/4`
- mean decode throughput: `30.6 t/s`
- mean wall time: `6707.5 ms`
- mean raw ratio: `1.76%`
- mean bytes saved: `98.24%`

Closed scalar reference:

- mean decode throughput: `2.73 t/s`
- mean wall time: `17022.5 ms`
- mean raw ratio: `26.56%`
- mean bytes saved: `73.44%`

Phase 3 decision from the current data:

- `vector_go`

Reason:

- the first vector snapshot path passes the fixed `8k` suite
- it materially beats the closed scalar reference on runtime
- it materially beats the scalar reference on storage ratio
- the representation-class change is justified by measured value, not only by correctness

## Context Ladder

Artifact:

- `docs/TURBOQUANT_VEC_LADDER.json`

Observed ladder:

- `8k`
  - passes: `4/4`
  - mean decode throughput: `30.6 t/s`
  - mean raw ratio: `1.76%`
- `16k`
  - passes: `4/4`
  - mean decode throughput: `12.55 t/s`
  - mean raw ratio: `1.76%`
- `32k`
  - passes: `4/4`
  - mean decode throughput: `20.52 t/s`
  - mean raw ratio: `1.76%`

Important reading:

- correctness holds across the fixed suite at `8k`, `16k`, and `32k`
- storage ratio stays stable across the ladder
- decode behavior becomes prompt-sensitive at longer contexts:
  - `smoke` remains fast
  - `instruct` remains the weakest path
  - `context_fill` also degrades sharply at longer contexts

Current branch decision:

- `harden_current_vector_path`

Reason:

- the vector representation is already a correctness and storage win
- the next risk is runtime behavior by prompt shape, not representation viability
- opening a second vector/codebook variant now would blur the diagnosis

## Stop Conditions

Stop Phase 3 quickly if any of these happen:

- correctness fails in the same way as the rejected scalar paths
- runtime remains near the scalar reference penalty band
- integration scope grows faster than the measured value

## Recommended First Attempt

Preferred first Phase 3 attempt:

- vector/codebook `V`-side prototype
- host-backed path only
- snapshot replay retained as the correctness baseline

Reason:

- this is the cleanest representation-class change from the closed scalar track
- it is closest to the paper's actual direction
- it gives the best chance of changing the value curve rather than relabeling the same scalar family

## Immediate Next Step

Implement the first executable vector slice only:

1. profile prompt-type-specific decode collapse on the current vector path
2. harden the current vector path before opening any second variant
3. only after that consider broader backend or `MLX` follow-through
