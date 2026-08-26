# Release Readiness Snapshot

Snapshot date: 2026-08-25

## Current State

XSHELF is in production-readiness hardening rather than broad substrate buildout.
The active work is contract stability, provenance, compatibility validation,
release hygiene, and guarded opt-in expansion.

Current merged readiness floor:
- `v2026.08.20` is published, and the annotated tag resolves to the validated
  release head after the reliability bundle and dependency advisory patch.
- `v2026.08.20` release source is validated, with the immutable tag retaining
  the release-head code, metadata, and compatibility inputs.
- strict run-log validation requires HTTP provenance keys on modern rows:
  `http_request_profile`, `http_provider_format`, and `http_parser_mode`.
- local and Docker compatibility scripts distinguish quick, full, smoke, and CI
  parity modes with explicit report metadata.
- release cadence metadata remains under the 14-day freshness gate.
- the Docker strategy has landed guarded floors for maintainer parity, CI parity,
  task sandboxing, provider sidecar contract documentation, and explicit image
  provenance.
- README and public website first-output examples now use the same current
  `task-check.v1` contract shape.

Current unpublished candidate:
- `VERSION` is advanced to `2026.08.25` for the CLI packaging validation line.
- The dirty diagnostic ARM64 archive and temporary-prefix Homebrew install,
  test, upgrade, and uninstall lifecycle are validated locally with user-state
  preservation.
- The candidate is not a validated release source until clean ARM64 and x86_64
  artifacts and native Intel Homebrew lifecycle evidence are complete.
- A manual-only `macos-15-intel` compatibility job is available to collect the
  missing native Intel evidence without treating Rosetta as equivalent.
- `v2026.08.20` remains the newest published release and immutable authority.

## Release Candidate Validation

Future release candidates are ready for review when these checks are green on
the release head:

```bash
./scripts/compat_local.sh --quick
./scripts/compat_local.sh --full
./scripts/compat_docker.sh --ci
cd rust/cxrs && ./scripts/guardrails.sh
```

Use host-native `compat_local --full` as the strongest local release-signoff
signal. Use Docker `--ci` as the closest local Linux core guardrail mirror, while
remembering that GitHub event-specific gates still run only in CI.

## Deferred From Release Blocking

The following items remain future opt-in work and should not block the next
patch or minor release unless they become explicit scope:
- published Docker maintainer images
- Docker Compose or service startup recipes for provider sidecars
- published Homebrew formula, bottles, and signed/notarized binary assets (the
  local deterministic archive and draft-formula validation foundation is
  documented in `docs/PACKAGING.md`)
- broader default capture prompt replacement beyond the current opt-in
  `shadow_narrow` profile
- additional backend adapter families beyond the guarded `http-curl`
  request-profile boundary

## Release Decision

The `v2026.08.20` release is cut. Its annotated tag and GitHub release are
published from the final validated release head; continue recording subsequent
changes under `Unreleased` until the next release candidate is prepared.
The `v2026.08.20` release source is validated for publication.
