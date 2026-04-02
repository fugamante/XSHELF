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
- current cumulative sidecar artifact: `patches/tq_p2_slice4.patch`
- upstream analysis checkout: pinned local analysis checkout
- compile check:
  - `scripts/turboquant_phase2.sh build-check <llama.cpp checkout> <build dir>`

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

## Execution Boundary Update

Phase 2 now has one confirmed implementation constraint:

- `cpy_v()` and `get_v()` are graph-build boundaries, not host-data mutation points
- real codec work must happen through a GGML execution-bearing path, most likely a custom op

That constraint is documented in:

- `docs/TURBOQUANT_EXEC.md`

## Current Execution Scaffold

The next Phase 2 checkpoint is now compile-clean:

- cumulative artifact: `patches/tq_p2_slice2.patch`

What it adds:

- identity `ggml_map_custom1` scaffold on the `V` path
- host/CPU-only activation
- explicit fallback for unsupported backend/layout paths

What it still does not add:

- any real TurboQuant codec math
- any memory win
- any quality change

## Current Codec Simulation

The next cumulative checkpoint is:

- `patches/tq_p2_slice3.patch`

What it adds:

- CPU-only group-wise quantize/dequantize simulation on the `V` write path
- use of configured:
  - `group_size`
  - `codebook_bits`
- simulated per-layer byte estimates

What it does not yet prove:

- compressed cache residency
- read-side dequant from stored compressed payload
- end-to-end memory reduction in the actual KV store

## Current Sidecar Slice

The current cumulative backend checkpoint is:

- `patches/tq_p2_slice4.patch`

What it adds:

- host-side persistent sidecar buffers for `V`
  - packed payload bytes
  - per-group scales
  - per-row offsets and counts
- explicit row accounting:
  - `rows_written`
  - `rows_bypassed`
  - `payload_bytes`
  - `scale_bytes`
- lazy sidecar sync from the host-backed `V` cache on the validated CPU path

What it still does not add:

- read-side reconstruction from the sidecar
- raw `V` eviction after successful encode
- measured KV memory reduction in the backing cache itself

## Current Sidecar Decode Slice

The current cumulative backend checkpoint is:

- `patches/tq_p2_slice5.patch`

What it adds:

- read-side reconstruction from the sidecar on the validated `!v_trans` host path
- explicit fallback when the expected sidecar rows are missing
- end-to-end parity checks against the fixed prompt suite using sidecar decode

What it still does not add:

- raw `V` eviction after successful encode
- real backing-cache shrinkage
- GPU/device execution

## Current Instrumentation Slice

The current cumulative backend checkpoint is:

- `patches/tq_p2_slice7.patch`

What it adds:

- sidecar-vs-simulated-vs-raw byte accounting helpers
- env-gated sidecar reporting:
  - `LLAMA_TQ_REPORT=1`
- validation-script passthrough for backend flags:
  - `--extra-args`
  - `CX_TURBOQUANT_VALIDATE_EXTRA_ARGS`

What it proves:

- the default local validation path still passes at `8k`
- the host-backed activation path is stable under:
  - `--flash-attn on`
  - `--no-kv-offload`
- the fixed prompt suite passes on that host-backed path
- direct sidecar reporting shows:
  - `raw_ratio=25.78%`
  - `sim_ratio=100.00%`

What it does not yet prove:

- raw `V` eviction safety
- backing-cache shrinkage after eviction

## Validation Checkpoint

The first fixed-prompt validation artifact now exists:

- `docs/TURBOQUANT_PHASE2_CHECK.json`

Validation setup:

- backend binary: patched `llama-cli` from the Phase 2 analysis checkout
- model: local `llama3.1:latest` GGUF asset
- context: `8k`
- compared modes:
  - baseline
  - `--turboquant-enable --turboquant-group-size 64 --turboquant-codebook-bits 8`

Observed result:

- all checked prompts passed in both modes:
  - `smoke`
  - `context_fill`
  - `retrieval`
  - `instruct`
- output parity held on the exact-value checks:
  - `OK`
  - `TURBO-314159`
  - required JSON object

Observed runtime deltas at `8k`:

- `smoke`
  - prompt throughput delta: `+3.90%`
  - decode throughput delta: `+7.25%`
  - wall delta: `-1330 ms`
- `context_fill`
  - prompt throughput delta: `+0.23%`
  - decode throughput delta: `-2.61%`
  - wall delta: `-10 ms`
- `retrieval`
  - prompt throughput delta: `+0.64%`
  - decode throughput delta: `+2.17%`
  - wall delta: `0 ms`
- `instruct`
  - prompt throughput delta: `-0.31%`
  - decode throughput delta: `-0.81%`
  - wall delta: `0 ms`

Interpretation:

- the CPU-only codec simulation appears quality-neutral on the fixed prompt set at `8k`
- runtime overhead is present but small
- this is enough evidence to continue Phase 2
- it is not yet evidence of memory benefit, because storage has not changed

The first sidecar-residency slice was then validated against the same prompt set and also passed all checks at `8k`.

The first sidecar-decode slice was then validated against the same prompt set and also passed all checks at `8k`, including exact output parity on:

- `smoke`
- `retrieval`
- `instruct`

The current instrumentation slice now confirms:

- the default local run path still passes the fixed suite at `8k`
- the host-backed active path also passes the fixed suite at `8k`
- the sidecar byte ratio on the active host path is approximately `25.78%` of raw `V`
- the packed sidecar bytes currently match the simulated estimate exactly

## Immediate Next Step

The custom-op boundary exists, the codec simulation is validated, and the branch now has host-side sidecar residency plus read-side decode on the validated path. The next practical step is:

1. decide whether raw `V` eviction should be prototyped behind a stricter fallback gate
2. if yes, gate it to the validated host path only:
   - `--flash-attn on`
   - `--no-kv-offload`
3. re-run the fixed suite and compare:
   - parity
   - runtime delta
   - host memory vs sidecar residency

Current branch artifacts:

- touchpoint map: `docs/TURBOQUANT_TOUCH.md`
- codec contract: `docs/TURBOQUANT_CODEC.md`
- execution-boundary note: `docs/TURBOQUANT_EXEC.md`
- sidecar storage contract: `docs/TURBOQUANT_SIDECAR.md`
- machine-readable prototype contract: `docs/TURBOQUANT_PROTO.json`
- function-level patch plan: `docs/TURBOQUANT_PATCH.md`
- machine-readable worklist: `docs/TURBOQUANT_WORK.json`
- helper script: `scripts/turboquant_phase2.sh`
- validation script: `scripts/turboquant_phase2_validate.sh`
- patch artifact: `patches/tq_p2_slice1.patch`
- execution scaffold artifact: `patches/tq_p2_slice2.patch`
- codec simulation artifact: `patches/tq_p2_slice3.patch`
- sidecar residency artifact: `patches/tq_p2_slice4.patch`
- sidecar decode artifact: `patches/tq_p2_slice5.patch`
- instrumentation artifact: `patches/tq_p2_slice7.patch`
- validation artifact: `docs/TURBOQUANT_PHASE2_CHECK.json`
