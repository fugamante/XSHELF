# Release Readiness Snapshot

Snapshot date: 2026-10-24 (dependency repair pending release qualification)

## Selected maintenance candidate (2026-10-10)

`v2026.10.10` selects the merged post-October 7 security and reliability
changes for release validation. It is not tagged or published. The candidate
has two-build native ARM and Intel package reproduction on prior merged source
`526e92eeacb5f437e78d49be5ba757e3eb7fbdf9`, with Intel evidence in
workflow `38092777853`. This evidence must be repeated on the final merged
source head. Signing, notarization, public-byte verification, and Homebrew
qualification are separate release gates. Scoped source signoff accepts the
lower-severity current-source issues listed below; publication remains on hold.
The October 7 release is still the latest published release.

### October 24 qualification hold

The frozen source `df42b88536af7a96764771a6fdf3ce6644083157` failed the fresh
Cargo Audit gate in workflow `40155975688`: locked `tokio 1.52.3` is affected
by `RUSTSEC-2026-0386`. The narrow repair selects patched `tokio 1.53.3`
without changing dependency requirements, dependency edges, or advisory policy.
This repair does not establish exploitability in XSHELF or release readiness.
Review and validate the repair, then select the replacement merged source and
repeat its exact-head release qualification. Earlier source-signoff markers and
native package evidence do not certify this later source. Tagging, Developer ID
signing, notarization, public-byte verification, Homebrew qualification, and
publication retain their separate evidence and authority gates.

### Current-source finding decision

Security Cloud listed 57 open XSHELF records on October 10 (16 medium, 19 low,
22 informational), observed on historical revisions. This inventory is not a
fresh scan of this source. Source inspection classifies 15 medium records as
historical paths or not reproducible on current main; the remaining medium
record `commit:55e2234450248191ad75a83c4b69fd80` is the static-site
guidance addressed in this candidate. Among the
41 low and informational records, 14 describe historical paths, 26 remain
source-current and deferred, and one describes intentional schema evidence.
The historical medium IDs are `commit:3222fc6cdadc8191a3d4a77fe163c198`,
`commit:e074015876288191894a804cd7244036`,
`commit:7e2e4e691be08191beb742cd67307620`,
`commit:bbab31cf634c81918a66e9341acae96c`,
`commit:28de076528e88191a31e22dcb76afb80`,
`commit:0e33e87799c881919f3fdddee291ac23`,
`commit:0c4ab36a13d88191859ba0da16687f15`,
`commit:585e39965cb88191ab7181fa118ad6d7`,
`commit:38ff71509e808191918fd596aa7afbaa`,
`commit:4e11be6be61481919d6f60d171cbfde8`,
`commit:709f9958bc148191bd7a15d4a5e116c5`,
`commit:d11700acc7b88191b751f55f11a5f6bf`,
`commit:cd3beb04dff48191a94a48d4a17e80d4`,
`commit:583e2fd2d32c8191a7844d57a9a8d613`, and
`commit:e8426c8102188191b03332c082e25d60`.
The historical low/informational IDs are `commit:a4def8b5a6dc8191a4c1db38538d8d37`,
`commit:033867d5ed0c8191b8d35fff36dd7128`,
`commit:ed9c1b3cc4fc819192249f226c411ca3`,
`commit:3bc752f3bcd08191b2549c51313d62b2`,
`commit:591ef30e2878819198d57ff842a262f0`,
`commit:c3e62090c960819199db990727681b09`,
`commit:132b3f870edc8191a0b2ec15fabfac7d`,
`commit:054c7729e04c8191ba01a8c38c4c21f9`,
`commit:45f4ecf2682c8191b5b87799a901b9d5`,
`commit:689c82374b108191b9f3ce3f1ba8b8a7`,
`commit:e5f8077fc5f88191ad68fd4c90465cc2`,
`commit:283f722c9bb88191877657165be66f2d`,
`commit:7e1238ad8db081919d390726e5a71ffb`, and
`commit:3d3629ab532c8191bd8bf453e217c04c`.

Maintainers accept the following source-current lower-severity issues for this
CLI maintenance-source decision. None is used as release evidence or a
publication-authority input. They remain open for separate fixes and
verification on merged source:

- Local log output and previews (`commit:6d49775eb1bc8191a8aadb237261d6b2`,
  `commit:9be50072c3a48191bebfe77981f1a3d0`,
  `commit:cd985965c0748191a897c043a7217b9e`,
  `commit:55f49837172481919167f65170264ee9`): untrusted repository log text
  can affect terminal or hosted CI output. No attacker-selected outbound
  destination or credential exfiltration path was demonstrated. Revisit
  before using log-derived CI output as trusted evidence or during the next
  log-output hardening.
- Diagnostic availability and interactive introspection
  (`commit:a393332e1748819191fb4c4b795c12c8`,
  `commit:d814638440fc8191861513944abad282`,
  `commit:193327bc34888191a66c8dacd7514bfe`): crafted counters can
  panic or wrap diagnostics, and an unset interactive Ollama model may start
  introspection. Neither establishes package or signing authority. Revisit
  before untrusted logs feed automated release checks or read-only claims cover
  interactive introspection.
- Operator output correctness (`commit:b07001fc3d108191a24fe69bdfb3d989`,
  `commit:731e8382ffa48191889b98dd436ea9bb`,
  `commit:61b19e752e7c819197e75023e874dfb5`,
  `commit:0edf883cd1408191bb2d8443132ccf0f`,
  `commit:d037eca91c188191bd4c344d8754b510`,
  `commit:03d4af73435c819192d8887b670d8868`): fanout, diagnostic action,
  benchmark, and dry-run displays can misstate outcomes. Revisit before an
  automation treats those displays as authoritative.
- Audit and contract metadata (`commit:3106a08cf3188191bd3f829f6720c5e5`,
  `commit:4709fcc9b3208191b6b9060760cece49`,
  `commit:7ca57f17101481918a7cc2feae96b6ce`): provenance, bundle drift,
  and retry metrics can mislead. Revisit before those fields become release or
  automation gates.
- Optional installer and telemetry contracts (`commit:c230ab4efb188191ab6f3bda9d229673`,
  `commit:29292ca483b481919b53805171cd98f8`,
  `commit:fd20b7ab6e4081918ec9efa6824545ae`,
  `commit:c8162e49ee6081918ec9efa6824545ae`,
  `commit:150f9a7aac148191b67f88ce874be1be`,
  `commit:a20e8ccfeefc8191a83421ae64f14804`): source inspection found
  shell-profile and optional output mismatches without a demonstrated new
  authority bypass. Revisit before those paths are used as release evidence
  or at the next compatibility release.
- Test-only path and coverage gaps (`commit:a5a9f53221f48191a4a7e2986b86d430`,
  `commit:927b0113ac6881919e37d0c0a8b777af`,
  `commit:6ee8f05788dc81918e7c4d58a559dcc4`,
  `commit:2fb02f0f994c8191b28e9d9a24ed9caa`): a controlled checkout path
  can affect one shell-based test, while retry and concurrency tests can miss
  regressions. These are not production CLI paths. Revisit before testing an
  untrusted checkout path or changing those test contracts.

`commit:642a4f1f5b9c819190f0a7abae4ea245` describes a documented separate
schema-failure evidence channel, not a run-telemetry opt-out bypass:
`CXLOG_ENABLED=0` suppresses run and capture rows but preserves private
quarantine and schema-failure records. The locked synthetic opt-out regression
passed on current main. Revisit this policy only if the operator contract
changes; do not claim that the opt-out erases security evidence.

The reported judge backend override (`commit:38ff71509e808191918fd596aa7afbaa`)
did not reproduce on current main: an owned synthetic task with explicit
`ollama` and a different process default invoked the mock Ollama backend for
both replicas and judge, with no default-provider call. A convergence summary
row still labels the restored default and belongs to the output-correctness
work above. No finding is closed by this source decision.

The repository-root guidance is repaired in `PUBLIC_SURFACES.md`. Static-site
deployment remains blocked until an actual host serves only the isolated
allowlist and rejects repository-private paths. This source decision does not
authorize a website deployment.

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
current source repair does not change those archives. The `v2026.10.10`
candidate is selected. Prior-head native ARM and Intel package evidence is
recorded above; it does not validate a later documentation merge. Signing,
notarization, public-byte verification, and publication authority have not
been established for this candidate. The October 7 signoff remains historical
evidence for its frozen source only.

The later-source signing preflight now checks the builder's exact v1 member
paths, types and modes before credential use. Owned synthetic ARM/Intel
archives reproduced acceptance of an extra self-manifested executable on the
previous main and verify rejection in current source; the checksum sidecar
and manifest remain consistency checks, not proof of unsigned input origin.
The current source bounds each local `chunk` payload to a positive character
budget, including oversized single lines, and rejects a zero budget. Task
fanout keeps its prior child count and stores only symbolic chunk references;
no in-repository provider payload path was established. The published October 7
source still has the oversized-line behavior. This source repair does not
supply native package or publication evidence for a new release.
The three Rust function names that failed the strict host naming check on
starting main `10ed4095` were renamed without changing the three-segment rule
or its allowlist. The pinned Rust 1.95 `compat_local.sh --full` gate now passes
on this maintenance source. Docker CI parity and hosted checks are separate
evidence; none of these source gates makes a later release ready.

The maintenance source now writes default local, aggregate, and Docker
smoke/CI compatibility reports through held `.cx/compat` directory descriptors
and private atomic replacement. Symlinked parents or leaves and nonregular
report destinations fail without following their targets; explicit `--out`
paths retain operator-selected behavior. The aggregate runner now records a
child's actual nonzero exit code and fails the overall result even when the
child's JSON claims success; missing or malformed child reports also produce
structured failures. These later-source changes do not alter the
published October 7 archives or establish native package or publication
readiness.

This source change also rejects task execution above 16 effective
replicas or 32 potential task/judge invocations, including retries and judge selection;
the selected run-all schedule is admitted before task mutations, and an
authorized sandbox performs one versioned handoff. Owned synthetic task and
mock-Docker regressions cover the source boundary. This is source evidence,
not exact-head native ARM/Intel package or publication evidence.

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
private bounded retry probe recovered all 1,280 rows in twenty batches.
Current source now retries only a transient missing final log leaf on the
held directory descriptor for at most eight short waits. Deterministic
fault injection and concurrent first-use and reuse regressions cover the
source fix; other errors and permanent missing leaves still fail closed.
Exact-head native package validation remains separate.
CI compatibility failure artifacts now use a runner-temporary directory
prepared before checkout. The summary reads only named generated logs, and
upload staging accepts only regular files opened without following symlinks.
Owned synthetic checkout-symlink and legitimate-log regressions cover this
path. Pull-request code retains its runner's same-user file access; hosted
failure-artifact upload has not been exercised as a separate release gate.
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
split rows, and normal use. Prior-head ARM and Intel reproduction is recorded
above. The maintenance release stays on hold for final-head package evidence,
signing, notarization, public-byte verification, Homebrew qualification, and
publication authority. The published `v2026.10.07` assets are unchanged.

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

Current maintenance candidate:
- `VERSION` is `2026.10.10`, selecting the fixes merged after the October 7
  release. Prior-head native evidence passed; the final merged source requires
  a fresh exact-head run before release action.

Published October 7 source evidence:
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
- New maintenance-release Homebrew bottles; August bottles remain historical.
  The published October 7 release used signed/notarized archives; a later
  release needs its own package and signing evidence.
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

The `v2026.10.10` release source is validated, with the lower-severity
current-source findings above accepted for this CLI maintenance candidate.
The `v2026.10.10` release source is validated for publication. These are
source-only markers required on the eventual tagged tree. They do not certify
the final merged head's native packages or authorize a tag. Exact-head ARM and
Intel reproduction, signing, notarization, public-byte verification, Homebrew
qualification, and publication remain separate fail-closed gates.
