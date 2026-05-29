# Phase XI: Token Compression Runtime Wiring

Status: active

## Objective

Wire the Phase X token-compression primitives into runtime capture through a contract-neutral, shadow-first rollout.

Phase XI is not a new reducer-expansion phase. It decides how the existing internal reducer metadata and budget-aware assembler can be exercised near live capture without changing model-visible output, public telemetry, replay behavior, or schema contracts until gates prove the path is safe.

## Current Baseline

Normal command capture remains:

1. run the system command
2. combine stdout and stderr
3. optionally run native command-aware reduction through `CX_NATIVE_REDUCE`
4. clip by the configured character and line budgets
5. return the clipped text and existing capture statistics

Phase X added internal primitives only:

- reducer metadata
- fixture-backed recall checks for test output and diffs
- budget-aware section assembly with omission records

Those primitives were intentionally not promoted into normal capture output.

## Rollout Contract

The Phase XI rollout rule is conservative:

- default capture behavior stays unchanged
- shadow assembly may compute an internal candidate but must not feed the model
- shadow assembly must not write public telemetry, JSON logs, schemas, quarantine records, or replay artifacts
- any future public surface must be additive, nullable where appropriate, fixture-backed, and documented before use
- rollback must restore the existing capture path by disabling the explicit opt-in

The initial private gate is `CX_CAPTURE_ASSEMBLY_SHADOW=1`. It runs typed assembly alongside the current pipeline and discards the result.

## Non-Goals

- no default prompt replacement in the first runtime slice
- no public telemetry keys for omission records yet
- no schema, policy, quarantine, or replay contract changes
- no LLM summaries as sole evidence
- no hash-only source references without guaranteed expansion
- no storage compression, SIMD, or assembly work in capture/control-plane paths

## Acceptance Gates

Before typed assembly can affect model-visible prompt text, every affected command class must pass:

- `default_unchanged`: default command capture output is byte-for-byte compatible with the existing pipeline
- `critical_span_recall`: required fixture spans survive reduction and assembly
- `lossiness_declared`: semantic extraction, lossy output, and uncertainty are labeled
- `fallback_safe`: high-uncertainty paths retain source evidence or fall back to current capture
- `bounded_cost`: shadow work is env-gated and avoids hidden downloads or slow disk scans
- `contract_neutral`: public JSON/log/schema/telemetry fixtures remain unchanged unless an additive contract slice explicitly changes them
- `rollback_simple`: disabling the opt-in returns to `run -> reduce -> clip`

## Slice Plan

1. `p11_spec_rollout_contract`: define the Phase XI rollout contract, work queue, and public-surface boundaries.
2. `p11_shadow_assembly`: run typed assembly in a private shadow path and prove it does not change returned capture text, status, or stats.
3. `p11_opt_in_profile`: add an explicit opt-in prompt profile for narrow command classes only after shadow evidence is fixture-backed.
4. `p11_measurement_gates`: collect non-public fixture measurements for omission counts, recall, and size deltas.
5. `p11_rollout_decision`: document whether to keep opt-in only, expand additively, or defer runtime wiring.

## Current State

Slice 1 is complete.

Slice 2 is complete. A private `CX_CAPTURE_ASSEMBLY_SHADOW=1` path builds a typed assembly candidate from command/status, reducer metadata, and reduced output, then discards it. Focused unit coverage verifies the candidate keeps command/status evidence, promotes high-uncertainty output evidence, and keeps metadata as context rather than promoted evidence. Normal capture output and public telemetry remain unchanged.
