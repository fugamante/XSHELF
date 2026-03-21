# Phase VI: Parallel Substrate Kickoff

Status: active kickoff (sequential default preserved)

## Objective

Evolve task orchestration from strictly sequential execution to parallel-ready scheduling while preserving deterministic contracts, policy boundaries, and telemetry integrity.

## Non-Goals (kickoff)

- no default switch to parallel execution
- no relaxation of schema determinism
- no reduction of safety policy checks

## Entry Constraints

- Rust remains canonical execution path (`bin/cx` -> `cxrs`)
- schema commands remain deterministic unless explicitly relaxed
- log contract remains append-only and JSON-validatable

## Initial Workstream

1. Keep `task run-all` sequential default and mixed-mode controls stable.
2. Add explicit parallel lane (`--mode parallel`) with optional strict planning gate (`--strict-plan`).
3. Expand telemetry quality checks for queue/start/finish timing fields.
4. Validate fairness and retry behavior under backend pool + caps.
5. Keep adapter HTTP path opt-in only during early Phase VI.

## Merge Gate (Phase VI increments)

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings -D clippy::too_many_arguments`
- `cargo test --tests -- --test-threads=1`
- `./bin/cx logs validate --strict`
- no regression in schema failure/quarantine behavior

## Operator Checks

```bash
./bin/cx diag --json --window 50 | jq .
./bin/cx scheduler --json --window 50 | jq .
./bin/cx telemetry 200 --json | jq .
./bin/cx task run-all --status pending --mode parallel --max-workers 2 --json | jq .
./bin/cx task run-all --status pending --mode parallel --strict-plan --max-workers 2
./bin/cx task run-all --status pending --mode parallel --strict-plan --plan-json | jq .
./bin/cx task run-all --status pending --mode parallel --dry-run --json | jq .
```
