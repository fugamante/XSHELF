# TurboQuant Execution Boundary

Branch: `cx/turboquant-spike`
Status: confirmed Phase 2 implementation constraint
Upstream target: `llama.cpp` `a1cfb64`

## Finding

The first compile-clean patch slice proved the config and fallback plumbing, but it also exposed the real execution boundary for TurboQuant work:

- `llama_kv_cache::cpy_v(...)` and `llama_kv_cache::get_v(...)` are graph-construction functions
- the `ggml_tensor *` values handled there are symbolic graph nodes, not host-materialized `V` payloads
- therefore a true `tq_v0` transform cannot be implemented there as plain C++ row-walking code

This is not a design preference. It is a backend execution fact.

## What This Means

The next codec-bearing step must use one of these routes:

1. GGML custom op
   - insert a custom op on the `V` path
   - execute quantize/dequantize when tensor data exists during graph execution
2. later memory/update stage
   - move compression to a point after graph evaluation where concrete `V` rows are available
3. backend-specific kernel path
   - larger scope
   - not appropriate for the first codec-bearing slice

For Phase 2, route `1` is the correct next move.

## Confirmed GGML Surface

Pinned upstream exposes custom-op hooks in:

- `/tmp/cx_llama_cpp/ggml/include/ggml.h:2484`

Relevant APIs:

- `ggml_map_custom1`
- `ggml_map_custom2`
- `ggml_map_custom3`
- `ggml_custom_4d`

These are the first realistic integration points for a `tq_v0` prototype.

## Phase 2 Implication

The previous patch plan assumed write-side codec work could be inserted directly inside `cpy_v()`.

That assumption is too optimistic.

The corrected sequence is:

1. keep `cpy_v()` as the gate and dispatch boundary
2. insert a custom-op scaffold for the `V` path
3. implement `tq_v0` quantize/dequantize inside that execution-bearing path
4. keep baseline fallback active for:
   - unsupported layouts
   - unsupported devices
   - unsupported tensor types
   - any non-CPU path until proven otherwise

## Device Risk

Custom ops exist in GGML CPU execution.

That does not imply safe support on every backend path, especially:

- Metal
- mixed backend scheduling
- offloaded KV/cache paths

So the next slice should assume:

- CPU-only TurboQuant prototype path first
- explicit fallback elsewhere

## Immediate Next Patch Goal

The next backend slice should add:

- a `tq_v0` custom-op scaffold on the `V` write/read path
- CPU-only support gating
- explicit fallback reason for non-CPU execution

It should not yet attempt:

- fused compressed attention
- Metal-native custom kernel support
- `K` compression
