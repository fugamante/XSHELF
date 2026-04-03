# TurboQuant Spike (Experimental)

Branch: `cx/turboquant-spike`
Status: active (`Phase 0` complete, `Phase 1` complete, `Phase 2` closed, `Phase 3` closed)
Owner: CX runtime

## Objective

Evaluate whether TurboQuant-style KV-cache compression can become a backend capability surfaced by CX without changing CX's core role as an orchestration/runtime layer.

This is not a prompt-compression feature. It is a backend inference optimization experiment.

## Architectural Position

TurboQuant belongs below the current CX control plane:

- CX today:
  - prompt assembly
  - provider selection
  - schema enforcement
  - budgeting/policy/telemetry
- TurboQuant target layer:
  - KV-cache storage format
  - attention read path
  - backend kernels / serving runtime

Conclusion:
- do not implement TurboQuant directly in core CX command logic
- implement it as a backend capability surfaced through CX adapter/runtime metadata

## Why Investigate It

- reduce KV-cache memory pressure for long-context local inference
- improve throughput for local backends under context-heavy workloads
- give CX a way to benchmark backend memory/latency/quality tradeoffs, not just prompt-level efficiency

## Non-Goals

- no attempt to "compress prompts" with TurboQuant
- no direct modification of Codex CLI behavior
- no default-path change for existing CX providers
- no merge of backend-kernel experiments into main without backend isolation and parity evidence

## Best Entry Points

### Option A: `llama.cpp` first

Pros:
- smaller and easier to reason about
- faster research iteration
- lower integration overhead for a spike

Cons:
- less representative of production serving/batching

### Option B: `MLX` second

Pros:
- native Apple Silicon relevance
- better fit for the actual local hardware used in this spike
- strong path for measuring real Metal-side memory and latency tradeoffs

Cons:
- different runtime internals than `llama.cpp`
- likely different cache hook points and attention-path assumptions
- higher complexity than the first `llama.cpp` feasibility slice

### Option C: `vLLM` later

Pros:
- stronger production relevance
- batching and serving behavior matter more

Cons:
- higher complexity
- not the right first target for a feasibility spike

Recommendation:
- start with `llama.cpp` for feasibility
- move to `MLX` next if the method survives baseline/prototype work
- move to `vLLM` only if quality/memory wins justify deeper investment beyond local Apple Silicon work

## CX Integration Model

If the backend experiment succeeds, CX should expose TurboQuant as capability/config, not as embedded kernel logic.

Potential CX-facing fields:

- backend capability:
  - `kv_cache_compression: turboquant`
- execution config:
  - `turboquant_enable`
  - `turboquant_group_size`
  - `turboquant_codebook_bits`
  - `turboquant_warmup_tokens`
  - `turboquant_fallback_threshold`
- telemetry:
  - `kv_cache_codec`
  - `kv_cache_mem_bytes`
  - `kv_cache_mem_saved_bytes`
  - `kv_cache_quality_delta`
  - `kv_cache_latency_delta_ms`

## Phase Plan

### Phase 0: Boundary Lock

Tasks:
- document CX/backend separation clearly
- identify candidate backend fork target
- define success metrics before any code is written

Acceptance:
- no ambiguity that this is a backend experiment, not a prompt-layer change

Completion notes:
- backend target selected: `llama.cpp`
- success metrics and provisional go/no-go thresholds locked in `docs/TURBOQUANT_PHASE0.md`
- CX integration explicitly constrained to capability/config + telemetry, not kernel logic

### Phase 1: Baseline Measurement

Tasks:
- lock first backend/model matrix
- lock reproducible baseline procedure
- lock Markdown + JSON artifact contracts
- collect first dry-run baseline

Acceptance:
- reproducible harness contract checked into docs
- baseline report path and artifact contract fixed before prototype work

Completion notes:
- Phase 1 contract locked in `docs/TURBOQUANT_PHASE1.md`
- baseline artifact version reserved as `turboquant-baseline.v1`
- first backend matrix remains deliberately narrow to avoid false precision
- prompt fixtures are checked in under `docs/tq_prompts/`
- quality probes now cover retrieval and instruction-follow behavior at `8k`, `16k`, and `32k`
- Phase 2 entry criteria are satisfied on the local `llama.cpp` path

### Phase 2: Minimal Prototype

Tasks:
- quantize V-cache only first
- keep K in safer format
- add enable/disable switch

Acceptance:
- measurable memory reduction
- no unacceptable generation degradation on small eval set

Execution contract:

- Phase 2 prototype boundaries and entry criteria are locked in `docs/TURBOQUANT_PHASE2.md`
- upstream touchpoints are pinned in `docs/TURBOQUANT_TOUCH.md`
- the first codec contract is pinned in `docs/TURBOQUANT_CODEC.md`
- the first function-level work sequence is pinned in `docs/TURBOQUANT_PATCH.md`

Completion notes:
- Phase 2 has answered the scalar-family feasibility question.
- Under snapshot-backed replay, correctness can be restored for:
  - grouped scalar
  - projection-assisted scalar
- Residual scalar remains a quality no-go.
- Report-backed value checks show the viable scalar modes reduce sidecar storage to `26.56%` of raw on the validated host path.
- That same path collapses decode throughput to roughly `~2.7 t/s` and raises wall time to roughly triple baseline on the fixed `8k` suite.
- Phase 2 is therefore closed as:
  - `scalar correctness: yes`
  - `scalar operational value: no`
- No deeper scalar-path work is recommended on this branch state.

### Phase 3: Full KV Prototype

Tasks:
- begin a new representation-class experiment rather than extending scalar quantization
- keep scope to `V`-side first until the new representation shows better value than the closed scalar track
- compare new representation against:
  - baseline
  - projection-assisted scalar reference
- only consider `K` work after the new representation clears both correctness and value gates

Acceptance:
- stable long-context quality within agreed threshold
- materially better runtime/value tradeoff than the closed scalar track

Phase 3 planning files:

- `docs/TURBOQUANT_PHASE3.md`
- `docs/TURBOQUANT_VEC.md`
- `docs/TURBOQUANT_PHASE3_WORK.json`
- `docs/TURBOQUANT_VEC_LAYOUT.md`
- `patches/tq_p3_slice1.patch`

Current Phase 3 checkpoint:

- Phase 3 is now closed as a `vector_go`
- preferred result:
  - `vec_bits6_bypass256`
- correctness holds through:
  - `8k`
  - `16k`
  - `32k`
- hardened vector reference at `8k`:
  - decode `17.68 t/s`
  - wall `12655 ms`
  - raw ratio `3.12%`
- closed scalar reference at `8k`:
  - decode `2.73 t/s`
  - wall `17022.5 ms`
  - raw ratio `26.56%`
- Phase 3 conclusion artifact:
  - `docs/TURBOQUANT_PHASE3_CLOSE.json`

### Phase 4: Read-Path Optimization

Tasks:
- avoid full-cache dequant where possible
- investigate fused dequant + attention path
- profile bandwidth and kernel overhead

Acceptance:
- end-to-end performance gain survives profiling, not just memory savings

### Phase 5: CX Capability Surface

Tasks:
- add adapter/runtime capability flags in CX
- expose TurboQuant settings only when backend advertises support
- log capability and benchmark results in CX telemetry

Acceptance:
- CX remains backend-agnostic while able to manage/measure TurboQuant-enabled runs

### Phase 3A: `MLX` Comparative Backend Track

Tasks:
- port the benchmark harness contract to `MLX`
- compare `MLX` vs `llama.cpp` on the same prompt set and artifact shape
- determine whether TurboQuant-style cache compression maps cleanly to `MLX` internals or needs a backend-specific approximation

Acceptance:
- identical benchmark/report contract across both backends
- explicit implementation-complexity assessment for `MLX`
- clear go/no-go decision before any `vLLM` work

## Risks

1. Wrong layer
- Risk: trying to force a backend kernel feature into CX prompt/runtime logic
- Control: keep implementation outside core CX until backend capability exists

2. Quality regression
- Risk: lower cache precision damages long-context generation
- Control: gate with eval fixtures and explicit thresholds

3. False wins
- Risk: memory savings offset by dequant/attention overhead
- Control: benchmark throughput and latency, not memory alone

4. Backend lock-in
- Risk: design becomes too specific to one backend
- Control: keep CX integration capability-based and adapter-scoped

5. False portability
- Risk: assuming `llama.cpp` cache-layout conclusions transfer directly to `MLX`
- Control: treat `MLX` as a comparative backend track with the same measurement contract but independent runtime assumptions

## Merge Gate For Any Future Mainline Proposal

- backend experiment isolated from core CX
- reproducible baseline + post-change benchmark data
- explicit quality threshold pass
- explicit latency/throughput justification
- CX integration remains optional and capability-driven

## Immediate Next Actions

1. Close Phase 2 in the branch record as a scalar value no-go.
2. Define the first non-scalar representation-class experiment for Phase 3.
3. Keep `MLX` and `vLLM` work out of scope until the next representation clears local `llama.cpp` value gates.
