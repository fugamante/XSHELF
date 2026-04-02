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

## Tuning Outcome

The first tuning pass has now been exhausted and recorded as a negative result.

Observed sweeps:

- group size:
  - `64`
  - `32`
  - `16`
  - `8`
- codebook bits:
  - `8`
  - `10`
- affine min/max group quantizer prototype:
  - checkpoint: `patches/tq_p2_slice13.patch`

Result:

- all tested group sizes kept the same outcome:
  - `smoke`: pass
  - `context_fill`: pass by non-empty rule only
  - `retrieval`: fail
  - `instruct`: fail
- increasing codebook width from `8` to `10` did not change the failure shape
- the affine min/max prototype also failed:
  - retrieval remained corrupted
  - strict JSON output remained corrupted

Meaning:

- this is not primarily a zero-point issue
- this is not primarily a group-size issue
- the current grouped scalar quantizer family is not strong enough for exact-task Phase 2 goals

Updated next step:

1. stop tuning the current scalar codec family
2. move to a stronger prototype shape:
   - residual quantization
   - rotation/projection before quantization
   - or a higher-fidelity fallback mode that proves a true quality ceiling

## Stronger Codec Outcome

The next stronger scalar checkpoint has also been tested and recorded.

Prototype:

- two-stage residual scalar quantization
- checkpoint: `patches/tq_p2_slice14.patch`

What changed:

- first scalar pass quantizes each group
- a second scalar pass quantizes the residual left by the first pass
- decode reconstructs `out0 + out1`

Result:

- retrieval still failed
- strict JSON still failed
- measured decode error increased on many layers instead of decreasing
- generation throughput also degraded sharply on the targeted probes

Meaning:

- “more scalar stages” is not enough
- the remaining fidelity gap is not plausibly fixable by continuing scalar-only variants of this prototype family

Updated Phase 2 conclusion:

- stop scalar-only codec exploration on this branch state
- the next meaningful experiment must introduce a different representation class:
  - rotation/projection
  - structured residual scheme closer to the paper’s intent
  - or a deliberately higher-fidelity ceiling path to measure whether exact-task parity is still reachable at all

## Projection-Assisted Outcome

The first projection-assisted prototype has now also been tested and recorded.

Prototype:

- per-group normalized Walsh-Hadamard transform before scalar quantization
- inverse Walsh-Hadamard after decode
- checkpoint: `patches/tq_p2_slice15.patch`

Result:

- retrieval still failed with the same corrupted secret-code shape
- strict JSON still failed with the same malformed-object shape
- per-layer decode error remained in roughly the same range as the earlier affine scalar codec

Meaning:

- simple decorrelation alone is not enough on this path
- the remaining quality gap is not solved by:
  - scalar tuning
  - scalar residual stacking
  - scalar quantization in a Hadamard-rotated basis

Updated boundary:

- Phase 2 should stop trying “simple codec swaps” within the same complexity band
- the next real experiment must be closer to the paper’s structure:
  - online vector quantization with a stronger learned/structured codebook path
  - or a deliberate high-fidelity ceiling experiment to determine whether exact-task parity is reachable at all before deeper backend work
