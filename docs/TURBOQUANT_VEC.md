# TurboQuant Phase 3 Vector Contract

Branch: `cx/turboquant-spike`
Status: planned
Scope: first non-scalar representation for Phase 3

## Representation Name

- `tq_v1_vec`

## Objective

Replace the closed scalar-family path with the smallest vector/codebook experiment that can change the value curve.

This is not a production codec.
It is the first representation-class test after the Phase 2 scalar closeout.

## Core Difference From Phase 2

Phase 2 stored scalar codes per coordinate group.

Phase 3 `tq_v1_vec` stores one code per subvector against a fixed codebook.

Target idea:

- split each `V` row group into subvectors
- assign each subvector to the nearest centroid in a fixed codebook
- store code indices plus explicit scale metadata only if needed
- reconstruct on read through centroid lookup

## Initial Shape

Keep the first attempt narrow:

- `V` only
- host-backed path only
- snapshot replay retained
- fixed codebook, not online-updated
- one codebook shape only for the first slice

Initial parameters:

- row group size: `64`
- subvector size: `8`
- subvectors per group: `8`
- codebook entries: `16`
- code bits per subvector: `4`
- codebook storage: `fp16` centroids

## Why This Shape

- small enough to patch into the existing sidecar path
- structurally different from scalar quantization
- cheap enough to benchmark quickly
- closer to the paper's vector-quantization direction than the closed Phase 2 path

## Stored Components

Per layer:

- fixed centroid table
- centroid dimension metadata
- codebook size metadata

Per row:

- packed subvector codes
- offsets/counts
- explicit row identity metadata already proven in Phase 2

## Write Path Contract

At write-side capture:

1. partition each row group into subvectors
2. map each subvector to the nearest centroid
3. pack centroid ids into the sidecar payload
4. record row offsets/counts
5. preserve baseline fallback on unsupported paths

## Read Path Contract

At snapshot-backed replay:

1. unpack centroid ids
2. expand ids through the fixed centroid table
3. reassemble the original row-group layout
4. write reconstructed values into the temporary tensor used by the read-side custom op

## Success Gates

`tq_v1_vec` is a Phase 3 `go` only if all hold:

- passes the fixed `8k` suite under snapshot-backed replay
- beats the closed scalar track on runtime/value
- keeps sidecar storage at or below the scalar reference band unless runtime improves materially enough to justify slightly larger storage

## Stop Conditions

Stop the first vector attempt immediately if:

- exact retrieval breaks
- exact strict JSON breaks
- runtime stays within the scalar penalty band
- patch scope expands beyond a narrow sidecar/codebook experiment

## Comparison Baselines

Required comparisons:

- raw baseline
- projection-assisted scalar reference

Reference artifacts:

- `docs/TURBOQUANT_SCALAR_VALUE.json`
- `docs/TURBOQUANT_SCALAR_COMPARE.json`
