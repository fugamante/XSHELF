# XSHELF (formerly CX)

`XSHELF` is a deterministic Rust runtime for LLM-assisted repository work: it
wraps repo commands with structured validation, inspectable runtime state, and
explicit execution policy.

Use it when free-form agent output is too loose for automation, CI, or operator
workflows that need stable JSON contracts and replayable diagnostics.

Trust and naming:
- `XSHELF/CX` is an independent open-source project and is not affiliated with
  or endorsed by OpenAI.
- `XSHELF` is the primary project name.
- `CX` remains a supported compatibility command surface during migration
  (`./bin/cx ...`).

## Try It

Start here in two minutes. These first commands are read-only inspection checks;
they do not change repository configuration.

```bash
./bin/xshelf version
./bin/xshelf task check --json
./bin/xshelf core --json
./bin/xshelf diag --json --window 20
```

Expected results:
- `version` prints the XSHELF runtime version.
- `task check --json` reports task orchestration readiness.
- `core --json` prints machine-readable runtime and backend state.
- `diag --json` prints recent machine-readable diagnostics.

Then check backend health:

```bash
./bin/xshelf llm check
./bin/xshelf doctor
./bin/xshelf health
```

Run `cxo` only after readiness and backend checks are healthy. A safe first
command is a read-only repository inspection:

```bash
./bin/xshelf cxo git status
```

The short alias is available as `./bin/xs ...`; the compatibility alias remains
available as `./bin/cx ...`.

## Who It Is For

XSHELF is for:
- repo operators who need auditable command results instead of loose agent text
- teams wiring LLM-assisted workflows into CI or local automation
- developers testing hosted or local model backends behind explicit policy

## What You Get

- Deterministic command pipeline: capture, reduce, budget, execute, validate,
  quarantine invalid structured output, and append telemetry.
- Structured runtime visibility: JSON surfaces for `core`, `diag`, `scheduler`,
  `telemetry`, `logs`, `optimize`, `policy`, `quota`, and `broker`.
- Task orchestration: `task add`, `task fanout`, `task run`, and `task run-all`
  with explicit readiness and concurrency summaries.
- Backend policy: default `codex`, optional `ollama`, `llamacpp`, and `mlx`,
  with HTTP transport behind explicit rollout policy.

Key terms:
- deterministic: same inputs and settings produce inspectable runtime behavior
- schema: the required JSON shape a structured command result must match
- quarantine: invalid structured output is saved for inspection and replay
- backend: the model runtime that handles LLM calls
- policy: explicit rules for allowed execution paths and transports

## Entrypoints

Canonical runtime:
- engine: `rust/cxrs`
- primary entrypoint: `bin/xshelf`
- short alias: `bin/xs`
- compatibility alias: `bin/cx`
- compatibility shell shim: `lib/cx.sh`

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
- `llama-cli` from llama.cpp if you want the optional `llamacpp` local backend
- `mlx-lm` in a Python environment if you want the optional macOS `mlx` backend

## Shell Setup

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
./bin/xshelf llm check
./bin/xshelf llm use codex
./bin/xshelf llm use ollama llama3.1
./bin/xshelf llm use llamacpp ggml-org/Qwen3-0.6B-GGUF:Q4_0
./bin/xshelf llm models list --json | jq .
./bin/xshelf llm models add local_qwen --backend mlx --model mlx-community/Qwen2.5-1.5B-Instruct-4bit
./bin/xshelf llm models inspect local_qwen --json | jq .
./bin/xshelf llm smoke "Respond with OK only."
./bin/xshelf llm unset model
```

`llamacpp` accepts either a local `.gguf` path or a llama.cpp Hugging Face repo
specifier. The default smoke recipe uses `ggml-org/Qwen3-0.6B-GGUF:Q4_0`, a
small Apache-2.0 GGUF model whose Q4_0 file is about 429 MB and whose model card
documents direct `llama-cli -hf ggml-org/Qwen3-0.6B-GGUF:Q4_0` usage.

```bash
brew install llama.cpp
./scripts/llamacpp_smoke.sh

# Equivalent manual smoke:
./bin/xshelf llm use llamacpp ggml-org/Qwen3-0.6B-GGUF:Q4_0
CX_CMD_TIMEOUT_SECS=600 \
CX_LLAMA_CPP_ARGS="-n 64 --temp 0 -c 2048 --simple-io" \
  ./bin/xshelf cxo printf 'xshelf llamacpp smoke\n'
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
  - `CX_LLAMA_CPP_MODEL`
  - `CX_LLAMA_CPP_BIN`
  - `CX_LLAMA_CPP_ARGS`
  - `CX_MLX_MODEL`
  - `CX_MLX_PYTHON`
  - `CX_MLX_ARGS`
  - `CX_MLX_MAX_TOKENS`
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

## Where To Go Next

Operator docs:
- [docs/README.md](docs/README.md) - documentation index
- [docs/manuals/00_README.md](docs/manuals/00_README.md) - manual entrypoint
- [docs/manuals/01_pdf/CX_MANUAL_MASTER.pdf](docs/manuals/01_pdf/CX_MANUAL_MASTER.pdf) - generated master manual PDF
- [docs/manuals/02_web/CX_MANUAL_MASTER.html](docs/manuals/02_web/CX_MANUAL_MASTER.html) - tracked HTML reader mirror
- [docs/providers/HTTP_PROVIDER_TLS.md](docs/providers/HTTP_PROVIDER_TLS.md) - HTTP/TLS operator guidance

Maintainer docs:
- [docs/project/REPO_ROLE_CONTRACT.md](docs/project/REPO_ROLE_CONTRACT.md)
- [docs/project/ROADMAP.md](docs/project/ROADMAP.md)
- [docs/project/RELEASE_CADENCE.md](docs/project/RELEASE_CADENCE.md)
- [docs/project/XSHELF_RENAME_MIGRATION.md](docs/project/XSHELF_RENAME_MIGRATION.md)
- [CONTRIBUTING.md](CONTRIBUTING.md)

Planning and history:
- [docs/orchestration/](docs/orchestration/) - phase plans, work queues, and milestone notes
- [docs/turboquant/](docs/turboquant/) - TurboQuant experiment archive
- [docs/project/RUST_FIRST_MIGRATION.md](docs/project/RUST_FIRST_MIGRATION.md)
- [docs/project/SECURITY_HISTORY_REWRITE.md](docs/project/SECURITY_HISTORY_REWRITE.md)

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
- [docs/contributing/GOOD_FIRST_ISSUES.md](docs/contributing/GOOD_FIRST_ISSUES.md)
- [docs/contributing/CONTRIBUTOR_WALKTHROUGH.md](docs/contributing/CONTRIBUTOR_WALKTHROUGH.md)

## License

MIT. See [LICENSE](LICENSE).
