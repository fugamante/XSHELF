# Release Readiness Snapshot

Snapshot date: 2026-10-10 (merged security boundary evidence reconciled)

## Release recovery completed (2026-10-07)

`v2026.10.07` is published from frozen source `f3525c08f76d58dd441a19391277a6d2058e1dbc`.
The October 9 recovery target is complete: native ARM/Intel reproduction and
runtime/lifecycle, Developer ID signing, accepted Apple notarization, anonymous
public-byte verification and Homebrew archive delivery are complete. Fresh
GitHub API publication-age evidence passes. The six logging alerts were
statically dispositioned as false positives; no runtime source changed.

## Postrelease security decision (2026-10-10)

The published October 7 archives remain bound to `f3525c08f76d58dd441a19391277a6d2058e1dbc`.
That source predates the Cargo/provider fixes and contains the quarantine ID,
repository model, and parity schema paths later fixed on main. Later
source merges do not change those public bytes. The tagged source also passes
HTTP credentials and operator-selected URL values through curl arguments; the
current source repair does not change those archives. A new security maintenance
candidate is on hold while live Security Cloud findings are grouped by current
reachability and release impact. No exact-head native ARM or Intel package
evidence, signing, notarization, public-byte verification, or publication
authority has been established for a later source. The October 7 signoff remains
historical evidence for its frozen source only.

Mixed/parallel task dependency/resource-wave fixes were verified on merged
source `43b7791e26d2270b280790a6e565a73e6044b2bf`, with hosted Linux and
CodeQL passing. Actual HTTP adapter execution confirmed credential arguments
and operator-supplied URL parameters. Current source transports them in a
anonymous curl config descriptor on Unix while preserving opt-in redirects. This source
change does not supply exact-head native ARM/Intel package evidence or release
authority. The `task run-all` accounting change counts selected planner
blockers in final failed/blocked outcomes, including all-blocked JSON runs.
Pending selections stay pending for retry; blocked `complete`/`in_progress`
selections become `failed`, and already failed selections remain failed. This
change still needs exact-head native package validation. Owned synthetic run
logs and quota catalogs reproduced excessive local reads and symlink
redirection. Current source bounds recent run-log views, task failure-row
lookup, task inspection and recovery, log tailing, validation, appended scans,
migration input and output, execution run-log writes, and quota catalog reads.
Default repository log and catalog reads reject symlinks. Repository-selected
run, schema-failure, and task-event JSONL appends now use descriptor-anchored
parents and reject symlinked or nonregular leaves. Task-event follow reads use
the same protected source and bound each row and poll while retaining incomplete
rows. A rejected catalog remains unavailable until refresh.
Local llama.cpp and MLX process adapters now send repository-derived prompt
content through an anonymous stdin file rather than process arguments. The
published October 7 source predates this change. Current maintenance source
caps test-output warning retention, clips selected lines before copying, and
bounds unfamiliar-output fallback. Full child-output capture remains a separate
availability limit. Current source locks the opened JSONL file across each
run, schema-failure, and task-event append, including explicit run-log aliases.
A failed partial write attempts rollback only when the file length still
matches this writer's bytes; cleanup failure or a changed length is reported,
and a fragment may remain. Lock waits are bounded to ten seconds. Noncooperating
writers can still interleave records, and a write racing rollback may be lost.
Focused synthetic regressions cover short writes, partial-write errors,
noncooperating appends, and inode aliases. This evidence does not establish
later-release readiness.
The opt-in parity diagnostic passes its repository script path and catalog
arguments to Bash as positional data. Synthetic quoted and command-bearing
repository paths no longer execute an injected marker; the trusted `cx`
function still receives its expected arguments. A separate macOS syscall
probe observed `openat` return `ENOENT` for the first schema-failure log leaf
through a valid held `.cx/cxlogs` descriptor. Three of ten 64-process cold
batches lost one observational row while retaining all quarantine records.
The same `openat` failure occurred in three of twenty 64-process batches on
exact pre-lock source `5786814f2fb0e105feb0523b2acbdba61df987af`, so the
JSONL lock did not introduce it. The OS-level cause remains unproven. A
private bounded retry probe recovered all 1,280 rows in twenty batches but
is not a source fix.
First-use behavior needs separate attribution and validation.
Explicit `CX_LOG_FILE` remains operator-selected. Remote write reachability is
unproven, and adjacent task-event and other local readers require separate
assessment.
PR #118 merged as `d31df5e309adc26013e08e31c48d3aeaa07540a0`, with an
identical tree to the independently reviewed patch. Pinned Rust 1.95 host
guardrails and full compatibility, Docker CI parity, exact-main Linux workflow
`38052301708`, and CodeQL workflow `38052301440` passed. Security Cloud now
marks the six bounded-log/quota findings and two combined HTTP URL findings
fixed after merged-source verification. The HTTP adapter and URL remain
operator-selected; nonlocal HTTP and redirects require explicit overrides.
Repository-selected JSONL append and task-event follow paths have owned
synthetic regressions for redirected writes, special files, oversized rows,
split rows, and normal use. The security maintenance release stays on
hold: no exact-head native ARM or Intel package reproduction, signing,
notarization, public-byte verification, or publication authority has been
established for this later source. The published `v2026.10.07` assets are
unchanged.

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
