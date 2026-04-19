# Repo Role Contract

Status: active

## Scope

This document defines responsibility boundaries between:

- `cx` (this repository)
- `cx-ops` (currently named `cx-eval-lab`)

## Canonical Responsibilities

`cx` is the canonical runtime substrate:

- execution engine and command routing
- schema enforcement and quarantine/replay
- safety/policy evaluation and policy logs
- telemetry contracts and JSON diagnostics surfaces
- orchestration engine (`task add/run/run-all`, strict-plan behavior)

`cx` owns contract definitions for:

- `diag --json`
- `scheduler --json`
- `optimize --json`
- run-log field contracts
- exported contract bundles consumed by `cx-ops` / `cx-eval-lab`

## Non-Responsibilities

`cx` does not own operator web UI concerns.
`cx` does not duplicate control-plane workflow UX that belongs in `cx-ops`.

## Promotion Policy

Components may be promoted from `cx-ops` into `cx` only when all conditions hold:

1. Runtime-critical (not UI-only).
2. Contract-stable and fixture-tested.
3. Safety and logging semantics preserved.
4. No duplicate behavior fork across repos.

## Compatibility Policy

`cx-ops` consumes `cx` JSON contracts and must not redefine their schema.
When contract drift occurs, changes must be coordinated with:

- fixture updates
- compatibility tests
- changelog notes in affected repos

The `eval-lab` export profile is fixture-backed in `cx` and must validate before bundle changes are treated as intentional:

- export path:
  - `cx contracts export --profile eval-lab --json`
- drift gate:
  - `cx contracts validate --profile eval-lab --json`
- fixture owner:
  - `rust/cxrs/tests/fixtures/eval_lab_bundle.json`

## Immediate Operating Model

Current model is selective upstream:

- keep repos separate
- upstream stable runtime-critical features incrementally
- keep UI/API iteration isolated in `cx-ops`
