# TurboQuant Fidelity Checkpoint

Branch: `cx/turboquant-spike`
Checkpoint: `tq_p2_slice12.patch`

## What Changed

This checkpoint fixes one concrete Phase 2 bug and adds one new diagnostic:

- write-side sidecar rows now derive `slot` and `strm` from `v_idxs`
- read-side decode can now be compared numerically against the raw `V` tensor

The previous prototype recorded local row ordinals during sidecar encode. That was incorrect for the active `!v_trans` host path because `v_idxs` already carries absolute cache identities.

## What It Proved

The row-identity bug was real.

After fixing sidecar row tagging:

- direct smoke probing on the host-backed path returns exact `OK`
- read-side tracing still shows `turboquant_read` executing
- the fixed suite no longer fails immediately at the first exact-output check

## Current Validation Result

Artifact:

- `docs/TURBOQUANT_PHASE2_CHECK.json`

Host path:

- `--flash-attn on`
- `--no-kv-offload`

Prompt results under TurboQuant:

- `smoke`: pass
- `context_fill`: pass on non-empty output only
- `retrieval`: fail
- `instruct`: fail

Observed degraded outputs:

- retrieval:
  - expected: `TURBO-314159`
  - observed: `TUR://://://://://://://://://://://://://://`
- instruct:
  - expected: exact JSON object
  - observed: malformed JSON fragment

## Numeric Decode Error

The new read-side diagnostic reports mean/max absolute error per layer while decoding from the sidecar.

Observed range on the retrieval probe:

- mean absolute error:
  - best layers: about `0.012`
  - worst layers: about `0.426`
- max absolute error:
  - worst layer observed: about `10.387`

Interpretation:

- the mapping bug is fixed
- the current codec path is still too lossy for exact retrieval and strict structured output
- the remaining blocker is codec fidelity, not graph attachment

## Current Phase 2 Meaning

Phase 2 now has three stable facts:

1. sidecar write attachment is real
2. sidecar read attachment is real
3. the current `tq_v0` grouped quantize/dequantize math is not quality-neutral

That means Phase 2 should not advance to raw-`V` shrinkage from this state.

## Next Step

The next correct move is codec improvement, not more routing work.

Priority order:

1. reduce decode error before any further memory-eviction attempt
2. test smaller/finer grouping or stronger scaling rules
3. keep the fixed prompt suite as the hard gate for:
   - `retrieval`
   - exact JSON output
