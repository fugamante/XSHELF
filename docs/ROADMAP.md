# Roadmap

## Now (0-4 weeks)

- Preserve JSON contract stability on automation surfaces (`diag/scheduler/optimize/telemetry/broker`).
- Keep quality gates strict (`raw_eprintln=0`, function/file limits).
- Maintain reliability matrix coverage for backend/capture/policy permutations.
- Guard mixed-mode orchestration invariants (fairness, retry, timeout, policy).
- Maintain release hygiene (contract policy + changelog + tagged releases).

## Next (1-2 months)

- Stage the `XSHELF` rename as a compatibility migration, not a breaking command/env/state rename.
- Land dual-surface command docs/install defaults so `xshelf` is documented first while `cx` stays fully supported.
- Add richer command-level JSON outputs for diagnostics tools.
- Landed CI-level artifact reports for reliability suite failures.
- Broaden OS validation matrix (Linux-focused CI pass).
- Landed run-level concurrency telemetry refinement (`worker_count`, workers, queue/start/finish timestamps, max retry attempt) across `task run-all`, diagnostics, and telemetry surfaces.
- Landed adapter rollout policy surface; HTTP transport remains explicit opt-in and diagnostics/telemetry now expose the rollout guard.
- Expand provider adapter coverage beyond current HTTP/process parity.
- Landed exported contract-bundle maintenance for `cx-eval-lab`, including bundle ownership and fixture-backed contract drift discipline.

## Later (2+ months)

- Pluggable backend adapters beyond Codex/Ollama.
- Incremental CLI packaging/distribution improvements (Homebrew-ready metadata).
- Optional distributed execution backends (multi-process/remote workers) while preserving current log/schema contracts.
- Backend capability experiments for local inference optimization must stay isolated from core CX until they prove value and preserve adapter boundaries.
- Current backend experiment note: `docs/TURBOQUANT_SPIKE.md`
- Current backend experiment harness spec: `docs/TURBOQUANT_PHASE1.md`
- Current TurboQuant status:
  - Phase 2 scalar track closed as a value no-go
  - Phase 3 vector track closed as a value go
  - Phase 3A `MLX` comparative track is closed as `mlx_portable_go`
  - `MLX` runtime is resolved and the ladder is green through `8k`, `16k`, and `32k`
  - `MLX` comparison now records live `cache_nbytes` alongside peak memory
  - Phase 3B `MLX` capability follow-on is closed as `mlx_comparative_only`
  - post-closeout optimization on `llama.cpp` should stay optional and secondary

## Phase VI (stabilized substrate)

- Kickoff spec: `docs/PHASE_VI_PARALLEL_SUBSTRATE.md`
- Execution-guidance contract: `docs/PHASE_VI_EXECUTION_GUIDANCE.md`
- Keep single-worker execution as default; introduce explicit parallel plans only via task orchestration controls.
- Preserve deterministic schema behavior under mixed/parallel scheduling paths.
- Expand run-level telemetry quality checks for queue/start/finish attribution and worker-level observability.
- Current substrate coverage includes:
  - stable `task_readiness`, `task_execution`, `run_readiness`, and `list_readiness` contracts
  - operator guidance surfaced across `task`, `doctor`, diagnostics, telemetry, optimize, and lean-session surfaces
- Cross-phase review completed:
  - `docs/POST_PHASE_VI_OVERVIEW.md`

## Phase VII (milestone complete)

- Planning spec: `docs/PHASE_VII_BUDGET_AWARE_ORCHESTRATION.md`
- Work queue: `docs/PHASE_VII_WORK.json`
- Goal: make CX choose the cheapest sufficient path without allowing quality drift.
- First contract focus:
  - `next_action.cost_class`
  - `next_action.reasoning_required`
  - `next_action.quality_risk`
  - `next_action.escalates_if`
- Current implementation state:
  - Slice 1 merged:
    - cost/quality metadata on `next_action`
  - Slice 2 merged:
    - explicit `reasoning_gate`
  - Slice 3 merged:
    - compact recent-context carry-forward on execution and preflight surfaces
  - Slice 4 merged:
    - measurable Phase VII efficiency and reuse metrics on telemetry/diagnostic surfaces
  - Follow-on merged:
    - explicit `phase7_bias` on `task_execution`
    - higher-level action rationales preserve and explain that bias
    - severity-preserving, cost-aware action ordering on diagnostics and optimize action surfaces
- Capability lanes locked for Phase VII:
  - reasoning gate
  - cheap structured action router
  - low-cost defaults
  - context carry-forward
- Phase VII milestone result:
  - CX now makes budget-aware guidance explicit and typed without silently lowering quality bars
  - the next decision is whether to stop here at a coherent milestone or continue into further policy tuning

## Guardrails

- Rust remains canonical runtime.
- Bash remains compatibility/bootstrap.
- Structured commands stay schema-enforced.
- Logging contract changes require tests + changelog.
- Mixed-mode orchestration must keep policy boundaries and deterministic schema behavior.
- Multi-model routing must preserve schema determinism, quarantine replayability, and stable log contracts.
