# Release Readiness Snapshot

Snapshot date: 2026-08-29

## Current State

XSHELF is in production-readiness hardening rather than broad substrate buildout.
The active work is contract stability, provenance, compatibility validation,
release hygiene, and guarded opt-in expansion.

Current merged readiness floor:
- `v2026.08.25` is published, and its annotated tag resolves to immutable
  packaged source `210b3b524c01f4dc673244077f02b53d39cedcda`.
- `v2026.08.25` release source is validated, with protected-main controller
  `8c2a16b937dc79d49ecc11dd2da94eb63ebd4eaf` supplying the canonical native
  reproduction policy without changing source provenance.
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

Current release candidate:
- `VERSION` is `2026.08.29`.
- `v2026.08.29` release source is validated, with Developer ID signing, Apple
  notarization, GitHub publication, and Homebrew tap publication still pending.
- The signing implementation pins the certificate identity, validates the
  configured team at execution time, preserves Apple submission evidence, and
  emits only sanitized receipts in distributable output.

Current published release:
- The latest published version is `2026.08.25`; the release is available at
  https://github.com/fugamante/XSHELF/releases/tag/v2026.08.25.
- Published assets are exactly:
  - ARM64 archive `9865e5440a5b6554cea952b630f8f3c26c6eabd64fdf39cb2e53c850306c873f`;
  - Intel archive `e908105a9767d60cb057e0a9469cd2c49b2b750c7082435a468980d29ca9fd2e`;
  - `SHA256SUMS` `3e56cb544b87d7e59a0d098941c1e4c4e3ab59f61c718aee34122b9c33b9beaf`.
- Clean ARM64 canonical reproduction, runtime, relocation, clean-home, and
  isolated Homebrew lifecycle validation passed locally. Native Intel evidence
  passed in workflow `33234570375`; retained artifact `9709817894` records
  `runner_arch=x86_64`, `translated=0`, exact controller and source revisions,
  two clean compilations, runtime, relocation, and the isolated Homebrew
  lifecycle.
- The assets are not Developer ID signed or notarized. ARM64 has
  linker-generated ad-hoc signing state; Intel is unsigned.
- No Homebrew formula or bottle is published.
- Archive-embedded documentation remains the immutable tag-time snapshot, so
  its pre-publication wording is historical.

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
- Homebrew bottles; the `v2026.08.29` source formula and signed/notarized binary
  archives are active release scope, while bottles remain deferred
- broader default capture prompt replacement beyond the current opt-in
  `shadow_narrow` profile
- additional backend adapter families beyond the guarded `http-curl`
  request-profile boundary

## Release Decision

The `v2026.08.25` release is cut. Its annotated tag and GitHub release publish
the validated unsigned native macOS assets from immutable source `210b3b5`;
continue recording subsequent changes under `Unreleased` until the next release
candidate is prepared. The `v2026.08.25` release source is validated for
publication. Signing, notarization, and Homebrew publication remain separate
future authority gates.

The `v2026.08.29` release source is validated for publication. This statement
does not claim that the tag, signed artifacts, Apple notarization acceptance,
GitHub release, or Homebrew formula already exists; each remains fail-closed
until its exact evidence is recorded.
