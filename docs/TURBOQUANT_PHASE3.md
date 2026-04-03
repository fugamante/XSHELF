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

## Slice History

Phase 3 slice 1:

- patch artifact: `patches/tq_p3_slice1.patch`
- compile-clean scaffold only
- explicit vector codebook state per layer
- deterministic `fp16` centroid residency for a fixed `[16][8]` codebook

Phase 3 slice 2:

- patch artifact: `patches/tq_p3_slice2.patch`
- vector payload capture landed
- deterministic nearest-centroid scoring and packed `vec_payload` storage
- no read-side replay yet

Phase 3 slice 3:

- patch artifact: `patches/tq_p3_slice3.patch`
- vector replay wiring landed
- but later correction work showed the earlier branch verdict was still contaminated by a capture-only path

Phase 3 slice 4:

- patch artifact: `patches/tq_p3_slice4.patch`
- read-path accounting corrected
- the branch now proves real replay work through non-zero `turboquant_report.read` counters
- this is the first checkpoint that can be treated as a real vector replay measurement

## Corrected Current Checkpoint

Artifact:

- `docs/TURBOQUANT_VEC_CHECK.json`

Corrected `8k` outcome under actual `vec` replay:

- `smoke`: fail
- `context_fill`: pass
- `retrieval`: pass
- `instruct`: fail

What is now proven:

- vector read-side work is real, not inferred from write-side accounting
- `turboquant_report.read.decode_calls` is non-zero
- `turboquant_report.read.decode_rows` is non-zero
- `turboquant_report.read.decode_groups` is non-zero
- replay fidelity depends strongly on vector codebook capacity

What changed from the earlier branch reading:

- the old `vector_go`/ladder/value checkpoints should now be treated as provisional capture-side evidence
- they are still useful for storage accounting and historical comparison
- they are not sufficient as the current correctness decision point

## Current Decision

- `vector_replay_correction`

Reason:

- the branch now measures the real replay path
- the real replay path is not yet back to `4/4` on the fixed `8k` suite
- further value expansion would be premature until exact `smoke` and strict JSON `instruct` are restored

## Codebook Sweep

Artifact:

- `docs/TURBOQUANT_VEC_SWEEP.json`

Corrected `8k` replay sweep:

- `4` bits
  - passes: `2/4`
  - failures: `smoke`, `retrieval`
  - mean decode: `13.6 t/s`
  - mean raw ratio: `1.97%`
- `6` bits
  - passes: `3/4`
  - failure: `smoke`
  - mean decode: `14.48 t/s`
  - mean raw ratio: `3.79%`
- `8` bits
  - passes: `4/4`
  - mean decode: `14.0 t/s`
  - mean raw ratio: `6.23%`
  - mean wall: `31415 ms`

Reading:

- `8` bits restores correctness on the real replay path
- but the wall-time penalty is severe
- `6` bits is the most interesting target because it keeps `retrieval` and `instruct` while missing only `smoke`

## Historical Artifacts Now Considered Provisional

These remain useful as branch history, but they are not the active correctness baseline:

- `docs/TURBOQUANT_VEC_VALUE.json`
- `docs/TURBOQUANT_VEC_16K.json`
- `docs/TURBOQUANT_VEC_32K.json`
- `docs/TURBOQUANT_VEC_LADDER.json`
- `docs/TURBOQUANT_VEC_PROFILE.json`

## Correction Plan

Artifact:

- `docs/TURBOQUANT_HARDEN.md`

Order:

1. prove read-side activity in artifacts
2. restore exact `smoke`
3. keep `6` bits as the primary optimization target
4. use `8` bits as the current correctness ceiling
5. only then reopen prompt-shape/runtime hardening
6. rerun the ladder from the corrected real replay baseline

## Stop Conditions

Stop Phase 3 quickly if any of these happen:

- exact `smoke` cannot be restored on the real replay path
- exact `retrieval` breaks while correcting `smoke`/`instruct`
- strict JSON `instruct` remains broken after replay-fidelity corrections
- integration scope grows faster than the measured value

## Immediate Next Step

Work the replay-fidelity gap on the real `vec` path:

1. isolate why `smoke` drifts from exact `OK`
2. keep `6` bits as the primary optimization target
3. keep `retrieval`, `instruct`, and `context_fill` as regression guards while correcting `smoke`
4. use `8` bits only as the current correctness ceiling
5. rerun `8k` before touching `16k/32k` again
