# TurboQuant Spike (Experimental)

Branch: `cx/turboquant-spike`
Status: active
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

### Option B: `vLLM` first

Pros:
- stronger production relevance
- batching and serving behavior matter more

Cons:
- higher complexity
- harder first implementation

Recommendation:
- start with `llama.cpp` for feasibility
- move to `vLLM` only if quality/memory wins justify deeper investment

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

### Phase 1: Baseline Measurement

Tasks:
- choose a single local model/backend
- measure:
  - memory at 8k / 16k / 32k context
  - token throughput
  - latency
  - quality proxy (perplexity / task fidelity)

Acceptance:
- reproducible baseline table checked into branch docs

### Phase 2: Minimal Prototype

Tasks:
- quantize V-cache only first
- keep K in safer format
- add enable/disable switch

Acceptance:
- measurable memory reduction
- no unacceptable generation degradation on small eval set

### Phase 3: Full KV Prototype

Tasks:
- quantize both K and V
- add per-layer/per-head toggles
- compare against baseline and simple cache quantization

Acceptance:
- stable long-context quality within agreed threshold

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

## Merge Gate For Any Future Mainline Proposal

- backend experiment isolated from core CX
- reproducible baseline + post-change benchmark data
- explicit quality threshold pass
- explicit latency/throughput justification
- CX integration remains optional and capability-driven

## Immediate Next Actions

1. Pick backend spike target (`llama.cpp` preferred first).
2. Define benchmark harness and success metrics.
3. Add a small CX doc describing how backend capability experiments plug into provider adapters without widening core command behavior.
