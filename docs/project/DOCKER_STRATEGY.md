# Docker Strategy

Status: proposed

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
- already partially landed through `compat_docker.sh`
- improves confidence for every later Docker step

Current floor:
- `Dockerfile`
- `scripts/compat_docker.sh --smoke`
- `scripts/compat_docker.sh --quick`

Next additions:
- document first-run and warm-cache expectations
- add a rebuild/prune note for stale image or cache state
- decide whether to publish a prebuilt maintainer image or keep local build only

Exit criteria:
- maintainers have one documented Linux parity path
- `--smoke` and `--quick` roles stay explicit and non-overlapping

### 2. CI Parity Harness

Objective:
- make local Docker validation mirror the Linux CI surface more directly

Why second:
- it turns the maintainer image into a real push-preflight tool
- it reduces CI-only surprises before expanding Docker into runtime features

Scope:
- align local Docker checks with the Linux `cxrs-compat` workflow where sensible
- keep host-native validation authoritative for non-Linux environments

Guardrail:
- do not create a second contract surface with Docker-only semantics

Exit criteria:
- maintainers can answer “what will Linux CI do?” locally with one command
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

Likely shape:
- project config points XSHELF at a container image or compose service
- `task run` / `task run-all` can choose host or container execution lanes
- runtime records whether execution happened on host or in container

Guardrails:
- no automatic Docker requirement for normal XSHELF use
- no hidden repo writes outside the mounted workspace
- preserve log/schema/quarantine determinism across host and container lanes

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

Guardrails:
- no hidden freshness claims
- document the source Dockerfile and rebuild/update policy

Exit criteria:
- first-run Docker setup is materially faster
- image provenance and update expectations are explicit

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
