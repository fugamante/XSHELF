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
python3 tools/quality_gate.py --max-file-lines 100000 --max-fn-lines 100000 --max-raw-eprintln 0
cd ../..
./scripts/release_pretag_check.sh
./scripts/check_action_pins.sh .
```

`./scripts/guardrails.sh` covers the release-cadence age check and
`tools.test_release_check`; local release validation stays strict. CI runs
metadata integrity inside compatibility, then calendar-age checks in the
independent `release-health` job after compatibility completes. The pre-tag wrapper runs
`release_check.py` with `--require-current-release-notes` and
`--require-published-status-docs`; normal development can keep rolling notes
under `Unreleased` and advance `VERSION` without claiming publication. The
published-status guard uses the newest final-release `vN.N.N` Git tag reachable
from `HEAD`, not `VERSION`, as its source of truth. Strict published-status
validation requires tags and sufficient history; tagless or depth-limited
checkouts fail with an explicit fetch diagnostic instead of inferring
publication from `VERSION`.

The pre-tag wrapper also requires durable release-source validation markers for
`vVERSION` on the exact release head. Update the roadmap and readiness decision
before creating the annotated tag so the immutable tagged source validates
itself for the current-source gate without claiming publication early. The
published-status gate remains separate: release-source markers cannot replace
the explicit roadmap, readiness, and release-decision markers for the newest
reachable published tag.

Validation preference for maintainers:
- prefer `./scripts/compat_local.sh --quick` when you need representative
  compat readiness for the current machine.
- use `cargo test --tests -- --test-threads=1` or
  `./scripts/compat_local.sh --full` when preparing release-signoff evidence.
- use `./scripts/compat_docker.sh --smoke` only as a cheaper Linux-hosted
  preflight when you want early runtime drift detection before paying for the
  fuller compat suite.
- use `./scripts/compat_docker.sh --ci` when you want the closest local mirror
  of the Linux `cxrs-compat` job before pushing, while remembering that
  event-specific PR metadata gates still live in GitHub Actions.

- Validate `CHANGELOG.md` has release notes.
- If command entrypoints or command-facing help/routing changed, validate `README.md` and `docs/project/XSHELF_RENAME_MIGRATION.md` were updated in the same bundle.
- Validate README requirements/version notes still match tested environment.

## Cadence Enforcement

Candidate age and published-release age are separate controls. A VERSION update
does not deliver a release to users. `compat-check (ubuntu-latest)` retains version
consistency, notes/status integrity, and all code/security checks. `release-health`
depends on compatibility with `if: always()`, so stale metadata cannot suppress
compatibility, shell regressions or dependency-security evidence. The health job
does not turn a failed compatibility result green. Branch protection remains
unchanged: compatibility is required; health remains a separate visible result.
Release signoff and the pre-tag checklist still require fresh candidate metadata.

At release planning and before closing a release pass, fetch fresh publication
evidence and run the independent 14-day publication audit:

```bash
release_audit_dir=$(mktemp -d "${TMPDIR:-/tmp}/xshelf-release-audit.XXXXXX")
gh api repos/fugamante/XSHELF/releases/latest > "$release_audit_dir/latest.json" &&
python3 rust/cxrs/tools/release_check.py \
  --published-release-json "$release_audit_dir/latest.json" \
  --max-published-age-days 14
```

The weekly target remains the planning goal; 14 days is the escalation threshold.
This audit is explicit and read-only locally, and runs in the separate CI health
job using a fresh read-only GitHub API response. No network calls are added to
local tests. Use a fresh API response each time; the validator checks its
contents, not authenticity or retrieval time. Missing, malformed, draft,
prerelease, future-dated, or overdue evidence fails. `release-exception` cannot
bypass publication age. An overdue audit requires an owner, blocker, and dated
recovery plan and stays red until publication. A tag alone cannot establish
publication. Passing age does not establish asset integrity or release signoff.

- CI enforces release recency with `python3 rust/cxrs/tools/release_check.py --max-version-age-days 14`.
- The check fails when `VERSION` has not changed for more than 14 days.
- Temporary bypass is allowed only on pull requests carrying label `release-exception`.
- Use `release-exception` only with explicit rationale and a follow-up release cut plan.
- PR label addition/removal triggers fresh checks. It applies to candidate age
  only, never publication age, compatibility/security checks, or pre-tag signoff.
- Publication age runs even if candidate age fails. Failure summaries link to
  this canonical recovery plan. No age failure uses `continue-on-error`.

### Recovery outcome (2026-10-07)

GitHub publication and the age gate recovered on October 7; the October 9
plan still requires Homebrew PR #19 native qualification and integration. `v2026.10.07` is published with
signed/notarized native ARM and Intel archives, verified anonymous public bytes,
and a proposed Homebrew archive formula. Both native reproduction/lifecycle gates passed
on `f3525c08f76d58dd441a19391277a6d2058e1dbc`; six logging alerts were dispositioned
as false positives. Fresh GitHub publication-age evidence passes. Retain the
recovery plan below as the decision record, rather than using VERSION-only bumps.

The ordinary pre-push guard infers published status from reachable tags, so a
new unpublished local tag can make its documentation check circular. This
release used the authorized annotated-tag API with exact target verification
after all preceding guards passed. Published-status docs were reconciled only
after publication. Keep the pre-tag source gate and actual API publication audit
separate; the frozen archive documentation is a historical snapshot.

### Historical security maintenance plan (2026-10-06)

- Owner: release maintainer. Review/cut target: 2026-10-09.
- Keep the current `2026.09.19` candidate and its actual modification date until
  preparing a new validated release candidate; source merges do not refresh its
  age or claim publication. Standalone `main` freshness remains red meanwhile.
- On October 7, select `2026.10.07` as the consolidated maintenance candidate,
  including merged fixes through `1f6007831eb5cbd252fa5f6d7940652e86101b76` and
  the release-health separation. The September candidate is superseded without
  claiming its publication. The new version identifies this concrete release
  scope; it is not a repeated freshness-only bump. Public August release age
  remains red until validated assets are actually published.
- Include the merged execution-filter, installer, anchored log, MLX and
  branch-audit and task-command authority fixes in the next maintenance
  candidate. Include complete Docker runtime/readiness, immutable-image, executable and
  credential authority remediation only after independent merged-source
  verification. Review the remaining live findings and release impact before
  publication. Do not treat command admission as Docker isolation.
- Require merged-source verification of canonical native reproduction root
  ownership and cleanup before packaging. Synthetic filesystem regressions do
  not replace the two-build native archive evidence or the separate Intel gate.
- Require merged-source verification of route lookup argument/path isolation and
  `CXLOG_ENABLED=0` run-log opt-out before release. Review remaining live medium
  findings on the exact candidate; a passing PR exception is not release
  readiness.
- Prepare truthful candidate version/release notes and run the complete pre-tag
  checklist on its exact head. Resolve unavailable platform/signing evidence or
  record the release decision before tagging, packaging or publication.
- If the target cannot be met, the maintainer records the blocker and a revised
  date. Do not extend the age limit, remove the gate, or claim a release occurred.
- Freeze maintenance scope and reconcile September candidate notes with merged
  security fixes under `Unreleased`. Select a new dated candidate only when its
  exact source is selected; refresh source-validation claims after that head passes.
- Exit evidence: full native release signoff and Docker CI parity on selected
  source, Intel/ARM package reproduction, signed/notarized inventory, verified
  GitHub public bytes and Homebrew lifecycle, and a fresh publication-age audit.
  External release actions retain separate operator authorization gates.

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
