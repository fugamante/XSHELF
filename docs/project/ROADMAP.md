# Roadmap

## Now (0-4 weeks)

- Preserve JSON contract stability on automation surfaces (`diag/scheduler/optimize/telemetry/broker`).
- Keep quality gates strict (`raw_eprintln=0`, function/file limits).
- Maintain reliability matrix coverage for backend/capture/policy permutations.
- Landed additional HTTP adapter reliability coverage for timeout and policy-block permutations.
- Landed mixed-mode orchestration invariants on `task run-all` and `task_execution`, including summary-level accounting and timing checks.
- Maintain release hygiene (contract policy + changelog + tagged releases).

## Next (1-2 months)

- Stage the `XSHELF` rename as a compatibility migration, not a breaking command/env/state rename.
- Land dual-surface command docs/install defaults so `xshelf` is documented first while `cx` stays fully supported.
- Landed richer command-level JSON outputs for diagnostics tools, including contract-backed `broker show --json`.
- Landed CI-level artifact reports for reliability suite failures.
- Land Linux-focused CI pass on PR/push while keeping full Linux+macOS compat on manual dispatch.
- Landed run-level concurrency telemetry refinement (`worker_count`, workers, queue/start/finish timestamps, max retry attempt) across `task run-all`, diagnostics, and telemetry surfaces.
- Landed adapter rollout policy surface; HTTP transport remains explicit opt-in and diagnostics/telemetry now expose the rollout guard.
- Landed one contract-safe HTTP request-profile expansion on the existing `http-curl` boundary:
  - OpenAI-compatible HTTP JSON request/response profile
  - request profile remains behind explicit opt-in rollout policy
  - telemetry and reliability coverage landed with the same bundle
- Landed custom CA bundle support on the same `http-curl` boundary:
  - `CX_HTTP_CA_BUNDLE` now maps to curl `--cacert`
  - `core` / `version` diagnostics expose whether a CA bundle is configured
- Landed mTLS client-auth support on the same `http-curl` boundary:
  - `CX_HTTP_CLIENT_CERT` now maps to curl `--cert`
  - `CX_HTTP_CLIENT_KEY` now maps to curl `--key`
  - `core` / `version` diagnostics expose whether client cert/key are configured
- Landed explicit HTTP TLS posture and transport hardening controls on the same `http-curl` boundary:
  - `core` / `version` now expose a compact `http_tls_posture` object
  - added `CX_HTTP_TLS_MIN_VERSION` for explicit TLS version floor control
  - added `CX_HTTP_FOLLOW_REDIRECTS` and `CX_HTTP_MAX_REDIRECTS` for explicit redirect policy control
- Landed explicit HTTP auth profiles on the same `http-curl` boundary:
  - `bearer` remains the default profile
  - added `basic` auth profile
  - added explicit custom-header auth profile
  - `core` / `version` now expose auth mode and header name without exposing secret values
- Landed HTTP auth secret-source handling on the same `http-curl` boundary:
  - added file-backed secret inputs for bearer token, header value, and basic password
  - `core` / `version` now expose auth secret source (`env|file|off`) without exposing secret values
  - Unix secret files now require restrictive permissions
- Prefer request-profile expansion over broad adapter proliferation:
  - keep `codex-cli` and `ollama-cli` as stable process defaults
  - keep `http-curl` behind explicit opt-in rollout policy
  - require telemetry + reliability coverage before any broader adapter matrix growth
- Landed exported contract-bundle maintenance for `cx-eval-lab`, including bundle ownership and fixture-backed contract drift discipline.

## Later (2+ months)

- Pluggable backend adapters beyond Codex/Ollama.
- Incremental CLI packaging/distribution improvements (Homebrew-ready metadata).
- Optional distributed execution backends (multi-process/remote workers) while preserving current log/schema contracts.
- Backend capability experiments for local inference optimization must stay isolated from core XSHELF until they prove value and preserve adapter boundaries.
- Current backend experiment note: `docs/turboquant/TURBOQUANT_SPIKE.md`
- Current backend experiment harness spec: `docs/turboquant/TURBOQUANT_PHASE1.md`
- Current TurboQuant status:
  - Phase 2 scalar track closed as a value no-go
  - Phase 3 vector track closed as a value go
  - Phase 3A `MLX` comparative track is closed as `mlx_portable_go`
  - `MLX` runtime is resolved and the ladder is green through `8k`, `16k`, and `32k`
  - `MLX` comparison now records live `cache_nbytes` alongside peak memory
  - Phase 3B `MLX` capability follow-on is closed as `mlx_comparative_only`
  - post-closeout optimization on `llama.cpp` should stay optional and secondary

## Phase VIII (active implementation)

- Planning spec: `docs/orchestration/PHASE_VIII_LOCAL_MODEL_SUBSTRATE.md`
- Work queue: `docs/orchestration/PHASE_VIII_WORK.json`
- Goal: turn local model selection into a typed lifecycle substrate for MLX and other local backends.
- Discovery basis:
  - oMLX shows the value of treating Apple-local models as lifecycle objects rather than one-off model strings.
  - XSHELF already supports `mlx` through `mlx-lm`, but model management is currently limited to a selected model string.
- First contract focus:
  - local model registry
  - alias resolution
  - cheap inspect/accounting
  - typed runtime capabilities
  - registry-aware smoke/benchmark profiles
  - explicit resident-server opt-in through existing adapter boundaries
- Current implementation state:
  - Slice 1 landed:
    - `.codex/local_models.json` registry
    - `xshelf llm models list|add|inspect|remove`
    - deterministic JSON/text outputs with focused integration coverage
- Guardrail:
  - do not claim persisted KV-cache restore, batching, VLM, embedding, or reranker support unless the selected backend exposes evidence for those capabilities.

## Phase VI (stabilized substrate)

- Kickoff spec: `docs/orchestration/PHASE_VI_PARALLEL_SUBSTRATE.md`
- Execution-guidance contract: `docs/orchestration/PHASE_VI_EXECUTION_GUIDANCE.md`
- Keep single-worker execution as default; introduce explicit parallel plans only via task orchestration controls.
- Preserve deterministic schema behavior under mixed/parallel scheduling paths.
- Expand run-level telemetry quality checks for queue/start/finish attribution and worker-level observability.
- Current substrate coverage includes:
  - stable `task_readiness`, `task_execution`, `run_readiness`, and `list_readiness` contracts
  - operator guidance surfaced across `task`, `doctor`, diagnostics, telemetry, optimize, and lean-session surfaces
- Cross-phase review completed:
  - `docs/orchestration/POST_PHASE_VI_OVERVIEW.md`

## Phase VII (milestone complete)

- Planning spec: `docs/orchestration/PHASE_VII_BUDGET_AWARE_ORCHESTRATION.md`
- Work queue: `docs/orchestration/PHASE_VII_WORK.json`
- Goal: make XSHELF choose the cheapest sufficient path without allowing quality drift.
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
  - XSHELF now makes budget-aware guidance explicit and typed without silently lowering quality bars
  - the next decision is whether to stop here at a coherent milestone or continue into further policy tuning

## Guardrails

- Rust remains canonical runtime.
- Bash remains compatibility/bootstrap.
- Structured commands stay schema-enforced.
- Logging contract changes require tests + changelog.
- Mixed-mode orchestration must keep policy boundaries and deterministic schema behavior.
- Multi-model routing must preserve schema determinism, quarantine replayability, and stable log contracts.
