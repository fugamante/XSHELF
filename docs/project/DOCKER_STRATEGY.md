# Docker Strategy

Status: active; stages 1-5 have landed as guarded floors. Remaining work is
future opt-in distribution/service packaging, not a prerequisite for normal
runtime use.

## Goal

Use Docker to improve XSHELF reproducibility, isolation, and Linux parity
without making normal runtime usage depend on containers.

## Ordering

Work this in order. Each step should stay useful on its own.

### 1. Maintainer Parity And Onboarding

Objective:
- make Linux-hosted validation and maintainer setup reproducible

Why first:
- lowest product risk
- landed through `compat_docker.sh`
- improves confidence for every later Docker step

Current floor:
- `Dockerfile`
- `scripts/compat_docker.sh --smoke`
- `scripts/compat_docker.sh --quick`
- local-build image default: `xshelf-compat:local`

Documented operations:
- first-run and warm-cache expectations
- rebuild/prune notes for stale image or bind-mounted cache state

Remaining decision:
- decide whether to publish a prebuilt maintainer image or keep local build only

Exit criteria:
- maintainers have one documented Linux parity path
- `--smoke` and `--quick` roles stay explicit and non-overlapping

### 2. CI Guardrail Harness

Objective:
- make local Docker validation mirror the core Linux CI guardrails more directly

Why second:
- it turns the maintainer image into a real push-preflight tool
- it reduces CI-only surprises before expanding Docker into runtime features

Scope:
- align local Docker checks with the Linux `cxrs-compat` workflow where sensible
- keep host-native validation authoritative for non-Linux environments

Current floor:
- `scripts/compat_docker.sh --ci`
- Linux-hosted guardrails/tests/shell regression steps that mirror the core
  `cxrs-compat` checks without depending on GitHub event payload context
- linked worktrees use an ephemeral read-only HEAD/tag metadata snapshot
  instead of mounting the parent checkout's common Git directory
- JSON report metadata under `ci_parity` records the local-vs-CI deltas that
  Docker does not claim to reproduce

Guardrail:
- do not create a second contract surface with Docker-only semantics

Exit criteria:
- maintainers can run the core Linux guardrail subset locally with one command
- differences between local Docker parity and CI are documented and intentional

### 3. Opt-In Project Task Sandbox

Objective:
- let task execution run inside a project-defined container when users need the
  project’s toolchain instead of the host’s toolchain

Why third:
- highest user-facing upside
- also the first step with real product complexity and safety tradeoffs

Scope:
- opt-in only
- repo-defined container contract, not automatic inference
- bind-mounted project workspace with explicit writable areas

Landed shape:
- project config points XSHELF at a container image or compose service
- `task run` / `task run-all` can choose host or container execution lanes
- runtime records whether execution happened on host or in container

Current floor:
- repo-scoped configuration requests a sandbox; process-only
  `CX_TASK_TRUST_SANDBOX=1` authorizes runtime and readiness
- process `CX_TASK_SANDBOX_IMAGE` must be a reviewed full local `sha256:` image ID;
  inspect confirms the exact immutable ID and both paths use `--pull=never`
- deliberate `/bin/bash` entrypoint, fixed startup/path controls and disabled
  image healthchecks; no repository startup scripts are sourced automatically
- image application selected from fixed absolute paths or the operator's absolute
  `CX_TASK_SANDBOX_EXECUTABLE`; reviewed repository wrappers need separate
  `CX_TASK_TRUST_REPO_EXEC=1` (including the toolchain-only compatibility image)
- credentials/proxies shared only through explicit process named-variable
  selection; authority/provenance/path keys are reserved, not shareable
- disabled or unauthorized readiness runs no Docker process; admitted readiness
  uses a read-only root/repository and no network, without sharing credentials
- authorized runtime uses a writable repository and normal Docker networking;
  command/provider grants and existing unsafe controls are transported separately
- stable sandbox/task/log contracts; container provenance uses the immutable ID

Guardrails:
- no automatic Docker requirement for ordinary host tasks
- preserve log/schema/quarantine contracts across host and container lanes
- operator image/executable/credential approval is not container escape
  prevention, a credential scrubber, or a boundary against same-user code
- readiness checks host `.cx` permissions and container executable access;
  its read-only probe does not establish actual runtime write permissions
- external Git metadata in linked worktrees remains a task-lane limitation;
  maintainer compatibility tooling has its separate explicit Git mount handling

Exit criteria:
- a project can say “run this task in the project container”
- execution provenance is visible in logs and diagnostics

### 4. Provider Sidecars And Local Services

Objective:
- make local provider setup more repeatable with optional sidecars for mock,
  Ollama, llama.cpp HTTP, or similar bounded services

Why fourth:
- depends on the earlier container execution and parity discipline
- useful, but should not outrun adapter boundaries

Scope:
- explicit sidecars only
- no silent background service management
- keep provider configuration inspectable

Current decision:
- the written local-service contract and fixture/mock validation floor is
  landed before any Docker Compose or service startup code

Current floor:
- `docs/providers/LOCAL_PROVIDER_SIDECARS.md` defines the local
  OpenAI-compatible MLX HTTP sidecar contract
- fixture-backed `llm resident probe-models --json` coverage validates the
  `/v1/models` readiness path and visible HTTP boundary fields without live
  provider services

Guardrails:
- process adapters remain stable defaults unless explicitly overridden
- Docker sidecars must not blur transport boundaries in diagnostics or telemetry

Exit criteria:
- XSHELF can document repeatable local provider stacks for projects that want them
- adapter telemetry still tells the truth about transport and provider type

### 5. Prebuilt Cache And Distribution

Objective:
- reduce first-run Docker cost for maintainers and larger projects

Why last:
- only worth doing after the workflow shape is stable

Scope:
- optional prebuilt maintainer image
- optional cached dependency layers
- possibly publish versioned image tags for release lines

Current decision:
- keep local-build-only as the default until a prebuilt image policy defines
  source Dockerfile, update cadence, tag/digest reporting, and multi-arch
  expectations
- any future remote image must be opt-in instead of pulled by default

Current floor:
- `scripts/compat_docker.sh --image <tag>` and `CX_COMPAT_IMAGE=<tag>` allow
  an explicit already-available image override
- default remains `xshelf-compat:local`, which auto-builds from the repo
  Dockerfile when missing
- override tags never pull automatically; missing override images fail unless
  `--rebuild` is used to build the repo Dockerfile into that tag
- JSON reports include `docker.image_source`, `docker.pull_policy=never`, and
  `docker.image_id` for local provenance

Guardrails:
- no hidden freshness claims
- document the source Dockerfile and rebuild/update policy

Exit criteria:
- first-run Docker setup is materially faster
- image provenance and update expectations are explicit

Remaining future work:
- optional published maintainer image policy
- optional versioned image tags or digest-pinned release images
- optional Docker Compose/service recipes for provider sidecars

## Non-Goals

- do not require Docker for ordinary XSHELF runtime usage
- do not replace host-native validation with Docker-only checks
- do not turn XSHELF into a generic container orchestrator
- do not blur provider adapter truth just because a provider happens to run in a container

## Decision Rule

Take the next Docker step only if it improves one of:
- reproducibility
- Linux parity
- project toolchain isolation
- provider setup repeatability

Do not take a Docker step that mainly adds indirection, startup cost, or hidden
state without a clear gain in one of those areas.
