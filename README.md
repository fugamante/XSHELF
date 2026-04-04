# XSHELF (formerly CX)

`cx` is a deterministic, Rust-first LLM dev runtime for repositories.

Project naming note:
- `XSHELF/CX` is an independent open-source project and is not affiliated with or endorsed by OpenAI.

- Canonical execution engine: `rust/cxrs`
- Canonical entrypoint: `bin/cx` (Rust-only dispatch)
- Deterministic structured commands: schema-enforced JSON + quarantine/replay on failure
- Unified execution pipeline: capture -> internal native reduction -> mandatory budgeting -> LLM -> validation -> logging
- Repo-local state and telemetry under `.codex/` (logs, schemas, tasks, quarantine, state)
- Built-in task graph and run orchestration (`task add/fanout/run/run-all`)
- Safety layer for command execution boundaries and policy visibility (`policy show`)
- Backend model: Codex by default, Ollama optional/user-selectable

## Runtime vs Development

For normal users, `CX` is a runtime tool, not a test harness.

- Normal usage does not run the full Rust test suite.
- Shell startup does not run tests.
- Runtime verification is through `./bin/cx doctor` and `./bin/cx health`.
- The full suite (`cargo test`, compat checks, guardrails, CI contract checks) is maintainer-only.

This separation is intentional: end users should get the runtime, while contributors and CI carry the validation load.

## Technical Exposé (Rust Refactor Snapshot)

This branch is actively decomposing `cxrs` from a monolithic command file into focused modules while preserving CLI behavior and contracts.

Current status:
- quality gate clean: `file_violations=0`, `function_violations=0`
- test suite passing in serial mode (`cargo test -q -- --test-threads=1`)
- command modules now consistently split into handler + internal helpers for lower coupling and easier review

Current refactor highlights:

- `src/app/mod.rs` remains the orchestrator/dispatcher (reduced substantially from initial monolith size)
- centralized runtime configuration in `src/modules/config.rs` (`AppConfig` loaded once at startup)
- command families extracted into dedicated modules:
  - `src/modules/introspect.rs` (`version`, `core`)
  - `src/modules/runtime_controls.rs` (`log-on/off`, `alert-*`, `capture-status`)
  - `src/modules/agentcmds.rs` (`cx/cxj/cxo/cxol/cxcopy/fix`)
  - `src/modules/logview.rs` (`budget`, `log-tail`)
  - `src/modules/analytics.rs` (`metrics/profile/trace/alert/worklog`)
  - `src/modules/diagnostics.rs` (`diag`, helpers)
  - `src/modules/routing.rs` (`where`, `routes`, provenance helpers`)
  - `src/modules/prompting.rs` (`prompt/roles/fanout/promptlint`)
  - `src/modules/optimize.rs` (`optimize`)
  - `src/modules/doctor.rs` (`doctor`, `health`)
  - `src/modules/schema_ops.rs` (`schema list`, `ci validate`)
  - `src/modules/settings_cmds.rs` (`state *`, `llm *`)
  - `src/modules/structured_cmds.rs` (`next`, `fix-run`, `diffsum*`, `commitjson`, `commitmsg`, `replay`)
  - `src/modules/task_cmds.rs` (`task add/list/show/claim/complete/fail/fanout/run/run-all`)
- consolidated LLM command path in `src/modules/agentcmds.rs` via shared `execute_llm_command(..., LlmMode)`

Design intent:
- keep command UX stable while shrinking coupling and improving testability
- make error paths explicit and quarantine-backed
- keep Rust as authoritative behavior for capture, schema, policy, and telemetry contracts

## Configuration Contract

`cxrs` now snapshots core environment configuration once at startup (`AppConfig`) and reuses it across modules.

Primary fields:
- budgets: `CX_CONTEXT_BUDGET_CHARS`, `CX_CONTEXT_BUDGET_LINES`, `CX_CONTEXT_CLIP_MODE`, `CX_CONTEXT_CLIP_FOOTER`
- process timeout: `CX_CMD_TIMEOUT_SECS` (default `120`)
- backend/model: `CX_LLM_BACKEND`, `CX_OLLAMA_MODEL`, `CX_MODEL`
- HTTP adapter transport policy: `CX_HTTP_PROVIDER_URL`, `CX_HTTP_PROVIDER_TOKEN`, `CX_HTTP_PROVIDER_FORMAT`, `CX_HTTP_REQUIRE_HTTPS` (default `1`), `CX_HTTP_ALLOW_LOCAL_HTTP` (default `1`), `CX_HTTP_ALLOWED_HOSTS` (optional allowlist), `CX_HTTP_TLS_PINNEDPUBKEY` (optional TLS pinning for curl transport)
- execution mode: `CX_MODE`, `CX_SCHEMA_RELAXED`
- operational toggles: `CXLOG_ENABLED`, `CXBENCH_LOG`, `CXBENCH_PASSTHRU`, `CXFIX_RUN`, `CXFIX_FORCE`, `CX_UNSAFE`

Key defaults:
- context chars: `12000`
- context lines: `300`
- run window defaults: `50`
- optimize window default: `200`
- quarantine list default: `20`

## Architecture

The runtime pipeline is unified in Rust:

1. Capture system output
2. Internal native reduction
3. Mandatory context budgeting (chars + lines)
4. LLM execution
5. Schema validation (for structured commands)
6. Quarantine on schema failure
7. Append-only JSONL logging

Structured commands are schema-enforced from `.codex/schemas/` and deterministic by default.

Repo role boundary:
- `docs/REPO_ROLE_CONTRACT.md`
- Cross-repo sync plan:
- `docs/REPO_SYNC_PLAN.md`

## Design Discipline

CX design/readability standards are enforced through guardrails and local template policy.
Current repository-enforced naming readability rule:
- max `3` segments (`2` underscores) for new file stems/functions/tests
- grandfathered allowlists for legacy names only

## Repository Layout

- `bin/cx` - single entrypoint, Rust-first dispatcher
- `rust/cxrs/src/main.rs` - module entrypoint
- `rust/cxrs/src/app/mod.rs` - command routing/orchestration
- `rust/cxrs/src/modules/*.rs` - domain modules (capture, logging, schema, tasks, policy, diagnostics)
- `cx.sh` - deprecated compatibility loader (sources `lib/cx.sh`)
- `lib/cx.sh` - thin shell shim that delegates to `bin/cx`
- `.codex/schemas/` - JSON schema registry
- `.codex/cxlogs/` - run + schema failure logs (runtime)
- `.codex/quarantine/` - invalid schema outputs (runtime)

## Versioning

`VERSION` is intentionally a single-line, machine-readable current version:

- `2026.03.05`

Human-readable release history lives in tags + changelog:

- [`v2026.02.21`](https://github.com/fugamante/cx/releases/tag/v2026.02.21) - schema hardening + strict routing baseline
- [`v2026.02.21-20260225T151634Z`](https://github.com/fugamante/cx/releases/tag/v2026.02.21-20260225T151634Z) - docs/manual snapshot milestone
- current development head tracked in [`CHANGELOG.md`](CHANGELOG.md) under `Unreleased`
- historical version definitions in [`VERSION_HISTORY.md`](VERSION_HISTORY.md)

Quick checks:

```bash
git tag --list --sort=creatordate
cat VERSION
```

## Requirements

### Runtime (required)

| Dependency | Minimum | Validated in this repo | Notes |
|---|---:|---:|---|
| OS | macOS or Linux | macOS (darwin) | Windows supported via WSL |
| `bash` | 5.0+ | 5.3.9 | Shell wrappers/bootstrap |
| `git` | 2.30+ | 2.53.0 | Repo detection, diff/log capture |
| `jq` | 1.6+ | 1.8.1 | JSON processing and compatibility scripts |
| `codex` CLI | 0.103.0+ | 0.103.0 | Default LLM backend |

### Runtime (optional)

| Dependency | Minimum | Validated in this repo | Notes |
|---|---:|---:|---|
| `ollama` | 0.17.0+ | 0.17.0 | Optional local LLM backend |

### Development / CI

| Dependency | Minimum | Validated in this repo | Notes |
|---|---:|---:|---|
| `rustc` | 1.93.1 | 1.93.1 | Canonical runtime is Rust |
| `cargo` | 1.93.1 | 1.93.1 | Build/test |
| `python3` | 3.10+ | 3.14.3 | Quality gate + helper scripts |
| `make` | 3.81+ | 3.81 | Convenience targets (`make install`, compat checks) |

### Rust crates

Rust crate dependencies are pinned in `rust/cxrs/Cargo.lock` for reproducible builds.

Development-only note:
- The `Development / CI` dependencies are for contributors and release validation.
- End users do not need to run `cargo test` or guardrail scripts to use `CX`.

## Quick Start
```bash
cd <repo-root>
./bin/cx version
./bin/cx core
./bin/cx cxo git status
```

Man page:

```bash
./bin/cx-install
man cx
```

Quick runtime verification:

```bash
./bin/cx doctor
./bin/cx health
./bin/cx core --json
./bin/cx version --json
./bin/cx diag --json --window 50
```

Backend capability note:
- runtime JSON surfaces now report typed backend experiment metadata
- current typed surfaces:
  - `core --json`
  - `version --json`
  - `diag --json`
  - `telemetry N --json`
- TurboQuant capability fields remain explicit:
  - `cx_runtime_support`
  - `selected_backend_role`
  - `memory_metric_kind`

## Lean Daily Session

Start a low-noise, quota-aware operator session:

```bash
cd <repo-root>
./bin/cx-lean-session
```

Strict gate mode (non-zero exit on warning/critical actions):

```bash
cd <repo-root>
./bin/cx-lean-session --strict
```

Note:
- `cx-lean-session` does not change broker policy implicitly.
- gate summaries now include compact `task_readiness` data from `diag` / `scheduler`:
  - recommended mode
  - mixed-ready vs parallel-ready
  - wave counts and largest parallel wave
- gate summaries now include compact `task_execution` data when available:
  - latest run mode
  - halted remaining work
  - backend fallback row count
  - wave-pressure kind, suggested mode, and max queue ms
  - advice
  - typed next-action kind
  - primary next command
- Set broker policy explicitly when needed:
```bash
./bin/cx broker set --policy quota_saver
```
- Optional dynamic quota guard:
```bash
./bin/cx quota guard on --warn-pct 25 --critical-pct 10 --auto-action none
./bin/cx quota guard check 30 --json | jq .
```
- Set or clear known quota totals explicitly:
```bash
./bin/cx quota set codex 2000000
./bin/cx quota unset codex
```
- Maintain a provider-source quota catalog (tier metadata + source URLs):
```bash
./bin/cx quota catalog refresh
./bin/cx quota catalog show --json | jq .
./bin/cx quota probe 30 --json | jq .
```
- Optional automatic catalog refresh (opt-in) with stale-age policy:
```bash
./bin/cx quota catalog auto on --interval-hours 168
./bin/cx quota catalog refresh --if-stale --max-age-hours 168
./bin/cx quota catalog auto show
./bin/cx quota catalog auto off
```

## Backend Selection

`cxrs` resolves backend/model using:

1. CLI intent
2. environment variables
3. persisted state (`.codex/state.json`)
4. default (`codex`)

Examples:

```bash
./bin/cx llm show
./bin/cx llm use codex
./bin/cx llm use ollama llama3.1
./bin/cx llm unset model
```

## Backend Capability Notes

CX keeps backend experiments behind explicit capability language.

Current rule:

- provider adapters are the runtime abstraction
- backend-specific inference experiments do not become core CX behavior until they prove value and preserve adapter boundaries

Current TurboQuant reading:

- `llama.cpp` is the current codec-bearing reference backend
- `MLX` is currently documented as a comparative backend, not a codec-bearing backend
- `MLX` memory reporting should be read as:
  - `cache_nbytes`
  - `peak_memory_gb`
- `MLX` memory reporting must not be described as a direct `raw_ratio` equivalent

Reference docs:

- `docs/TURBOQUANT_SPIKE.md`
- `docs/TURBOQUANT_METRIC.md`
- `docs/TURBOQUANT_CAP_MLX.md`
- `docs/PROVIDER_ADAPTER_PLAN.md`

`llm use`/`llm set-*` now triggers an automatic quota probe notice to stderr.
For local providers (`ollama`), CX reports a local-unmetered fallback notice when provider quota cannot be resolved.

## Output Mode Resolution

CX supports deterministic output-mode resolution for human and automation paths.

Precedence:
1. CLI override (`--json` / `--text`)
2. `CX_JSON_DEFAULT`
3. `.codex/state.json` at `preferences.default_json_output`
4. Optional auto signals (`CX_JSON_AUTO=1`)
5. Command default

Inspect current decision:

```bash
./bin/cx mode
./bin/cx mode --json | jq .
```

Auto-mode examples:

```bash
CX_JSON_AUTO=1 ./bin/cx diag --window 50
CX_JSON_AUTO=1 ./bin/cx task run-all --status pending
```

## Structured Commands

Schema-enforced commands:

- `commitjson`
- `diffsum`
- `diffsum-staged`
- `next`
- `fix-run`

Schema registry inspection:

```bash
./bin/cx schema list
./bin/cx schema list --json | jq .
```

Relaxed mode override (not default):

```bash
CX_SCHEMA_RELAXED=1 ./bin/cx next git status
```

## Logging + Quarantine

Run log:

- `.codex/cxlogs/runs.jsonl`

Schema failure log:

- `.codex/cxlogs/schema_failures.jsonl`

Quarantine directory:

- `.codex/quarantine/`

Useful commands:

```bash
./bin/cx metrics 20
./bin/cx trace
./bin/cx quarantine list
./bin/cx replay <quarantine_id>
```

Telemetry health:

```bash
./bin/cx logs stats 200
./bin/cx logs stats 200 --json | jq .
./bin/cx logs stats 200 --json | jq '.critical_telemetry'
./bin/cx telemetry 50 --json | jq .
./bin/cx telemetry 50 --json | jq '.critical_telemetry'
./bin/cx diag --json --window 50 | jq .
./bin/cx diag --json --strict --window 50 | jq '.severity,.severity_reasons,.critical'
./bin/cx diag --json --window 50 | jq '.concurrency.defaults,.concurrency.observed'
./bin/cx scheduler --json --window 50 | jq .
./bin/cx scheduler --json --strict --window 50 | jq '.severity,.severity_reasons,.critical'
./bin/cx scheduler --json --window 50 | jq '.concurrency.defaults,.concurrency.observed'
./bin/cx optimize 200 --json | jq .
./bin/cx quota probe 30 --json | jq .
./bin/cx quota guard show
./bin/cx quota guard check 30 --json | jq .
```

Retry-health JSON surfaces:
- `diag --json`: top-level `retry`
- `scheduler --json`: top-level `retry`
- `optimize --json`: `scoreboard.retry_health`
- `optimize --json`: `scoreboard.timing_attribution_coverage`
- `diag --json`: top-level `concurrency`
- `scheduler --json`: top-level `concurrency`
- `diag --json`: `scheduler.rows_with_retry_attempt`, `scheduler.rows_with_queue_started_at`, `scheduler.rows_with_task_started_at`, `scheduler.rows_with_task_finished_at`
- `scheduler --json`: same scheduler timing/attempt coverage keys as `diag --json`
- contract markers: top-level `contract_version` on JSON diagnostics surfaces
- actions markers: top-level `actions_contract_version` when `--actions` is used

Contract policy:
- [`docs/CONTRACT_COMPATIBILITY.md`](docs/CONTRACT_COMPATIBILITY.md)

Expected JSON shape (key excerpts):

```json
{
  "diag": {
    "scheduler": {
      "window_runs": 50,
      "queue_ms_p95": 1200,
      "rows_with_retry_attempt": 6,
      "rows_with_queue_started_at": 6,
      "rows_with_task_started_at": 6,
      "rows_with_task_finished_at": 6
    },
    "concurrency": {
      "defaults": { "run_all_mode": "sequential", "max_workers": 1, "fairness": "round_robin" },
      "observed": { "run_all_rows": 3, "latest_run_all_mode": "mixed", "run_all_mode_counts": { "mixed": 3 } }
    },
    "retry": {
      "rows_with_retry_metadata": 8,
      "rows_after_retry_success_rate": 0.75,
      "attempt_histogram": { "1": 42, "2": 8 }
    }
  },
  "scheduler": {
    "scheduler": { "queue_rows": 20, "worker_distribution": { "w1": 10 } },
    "concurrency": {
      "defaults": { "run_all_mode": "sequential", "backend_pool": ["codex"], "halt_on_critical": false },
      "observed": { "run_all_rows": 2, "latest_run_all_mode": "parallel", "halt_on_critical_rows": 1 }
    },
    "retry": {
      "tasks_with_retry": 3,
      "tasks_retry_recovery_rate": 0.67
    }
  },
  "optimize": {
    "scoreboard": {
      "retry_health": {
        "rows_after_retry": 8,
        "rows_after_retry_success": 6,
        "tasks_recovery_rate": 0.67,
        "attempt_histogram": [[1, 42], [2, 8]]
      }
    }
  }
}
```

## Task Graph + Safety + Optimization

Stage II runtime commands:

```bash
./bin/cx task add "Implement parser hardening" --role implementer
./bin/cx task list --status pending
./bin/cx task list --json | jq .
./bin/cx task fanout "Ship release notes improvements" --from staged-diff
./bin/cx task check --json | jq .
./bin/cx task check --strict-plan --json | jq .
./bin/cx task run-plan --status pending
./bin/cx task show <task_id> | jq .
./bin/cx task run <task_id> --mode deterministic --backend codex
./bin/cx task run-all --status pending
./bin/cx task run-all --status pending --mode mixed
./bin/cx task run-all --status pending --mode parallel --max-workers 2
./bin/cx task run-all --status pending --mode parallel --strict-plan --max-workers 2
./bin/cx task run-all --status pending --mode parallel --strict-plan --plan-json | jq .
./bin/cx task run-all --status pending --mode parallel --dry-run --json | jq .
./bin/cx task run-all --status pending --mode mixed --halt-on-critical
./bin/cx task run-all --status pending --summary json | jq .
CX_TASK_HALT_ON_CRITICAL=1 ./bin/cx task run-all --status pending

./bin/cx optimize 200
./bin/cx optimize 200 --json | jq .
./bin/cx diag --json --window 50 | jq .
./bin/cx scheduler --json --window 50 | jq .
./bin/cx broker show --json | jq .
./bin/cx broker benchmark --backend codex --backend ollama --window 200 --json | jq .
./bin/cx broker benchmark --backend codex --backend ollama --window 200 --strict --min-runs 5 --json | jq .
./bin/cx broker benchmark --backend codex --backend ollama --window 200 --strict --min-runs 5 --severity warn --json | jq .

./bin/cx policy show
./bin/cx policy show --json | jq .
./bin/cx logs validate --fix=false
```

## Migration Phase III (Orchestration Modes)

Current status:
- task graph and runner are live (`task add/list/fanout/run/run-all`), with sequential default.
- orchestration modes are explicit:
  - `sequential` (default)
  - `mixed` (wave-aware scheduling with backend/fairness controls)
  - `parallel` (explicit Phase VI lane behind `--mode parallel`)
- deterministic planning is available via `task run-plan`.

Safety/determinism contracts remain unchanged:
- policy gates are still enforced for execution paths
- schema commands remain deterministic by default
- telemetry/log contracts remain append-only and validated

## Phase IV Preview (Multi-Model Tandem)

Planned next migration focus:
- broker-managed backend/model routing for tasks (`codex`, `ollama`, `auto`)
- tandem execution convergence (`first_valid`, `majority`, `judge`, `score`)
- backend pool scheduling for mixed-mode run-all with deterministic planning constraints

Design and schedule:
- `docs/PHASE_IV_MULTI_MODEL_ORCHESTRATION.md`

## Phase VI Kickoff

Initial Phase VI scope is active with explicit controls only (no default behavior switch).

Spec:
- `docs/PHASE_VI_PARALLEL_SUBSTRATE.md`
- `docs/PHASE_VI_EXECUTION_GUIDANCE.md`
- `--strict-plan` can be used with `--mode parallel` to fail fast when dependencies/resource locks would force serialized execution.
  - note: parallel tasks default to a conservative `repo:write` lock unless you set explicit resource keys.

Planned follow-up after Phase VI stabilizes:

- `docs/POST_PHASE_VI_OVERVIEW.md`

## Validation

Validation in `CX` means checking that structured outputs, logs, and runtime contracts remain consistent and machine-readable.

Runtime validation:

```bash
./bin/cx doctor
./bin/cx health
./bin/cx logs validate --fix=false
```

What these cover:
- `doctor` checks runtime prerequisites and repo-local wiring
- `health` provides a lightweight runtime status check
- `logs validate` scans `.codex/cxlogs/runs.jsonl` for JSON integrity and required telemetry fields

Schema failures are quarantined under `.codex/quarantine/`, and invalid structured outputs are prevented from silently re-entering the pipeline.

## Maintainer Validation

`cxrs-compat` workflow is manual-only (`workflow_dispatch`) while CI billing is constrained. Use this local gate before push:

```bash
./scripts/compat_local.sh --quick
# full local compat + JSON artifact
./scripts/compat_local.sh --full --out .codex/compat/latest.json
# aggregate compat across sibling repos (if present)
./scripts/compat_all.sh --quick
./scripts/compat_all.sh --full --out .codex/compat/all_latest.json
# wrapper:
./bin/cx-compat-local --quick

cd rust/cxrs
cargo fmt
cargo check
cargo test --tests
python3 tools/release_check.py --repo-root ../..

cd ../..
./test/bin_cx_entrypoint.sh
./test/provenance_tools.sh
./test/schema_registry.sh
./test/core_pipeline.sh
```

Local push guardrails:

```bash
./bin/cx-enable-githooks
# pre-commit scans staged content for local-path/PII/secrets leaks
# pre-push scans tracked content, then enforces fmt + clippy + tests
git push
```

These checks are for development and CI. They are not part of ordinary end-user runtime execution.

## Notes

- No automatic checks run during shell startup.
- Diagnostics are sent to stderr; pipeline-oriented command output remains on stdout.
- Capture is internal-native only; schema JSON outputs are never transformed.

## License

This project is licensed under the MIT License. See `LICENSE`.

## Contributing and Security

- Contributing guide: `CONTRIBUTING.md`
- Code of conduct: `CODE_OF_CONDUCT.md`
- Security reporting: `SECURITY.md`
- New contributor issue list: `docs/GOOD_FIRST_ISSUES.md`
- Contributor walkthrough: `docs/CONTRIBUTOR_WALKTHROUGH.md`
- Roadmap: `docs/ROADMAP.md`
- Release cadence: `docs/RELEASE_CADENCE.md`
- HTTP adapter TLS policy: `docs/HTTP_PROVIDER_TLS.md`
