# Release Cadence

## Cadence

- Target: weekly patch releases while in active development.
- Release trigger:
  - meaningful feature set merged, or
  - reliability/security fix requiring user visibility.

## Pre-release Checklist

```bash
cd rust/cxrs
./scripts/guardrails.sh
./scripts/check_rs_max_lines.sh 600 ../..
./scripts/check_integration_guardrails.sh ../.. 500
cargo check
cargo test --test reliability_integration -- --test-threads=1
python3 -m unittest tools.test_release_check
python3 tools/quality_gate.py --max-file-lines 100000 --max-fn-lines 100000 --max-raw-eprintln 0
python3 tools/release_check.py --repo-root ../.. --max-version-age-days 14
cd ../..
./scripts/check_action_pins.sh .
```

- Validate `CHANGELOG.md` has release notes.
- If command entrypoints or command-facing help/routing changed, validate `README.md` and `docs/project/XSHELF_RENAME_MIGRATION.md` were updated in the same bundle.
- Validate README requirements/version notes still match tested environment.

## Cadence Enforcement

- CI enforces release recency with `python3 rust/cxrs/tools/release_check.py --max-version-age-days 14`.
- The check fails when `VERSION` has not changed for more than 14 days.
- Temporary bypass is allowed only on pull requests carrying label `release-exception`.
- Use `release-exception` only with explicit rationale and a follow-up release cut plan.

## Versioning Policy

- Use semantic versioning intent:
  - patch: bugfix/reliability/docs-only behavior clarifications
  - minor: backward-compatible feature additions
  - major: breaking command or contract changes

## Release Notes Minimum

- New features
- Behavior changes
- Fixes
- Known limitations
