# XSHELF (formerly CX)

`XSHELF` is a deterministic, Rust-first LLM runtime for repository work.

It is built for people who want:
- structured command execution with schema-enforced JSON
- repo-scoped runtime state, logs, quarantine, and task orchestration
- explicit policy boundaries and execution guidance instead of silent agent drift
- machine-readable diagnostics that downstream tools can rely on

Naming note:
- `XSHELF/CX` is an independent open-source project and is not affiliated with or endorsed by OpenAI.
- `CX` remains the compatibility name across the current command surface during transition.

Canonical runtime:
- engine: `rust/cxrs`
- primary entrypoint: `bin/xshelf`
- short alias: `bin/xs`
- compatibility alias: `bin/cx`
- compatibility shell shim: `lib/cx.sh`

## What It Does

Core runtime capabilities:
- deterministic structured commands with quarantine/replay on validation failure
- unified Rust execution pipeline:
  - capture
  - native reduction
  - budgeting
  - LLM execution
  - validation
  - logging
- task graph and run orchestration:
  - `task add`
  - `task fanout`
  - `task run`
  - `task run-all`
- append-only telemetry, diagnostics, and policy visibility
- run-all concurrency summaries on task, diagnostics, and telemetry surfaces
- backend adapters with explicit rollout policy and opt-in HTTP transport
- one OpenAI-compatible HTTP JSON request profile on the existing `http-curl` adapter boundary

Default backend model:
- `codex` by default
- `ollama` optional

## Requirements

Minimum local requirements:
- `bash`
- `git`
- Rust toolchain for development and local validation:
  - `cargo`
  - `rustfmt`
  - `clippy`

Optional but commonly used:
- `jq` for JSON inspection in examples
- `ollama` if you want the optional local backend path

## Quick Start

Basic runtime checks:

```bash
cd <repo-root>
./bin/xshelf version
./bin/xshelf core
./bin/xshelf doctor
./bin/xshelf health
```

Run a normal command through the runtime:

```bash
./bin/xshelf cxo git status
```

Inspect runtime status in JSON:

```bash
./bin/xshelf core --json
./bin/xshelf diag --json --window 50
./bin/xshelf scheduler --json --window 50
```

Install shell functions and man pages:

```bash
./bin/xshelf-install
./bin/xs-install
man xs
man xshelf
man cx
```

Compatibility note:
- `./bin/xs ...` is a supported short alias for `xshelf`.
- `./bin/cx ...` remains fully supported during migration.

## Runtime Model

The Rust pipeline is intentionally strict:

1. capture output
2. reduce context
3. enforce budget
4. run the backend
5. validate structured output
6. quarantine invalid results
7. append telemetry

Current compatibility storage path:
- repo-scoped runtime state currently lives under `.codex/`

Current layout:
- schemas: `.codex/schemas/`
- logs: `.codex/cxlogs/`
- quarantine: `.codex/quarantine/`
- state/tasks/runtime metadata: `.codex/`

Runtime vs development:
- end users should use `doctor`, `health`, and command JSON surfaces
- maintainers and CI run the Rust suite, compat checks, and guardrails

## Common Commands

Backend selection:

```bash
./bin/xshelf llm show
./bin/xshelf llm use codex
./bin/xshelf llm use ollama llama3.1
./bin/xshelf llm unset model
```

Structured command and schema inspection:

```bash
./bin/xshelf schema list
./bin/xshelf schema list --json | jq .
./bin/xshelf next git status
```

Task graph:

```bash
./bin/xshelf task add "Implement parser hardening" --role implementer
./bin/xshelf task list --status pending
./bin/xshelf task list --json | jq .
./bin/xshelf task fanout "Ship release notes improvements" --from staged-diff
./bin/xshelf task check --json | jq .
./bin/xshelf task run-all --status pending --mode mixed
./bin/xshelf task run-all --status pending --mode parallel --max-workers 2
```

Telemetry and policy:

```bash
./bin/xshelf telemetry 50 --json | jq .
./bin/xshelf logs stats 200 --json | jq .
./bin/xshelf optimize 200 --json | jq .
./bin/xshelf policy show --json | jq .
./bin/xshelf quota probe 30 --json | jq .
./bin/xshelf broker show --json | jq .
```

## Configuration Essentials

Important runtime knobs:
- budgeting:
  - `CX_CONTEXT_BUDGET_CHARS`
  - `CX_CONTEXT_BUDGET_LINES`
  - `CX_CONTEXT_CLIP_MODE`
  - `CX_CONTEXT_CLIP_FOOTER`
- timeout:
  - `CX_CMD_TIMEOUT_SECS`
- backend/model:
  - `CX_LLM_BACKEND`
  - `CX_OLLAMA_MODEL`
  - `CX_MODEL`
- output mode:
  - `CX_JSON_DEFAULT`
  - `CX_JSON_AUTO`
- execution mode:
  - `CX_MODE`
  - `CX_SCHEMA_RELAXED`
- HTTP adapter policy:
  - `CX_HTTP_PROVIDER_URL`
  - `CX_HTTP_PROVIDER_TOKEN`
  - `CX_HTTP_AUTH_PROFILE`
  - `CX_HTTP_AUTH_HEADER`
  - `CX_HTTP_AUTH_VALUE`
  - `CX_HTTP_AUTH_VALUE_FILE`
  - `CX_HTTP_AUTH_USERNAME`
  - `CX_HTTP_AUTH_PASSWORD`
  - `CX_HTTP_AUTH_PASSWORD_FILE`
  - `CX_HTTP_PROVIDER_TOKEN_FILE`
  - `CX_HTTP_REQUEST_PROFILE`
  - `CX_HTTP_PROVIDER_MODEL`
  - `CX_HTTP_PROVIDER_FORMAT`
  - `CX_HTTP_REQUIRE_HTTPS`
  - `CX_HTTP_ALLOW_LOCAL_HTTP`
  - `CX_HTTP_ALLOWED_HOSTS`
  - `CX_HTTP_CA_BUNDLE`
  - `CX_HTTP_CLIENT_CERT`
  - `CX_HTTP_CLIENT_KEY`
  - `CX_HTTP_TLS_PINNEDPUBKEY`
  - `CX_HTTP_TLS_MIN_VERSION`
  - `CX_HTTP_FOLLOW_REDIRECTS`
  - `CX_HTTP_MAX_REDIRECTS`

Useful state/config commands:

```bash
./bin/xshelf mode
./bin/xshelf mode --json | jq .
./bin/xshelf broker show --json | jq .
./bin/xshelf quota guard show
./bin/xshelf quota catalog show --json | jq .
```

## Docs Map

Use the README as the front door. Use the docs below for deeper material.

Product and repo boundary:
- [docs/REPO_ROLE_CONTRACT.md](docs/REPO_ROLE_CONTRACT.md)
- [docs/REPO_SYNC_PLAN.md](docs/REPO_SYNC_PLAN.md)
- [docs/XSHELF_RENAME_MIGRATION.md](docs/XSHELF_RENAME_MIGRATION.md)
- [docs/ROADMAP.md](docs/ROADMAP.md)
- [docs/RELEASE_CADENCE.md](docs/RELEASE_CADENCE.md)

Execution guidance and orchestration:
- [docs/PHASE_VI_EXECUTION_GUIDANCE.md](docs/PHASE_VI_EXECUTION_GUIDANCE.md)
- [docs/PHASE_VI_PARALLEL_SUBSTRATE.md](docs/PHASE_VI_PARALLEL_SUBSTRATE.md)
- [docs/POST_PHASE_VI_OVERVIEW.md](docs/POST_PHASE_VI_OVERVIEW.md)
- [docs/PHASE_IV_MULTI_MODEL_ORCHESTRATION.md](docs/PHASE_IV_MULTI_MODEL_ORCHESTRATION.md)

Budget-aware orchestration:
- [docs/PHASE_VII_BUDGET_AWARE_ORCHESTRATION.md](docs/PHASE_VII_BUDGET_AWARE_ORCHESTRATION.md)
- [docs/PHASE_VII_WORK.json](docs/PHASE_VII_WORK.json)

Contracts and adapters:
- [docs/CONTRACT_COMPATIBILITY.md](docs/CONTRACT_COMPATIBILITY.md)
- [docs/PROVIDER_ADAPTER_PLAN.md](docs/PROVIDER_ADAPTER_PLAN.md)
- [docs/HTTP_PROVIDER_TLS.md](docs/HTTP_PROVIDER_TLS.md)

TurboQuant research archive:
- [docs/TURBOQUANT_SPIKE.md](docs/TURBOQUANT_SPIKE.md)
- [docs/TURBOQUANT_METRIC.md](docs/TURBOQUANT_METRIC.md)
- [docs/TURBOQUANT_CAP_MLX.md](docs/TURBOQUANT_CAP_MLX.md)

## Validation

Runtime-facing validation:

```bash
./bin/xshelf doctor
./bin/xshelf health
./bin/xshelf logs validate --fix=false
```

Maintainer validation:

```bash
./scripts/compat_local.sh --quick
./scripts/compat_local.sh --full --out .codex/compat/latest.json
./scripts/compat_all.sh --quick
./scripts/compat_all.sh --full --out .codex/compat/all_latest.json
./bin/cx-compat-local --quick
```

Rust maintainer path:

```bash
cd rust/cxrs
cargo fmt --check
cargo clippy --all-targets -- -D warnings -D clippy::too_many_arguments
cargo test --tests -- --test-threads=1
```

Git hook guardrails:

```bash
./bin/cx-enable-githooks
git push
```

## Development Notes

Repository layout:
- `bin/xshelf` - canonical runtime entrypoint
- `bin/xs` - short runtime alias
- `bin/cx` - compatibility runtime alias
- `rust/cxrs/src/main.rs` - Rust binary entry
- `rust/cxrs/src/app/mod.rs` - routing/orchestration
- `rust/cxrs/src/modules/*.rs` - runtime modules
- `lib/cx.sh` - shell compatibility shim

Current design discipline:
- Rust is authoritative for runtime behavior, contracts, and telemetry
- no automatic checks during shell startup
- diagnostics go to stderr; pipeline output stays on stdout
- capture is internal-native only

Versioning:
- machine-readable current version: `VERSION`
- release history: `CHANGELOG.md`, tags, and `VERSION_HISTORY.md`

## Contributing and Security

- [CONTRIBUTING.md](CONTRIBUTING.md)
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)
- [SECURITY.md](SECURITY.md)
- [docs/GOOD_FIRST_ISSUES.md](docs/GOOD_FIRST_ISSUES.md)
- [docs/CONTRIBUTOR_WALKTHROUGH.md](docs/CONTRIBUTOR_WALKTHROUGH.md)

## License

MIT. See [LICENSE](LICENSE).
