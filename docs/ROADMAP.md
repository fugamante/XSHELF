# Roadmap

## Now (0-4 weeks)

- Preserve JSON contract stability on automation surfaces (`diag/scheduler/optimize/telemetry/broker`).
- Keep quality gates strict (`raw_eprintln=0`, function/file limits).
- Maintain reliability matrix coverage for backend/capture/policy permutations.
- Guard mixed-mode orchestration invariants (fairness, retry, timeout, policy).
- Maintain release hygiene (contract policy + changelog + tagged releases).

## Next (1-2 months)

- Add richer command-level JSON outputs for diagnostics tools.
- Add CI-level artifact reports for reliability suite failures.
- Improve task orchestration ergonomics (`task show`/`task run` UX polish).
- Broaden OS validation matrix (Linux-focused CI pass).
- Add run-level concurrency telemetry refinement (`worker_id`, queue/start/finish timestamps, attempt) for SLO reporting.
- Begin Phase VI (parallel execution substrate) on top of mixed-mode scheduler.
- Lock adapter rollout policy and keep HTTP transport opt-in through Phase VI early cycles.
- Expand provider adapter coverage beyond current HTTP/process parity.

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
  - active next step is comparative backend follow-through (`MLX`)
  - `MLX` runtime is resolved and first `8k` parity pass is green
  - `MLX` ladder is also green through `16k` and `32k`
  - post-closeout optimization on `llama.cpp` should stay optional and secondary

## Phase VI Kickoff (current)

- Detailed kickoff spec: `docs/PHASE_VI_PARALLEL_SUBSTRATE.md`
- Keep single-worker execution as default; introduce explicit parallel plans only via task orchestration controls.
- Preserve deterministic schema behavior under mixed/parallel scheduling paths.
- Expand run-level telemetry quality checks for queue/start/finish attribution and worker-level observability.
- Require parity + reliability suites green before each Phase VI increment merges.

## Guardrails

- Rust remains canonical runtime.
- Bash remains compatibility/bootstrap.
- Structured commands stay schema-enforced.
- Logging contract changes require tests + changelog.
- Mixed-mode orchestration must keep policy boundaries and deterministic schema behavior.
- Multi-model routing must preserve schema determinism, quarantine replayability, and stable log contracts.
