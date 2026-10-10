# Release Readiness Snapshot

Snapshot date: 2026-10-09

## Release recovery completed (2026-10-07)

`v2026.10.07` is published from frozen source `f3525c08f76d58dd441a19391277a6d2058e1dbc`.
The October 9 recovery target is complete: native ARM/Intel reproduction and
runtime/lifecycle, Developer ID signing, accepted Apple notarization, anonymous
public-byte verification and Homebrew archive delivery are complete. Fresh
GitHub API publication-age evidence passes. The six logging alerts were
statically dispositioned as false positives; no runtime source changed.

## Postrelease security decision (2026-10-09)

The published October 7 archives remain bound to `f3525c08f76d58dd441a19391277a6d2058e1dbc`.
That source predates the Cargo/provider fixes and contains the quarantine ID,
repository model, and parity schema paths later fixed on main. Later
source merges do not change those public bytes. A new security maintenance
candidate is on hold while live Security Cloud findings are grouped by current
reachability and release impact. No exact-head native ARM or Intel package
evidence, signing, notarization, public-byte verification, or publication
authority has been established for a later source. The October 7 signoff remains
historical evidence for its frozen source only.

Synthetic runs reproduced mixed/parallel task dependency/resource-wave bypasses
and the one-worker and completed-task rerun paths. The source fix is subject to
merged-tree and hosted verification before closure. Actual HTTP
adapter execution confirmed credential arguments and operator-supplied URL
parameters. The HTTP argument findings and unbounded repository log readers
remain open.

## Current State

XSHELF is in production-readiness hardening rather than broad substrate buildout.
The active work is contract stability, provenance, compatibility validation,
release hygiene, and guarded opt-in expansion.

Current merged readiness floor:
- `v2026.10.07` is published, and its annotated tag resolves to immutable
  source `f3525c08f76d58dd441a19391277a6d2058e1dbc`.
- `v2026.08.29` is published, and its annotated tag resolves to immutable
  source `b8ea981b5ea0e6a64bfd92b87611f954d3c6288e`.
- `v2026.08.29` release source is validated, with native ARM and Intel
  reproducibility, Developer ID signing, Apple notarization, public-byte
  verification, and isolated Homebrew lifecycle evidence complete.
- `v2026.08.25` is published, and its annotated tag resolves to immutable
  packaged source `210b3b524c01f4dc673244077f02b53d39cedcda`.
- `v2026.08.25` release source is validated, with protected-main controller
  `8c2a16b937dc79d49ecc11dd2da94eb63ebd4eaf` supplying the canonical native
  reproduction policy without changing source provenance.
- strict run-log validation requires HTTP provenance keys on modern rows:
  `http_request_profile`, `http_provider_format`, and `http_parser_mode`.
- local and Docker compatibility scripts distinguish quick, full, smoke, and CI
  parity modes with explicit report metadata.
- candidate age and publication age are independent of compatibility evidence.
- the Docker strategy has landed guarded floors for maintainer parity, CI parity,
  task sandboxing, provider sidecar contract documentation, and explicit image
  provenance.
- README and public website first-output examples now use the same current
  `task-check.v1` contract shape.

Current maintenance source:
- `VERSION` is `2026.10.07`, including the superseded September candidate,
  merged maintenance/security fixes and release-health separation.
- `v2026.10.07` release source is validated, with full native guardrails,
  local compatibility, Docker CI parity, hosted Linux/macOS compatibility,
  CodeQL and both native package reproduction/lifecycle gates passing on `f3525c08f76d58dd441a19391277a6d2058e1dbc`.
- The source binds the exact seven-file signed inventory before exclusive
  atomic publication; the public GitHub inventory contains five files.
- Tag-time documentation retains its pre-publication wording as history.

Current published release:
- The latest published version is `2026.10.07`, available at
  https://github.com/fugamante/XSHELF/releases/tag/v2026.10.07.
- Published assets are exactly two native archives, `SHA256SUMS`, and a sanitized
  `.notary.json` record for each archive:
- `xshelf-2026.10.07-aarch64-apple-darwin.tar.gz`: `db276dab7662bfbc58ff79968b6c4e9bd3295456d66c858bc93758267ef0b5b1`
- `xshelf-2026.10.07-x86_64-apple-darwin.tar.gz`: `27af439a84495e6d68e127df5ba1dabaeaebe3593b28497b606abb246fc6686d`
- `SHA256SUMS`: `1b2ff930dd89ea35538ee571189d0bd7b29647c059e525660be609f26326ff9c`
- Native Intel evidence passed in workflow `37576041904`, retained artifact
  `11463034612`, with native x86_64 execution, matching controller/source,
  two clean builds, runtime, relocation and isolated Homebrew lifecycle.
- Both binaries are Developer ID signed with hardened runtime and secure
  timestamps; Apple accepted both notarization submissions. Public downloads
  match the exact signed inventory. Signed ARM Homebrew lifecycle and production-formula install/test passed;
  both native tap qualification jobs passed in run `37633497731` (PR #19).
- The archive formula is published in `fugamante/homebrew-tap` and preserves
  its macOS Sequoia floor. No new bottle is included in this release.

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
- New maintenance-release Homebrew bottles; August bottles remain historical,
  while this release uses the signed/notarized archives
- broader default capture prompt replacement beyond the current opt-in
  `shadow_narrow` profile
- additional backend adapter families beyond the guarded `http-curl`
  request-profile boundary

## Release Decision

The `v2026.08.29` release is cut. Its annotated tag, five-file GitHub release,
and Homebrew source formula publish the validated signed/notarized native macOS
assets from immutable source `b8ea981`. Public archive bytes and notarization
records match the locally accepted release inventory. The `v2026.08.29`
release source is validated for publication.

The `v2026.09.19` release source is validated for publication. This statement
records the locally validated source candidate only; it does not claim that a
tag, package, signature, notarization result, GitHub release, or Homebrew update
exists. Each remains a separate fail-closed authority and evidence gate.

The `v2026.10.07` release source is validated for publication. The
`v2026.10.07` release is cut. The annotated tag preserves `f3525c08f76d58dd441a19391277a6d2058e1dbc`;
GitHub publishes five verified signed/notarized assets, and Homebrew publishes
an archive formula from those immutable URLs and hashes. The publication-age
recovery gate is complete. Future source changes require their own validation;
this signoff does not cover a later candidate.
