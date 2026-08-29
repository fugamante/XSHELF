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
- Clean ARM64 packaging and Homebrew checks passed locally. Native Intel
  archive and isolated Homebrew lifecycle validation passed on GitHub's
  `macos-15-intel` runner for exact source
  `727afec7a2214704fb9cb6e686872325e765afd9` in workflow run `32986914305`.
- That run is predecessor evidence only after later documentation commits. The
  authoritative Intel evidence for review is the retained artifact from a
  successful final-head dispatch whose `source_revision` equals the exact
  candidate commit selected for PR or merge review.
- Exact-head Intel evidence must record `runner_arch=x86_64`, `translated=0`,
  reproducible archive bytes, validated manifest/provenance, clean-home and
  relocation behavior, `xshelf` / `xs` / `cx` compatibility, install,
  `brew test`, revision upgrade, uninstall, and unchanged Homebrew
  configuration and user state.
- Final-head checksums and provenance stay in the 90-day workflow artifact so
  recording them does not create a new, unvalidated repository head.
- `v2026.08.25` release source is validated, and its annotated source tag points
  at that source. GitHub release publication, signing, notarization, and formula
  publication remain separate decisions.
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

The `v2026.08.25` release source is validated for publication. This is a source
readiness statement, not a publication claim, and is supported only when the
retained native Intel artifact names the exact selected candidate head. Keep
published status anchored to reachable tag `v2026.08.20` unless a separate
release action is explicitly authorized.
