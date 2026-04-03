# TurboQuant Phase 3 Representation Spike

Branch: `cx/turboquant-spike`
Status: planned
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

Implement the first narrow design step only:

1. define fixed centroid table shape and sidecar payload layout
2. export that as the first Phase 3 patch-plan checkpoint
3. do not touch read/write kernels until the storage contract is pinned
