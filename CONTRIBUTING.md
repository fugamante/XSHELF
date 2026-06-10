# Contributing

## Scope

- Rust (`rust/cxrs`) is the canonical implementation.
- Bash is compatibility/bootstrap only.
- Keep behavior deterministic and non-interactive unless explicitly required.

## Branding and Command Stability

- Project branding is `XSHELF (formerly CX)`.
- Canonical user-facing command spelling is `xshelf`.
- `xs` is an allowed short alias where brevity helps and ambiguity stays low.
- `cx` remains a supported compatibility alias during migration.
- Preserve `CX_*` environment variable compatibility by default.
- Keep `cx` examples where compatibility context matters.
- Rename policy is defined in `docs/project/XSHELF_RENAME_MIGRATION.md`.

## Start Here

- New contributor issue list: `docs/contributing/GOOD_FIRST_ISSUES.md`
- Contributor walkthrough: `docs/contributing/CONTRIBUTOR_WALKTHROUGH.md`
- Roadmap: `docs/project/ROADMAP.md`
- Release cadence: `docs/project/RELEASE_CADENCE.md`

## Development Setup

```bash
cd rust/cxrs
./scripts/guardrails.sh
./scripts/check_rs_max_lines.sh 600 "$(pwd)/../.."
./scripts/check_integration_guardrails.sh "$(pwd)/../.." 500
cargo fmt
cargo check
cargo test --tests -- --test-threads=1
python3 tools/quality_gate.py --max-file-lines 100000 --max-fn-lines 100000 --max-raw-eprintln 0
python3 tools/release_check.py --repo-root ../.. --max-version-age-days 14
```

## Pull Request Requirements

- Include tests for new behavior and failure paths when applicable.
- Preserve stdout pipeline behavior; diagnostics go to stderr.
- Do not introduce startup side effects.
- Update `README.md`/`CHANGELOG.md` when behavior or contracts change.
- When command entrypoints or command-facing Rust routing/help surfaces change, update `README.md`, `CHANGELOG.md`, and `docs/project/XSHELF_RENAME_MIGRATION.md` together.
- Keep release cadence current: CI fails when `VERSION` is older than 14 days unless the PR is explicitly labeled `release-exception`.
- Keep Rust/file/integration guardrails green locally before push: `./scripts/guardrails.sh`, `./scripts/check_rs_max_lines.sh`, and `./scripts/check_integration_guardrails.sh`.
- Keep third-party GitHub Actions pinned to full 40-character commit SHAs; validate with `./scripts/check_action_pins.sh .`.

## Commit Guidance

- Keep commits focused and reviewable.
- Prefer small mechanical refactors before functional changes.
- Add migration notes when changing log/schema contracts.

## Branch Naming

- Use `primary/<short-scope>` for local feature branches.
- Prefer short scope slugs over phase-sentence names.
- Keep branch names within the same readability rule used elsewhere:
  - max `3` segments
  - concise snake_case or kebab-case scope
- Good:
  - `primary/session-pairing`
  - `primary/contract-bundle`
  - `primary/provider-adapter`
- Avoid:
  - `primary/session-token-pairing-integration`
  - `primary/add-phase-vi-execution-guidance-surface`
- Do not rename already-published shared `cx/*` branches casually; shorten new work by default.
