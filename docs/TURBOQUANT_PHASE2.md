# TurboQuant Phase 2 Minimal Prototype

Branch: `cx/turboquant-spike`
Status: active
Scope: `llama.cpp` V-cache-first feasibility slice

## Objective

Implement the smallest backend-side prototype that can answer one question cleanly:

- does online TurboQuant-style compression of V-cache produce measurable memory benefit without unacceptable quality loss on the Phase 1 prompt set?

Phase 2 is not a production path. It is a backend experiment with explicit rollback.

## Entry Criteria

Phase 2 may begin because all of the following now hold:

- Phase 1 baseline artifact exists in `docs/TURBOQUANT_ARTIFACT.json`
- Phase 1 baseline report exists in `docs/TURBOQUANT_BASELINE.md`
- prompt fixtures are fixed in `docs/tq_prompts/`
- local `llama.cpp` baseline runs are reproducible
- retrieval and instruction-follow probes both pass at `8k`, `16k`, and `32k`

## Prototype Slice

Keep the prototype narrow:

- backend target: `llama.cpp`
- cache target: `V` only
- `K` remains in the safer baseline format
- default state: disabled
- activation: explicit compile-time or runtime switch inside the backend fork

Do not implement in Phase 2:

- `K` quantization
- fused attention against compressed cache
- CX CLI flags
- CX telemetry schema changes
- multi-backend support

## Required Controls

The backend prototype must expose at least:

- `turboquant_enable`
- `turboquant_group_size`
- `turboquant_codebook_bits`
- fallback path to baseline cache behavior

If a requested shape, dtype, or device path is unsupported, fallback must be explicit and measurable.

## Required Measurements

Compare against the Phase 1 baseline using the same prompt fixtures.

For each enabled/disabled comparison run, record:

- context target
- prompt tokens
- generated tokens
- V-cache memory estimate
- end-to-end wall time
- decode tokens/sec
- derived prefill ms
- quality result for:
  - `structured_smoke`
  - `retrieval_check`
  - `instruction_follow`
- fallback triggered: `true|false`
- fallback reason: nullable string

## Success Thresholds

Phase 2 is a `go` only if all hold:

- measurable V-cache memory reduction versus baseline
- no retrieval failure on `retrieval_check`
- no JSON-shape failure on `instruction_follow`
- no catastrophic decode collapse
- fallback path remains functional and obvious under unsupported conditions

## Failure Conditions

Phase 2 is a `no-go` if any of the following occur:

- retrieval regression on the checked-in prompt set
- instruction-follow regression that breaks exact JSON output
- memory savings are negligible relative to added complexity
- decode or wall-time degradation makes the approach net-worse
- backend integration surface is materially larger than the feasibility value justifies

## Output Artifacts

Phase 2 should create:

- `docs/TURBOQUANT_PHASE2.md` updates with measured results
- a Phase 2 comparison artifact adjacent to the Phase 1 artifact
- concise notes on fallback behavior and unsupported paths
- a checked-in backend patch artifact for the first compile-clean slice

## Current Implementation Checkpoint

The first backend slice now exists as a compile-clean patch artifact against `llama.cpp` `a1cfb64`:

- patch artifact: `patches/tq_p2_slice1.patch`
- upstream analysis checkout: `/tmp/cx_llama_cpp`
- compile check:
  - `scripts/turboquant_phase2.sh build-check /tmp/cx_llama_cpp /tmp/cx_llama_cpp/build-cx-tq`

What the first slice covers:

- runtime arg plumbing for TurboQuant prototype controls
- context-to-memory parameter propagation
- KV-cache sidecar state for `V`-only experiments
- explicit write/read fallback gates
- zero codec math so baseline behavior remains the only active path

What the first slice does not cover:

- `tq_v0` write transform
- `tq_v0` dequant-on-read
- compressed memory accounting
- any graph-boundary rewrite

## Immediate Next Step

Begin the first codec-bearing patch:

1. implement `tq_v0` payload/scales sidecar write in `cpy_v()`
2. keep the baseline `ggml_set_rows()` path intact behind fallback
3. only after write-side storage exists, add dequant-on-read in `get_v()`

Current branch artifacts:

- touchpoint map: `docs/TURBOQUANT_TOUCH.md`
- codec contract: `docs/TURBOQUANT_CODEC.md`
- machine-readable prototype contract: `docs/TURBOQUANT_PROTO.json`
- function-level patch plan: `docs/TURBOQUANT_PATCH.md`
- machine-readable worklist: `docs/TURBOQUANT_WORK.json`
- helper script: `scripts/turboquant_phase2.sh`
- patch artifact: `patches/tq_p2_slice1.patch`
