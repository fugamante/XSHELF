# Optional capture evidence

Status: initial local implementation, not a released interface. Owner: XSHELF
maintainer. This is the canonical operating guide and v1 contract for `evidence`;
ordinary `capture`, provider execution, and existing run-log schemas are unchanged.

## Use and retention

Use this provider-free path for noisy Cargo or Xcode/Swift build and test output
when a compact first response plus targeted details helps. Short checks should
remain direct: a JSON envelope can cost more context than the original output.

```bash
# Run in the project being inspected; choose an existing XSHELF executable.
evidence_dir=$(mktemp -d "${TMPDIR:-/tmp}/xshelf-evidence.XXXXXX")
xshelf evidence capture --store "$evidence_dir" -- cargo test
# Retain the printed ID and the absolute directory across shell invocations.
xshelf evidence show --store "$evidence_dir" --id ID --start 120 --lines 20
```

The store must be an existing absolute private directory (0700 on Unix), outside
the caller's project root. The nearest ancestor containing `.git` defines that
root; otherwise the current directory does. A reserved record is created with
0600 permissions before execution, without overwriting an existing file. Final
symlink records, hard-linked records, and records readable by other users are
rejected during retrieval. The store and its ancestors must remain under trusted
local control; this is not isolation from other processes running as the same user.
Non-Unix storage is not currently supported.

**Explicit retention:** this command saves the complete decoded captured text,
including any private content it contains. It does not redact secrets. Use only
approved or synthetic output. No automatic upload, LLM call, expiry, or deletion
occurs. Retain the directory only as long as needed; temporary stores may vanish
under OS cleanup. There is no implicit store and no write to `runs.jsonl` (including
`CX_LOG_FILE`); the evidence record is this lane's execution evidence.

The shared capture runner preserves command arguments, working directory, timeout
handling, and child exit status. Its text is UTF-8-lossy decoded stdout followed
by nonblank stderr, not an exact byte archive or chronological stream merge.
Like legacy capture, process output is buffered in memory. This does not sandbox
the command. Failed execution or persistence can leave an incomplete private
record; a retrieval error is not permission to rerun the original command.
Records larger than 64 MiB of serialized JSON are refused. `sync_all` does not
establish directory-entry power-loss durability.

## Summaries and retrieval

`cargo test|build|check`, `xcodebuild`, and `swift test|build` select narrow reducer
candidates; absolute executable paths are supported. Recognized output prioritizes
final counts/outcomes, then diagnostic lines, then two neighboring lines of
context. Exact repeated selected lines are collapsed. Source line numbers refer
to the retained decoded text; summaries may reorder lines by priority.
Unrecognized output uses the existing bounded head/tail clipping primitive with
automatic selection, not inferred test success. Wrapper commands are not guessed
or unwrapped. Summary text is bounded to 6,000 characters / 60 lines before the
clipping footer. Status is always a separate field even when text is truncated.
This is heuristic evidence selection, not a complete parser of all compiler logs.

`show` accepts 1-based `--start` (default 1), `--lines` in 1..200 (default 40), and
0-based Unicode-character `--offset` within that selected page (default 0). Each
response contains at most 12,000 text characters before JSON escaping. When
`next_offset` is non-null, use it with the **same start and lines**; after the
page is complete, use `next_start` and reset offset to zero. This preserves access
to very long lines without returning an unbounded response. Empty output is
retrievable at start 1. Invalid ranges are errors, not silent empty successes.

## JSON v1 contract

Every successful operation emits one JSON object and a newline. Required keys:

| Kind | Required fields |
| --- | --- |
| summary | `schema_version: 1`, `kind: "summary"`, `id: string`, `exit_status: integer`, `summary: object`, `retention: "explicit_store_until_user_removal"` |
| detail | `schema_version: 1`, `kind: "detail"`, `id: string`, `exit_status: integer`, `start: integer`, `total_lines: integer`, `returned_lines: integer`, `offset: integer`, `next_offset: integer or null`, `truncated: boolean`, `next_start: integer or null`, `text: string` |

The summary object contains `profile` (`cargo|xcode|generic`), `text`, `raw_chars`,
`summary_chars`, `raw_lines`, `selection_omitted_lines`, and `truncated`.
`selection_omitted_lines` counts removal/deduplication before clipping; it does
not claim to count lines lost during clipping. `summary_chars` measures only
summary text, not JSON overhead. No field represents measured tokens or money.

Stored records have `schema_version: 1`, `id`, `exit_status`, `summary`, and
`output`. IDs are opaque and checked before path construction. Invalid versions,
ID mismatches, malformed records, and oversized records fail retrieval. These
experimental evidence records are separate from the existing exported contract
bundle and schema registry. Existing consumers require no migration; consumers
of this opt-in surface must check the version. Breaking changes require a new
version and migration notes.

`capture` returns the child's exit code after successful persistence. Parse errors
return 2; execution/storage/retrieval errors return 1 on stderr. If persistence
fails after execution, stderr reports the child status. `import` and `show`
return 0 when successful even when the stored child status was nonzero.

## Replay and validation

Import approved existing logs without rerunning a build:

```bash
xshelf evidence import --store "$evidence_dir" --input synthetic.log --profile xcode --status 0
```

Import requires a profile and recorded exit status, reads at most 8 MiB, and does
not execute any command. The supplied status is an assertion by the caller, not
an independently verified result.

Reproduce the synthetic comparison:

```bash
EVIDENCE_PYTHON=/path/to/pinned-venv/bin/python scripts/evidence_harness.sh > evidence-report.json
```

The harness rebuilds the current source, runs the focused Rust and Python checks, and
emits one JSON study report on stdout. Set `TIKTOKEN_CACHE_DIR` when using an
offline cache for `o200k_base`. It fails if required evidence, declared status,
expected effective profile, or savings in any noisy case fail. It rejects empty
and malformed fixture manifests before replay. The pinned
Python packages are in `scripts/evidence_requirements.txt`; the tokenizer data
is fetched by `tiktoken` on first use and hash-verified by that package.
`evidence-report.json` is a local generated artifact; keep it outside source
control if it contains a future approved private replay.

`scripts/evidence_measure.py` is the canonical source for generated synthetic
logs and the stdout JSON measurement report; it uses an isolated temporary
workspace and deletes only that workspace on completion. The baseline is legacy
`capture cat` over the same source, matching an unrecognized build-output stream;
it is not a claim of improvement over every existing specialized reducer.
On the initial native run, full summary plus one three-line detail response used
783 vs 6,879 characters for Cargo-style output and 881 vs 9,229 for Xcode-style
output. A concise 18-character check grew to 508 characters. IDs can vary the
response length slightly. Required fixture warnings and outcomes survived.
In that original character-only report, measured tokens are explicitly null. No application build was rerun for this
comparison and no ScriptBoard/private source was used.

Acceptance evidence covers failure details, late outcomes, warning deduplication,
unknown-output fallback, argument/status preservation, Unicode paging, private
storage, and invalid input rejection. Refresh this evidence on changes to reducer
selection, response/storage contracts, process handling, or privacy boundaries.
The roadmap tracks selective adoption and explicit run deltas after the named
tokenizer study below. This initial synthetic result is not total task-token,
cost, hosted CI, release, or production assurance.

Local validation: host `guardrails.sh` passed (348 Rust tests, release metadata,
and packaging lifecycle checks); Docker `compat_docker.sh --ci` passed all 14
gates with 347 Rust tests, compatibility comparisons, and shell regressions.
The platform count difference is the existing macOS-only telemetry test. The
10 focused evidence tests also passed after contract-fixture assertions were
added. Host tests ran with the outer capture's `CX_LOG_FILE` removed from their
subprocess environment so fixture-owned log paths remained authoritative.
The local skill validator and all three CLI entrypoint probes passed. This is
local evidence from the initial implementation; no hosted CI or public release was run.


## Named-tokenizer study and decision

Decision: **go for explicit, approved noisy build/test evidence; no-go for
universal routing or concise commands**. Keep generic fallback conservative.
The next adoption gate is naturally occurring native failure output or a
materially different approved toolchain/output shape during independently
required validation; do not rerun builds solely to accumulate measurements.

Follow-up native adoption result (2026-09-06): the ScriptBoard development task
reported 53/53 native tests passing and supplied a private measurement record.
Using the same named tokenizer below, complete legacy responses plus two detail
reads totaled 7,798 tokens; evidence responses plus one detail page totaled 2,278
tokens, a reduction of 5,520 (70.79%). Both paths reported preservation of every
declared outcome, material warning, and large FDX scale marker. Native output
contained four nonfatal toolchain warnings and no test failures. Complete
response character totals are unknown. This successful run does not establish
native failure-diagnostic recovery. The final small description/privacy-assertion
change received typecheck and static review after the measured native suite;
the suite result applies to the earlier tested state. Two earlier native checks
were excluded from this comparison. The archival pass verified the measurement
copy and arithmetic, not the execution claims or full-response tokenization.
Only the content-safe measurement record was copied to durable private local
storage; the original raw record remains subject to temporary-storage cleanup.
Selective adoption remains justified; broad automatic routing is not established.

The measurement uses `tiktoken==0.12.0`, `regex==2026.9.3`, and the explicitly
named `o200k_base` encoding. This is a reference tokenizer, not a claim about the
active assistant's tokenizer or account billing. Authority: the
[upstream tokenizer documentation](https://github.com/openai/tiktoken/tree/0.12.0)
and the pinned installed implementation. The encoding asset SHA-256 is
`446a9538cb6c348e3516120d7c08b09f57c36495e2acfffe59a5bf8b0cfb1a2d`.

`scripts/evidence_tokens.py` is the canonical measurement tool;
`rust/cxrs/tests/fixtures/evidence_study.json` declares the synthetic corpus,
required evidence, and retrieval windows before replay. The tool returns a
separate JSON v1 report and does not change `evidence` or legacy-report contracts.
Its `evaluation` section gates required-span recovery, imported exit status, and
expected effective profile on every case (including declared generic fallback).
A failed correctness or per-noisy-case savings gate exits nonzero. It separately totals
token deltas for noisy cases (at least 1,000 source characters) and lists concise
regressions; `selective_opt_in` is a study recommendation, not a production or
release gate. The legacy comparison replays `capture cat`, so it does not claim
to verify the original failing command's exit status. Imported status is checked
against the manifest's declared status, not independently observed execution.
It sums tokens for each full stdout response separately, including JSON keys,
IDs, escaping, clipping notices, and needed detail pages. Both paths must recover
all required spans. Legacy capture pays for any required direct line-range read;
the evidence lane pays for its JSON detail responses. Windows with declared
`targets` are fetched only when those targets are missing. Retrieval is
fixture-driven, not an autonomous model experiment. Tool-call arguments, framing,
reasoning, context reuse, caching, latency, and financial savings are not measured.

| Fixture / replay | Legacy + needed reads | Evidence + needed pages | Token reduction |
| --- | ---: | ---: | ---: |
| Cargo success | 2,226 | 134 | 94.0% |
| Cargo failure with missing assertion values | 2,090 | 279 | 86.7% |
| Xcode success | 1,633 | 143 | 91.2% |
| Xcode failure | 1,629 | 146 | 91.0% |
| Concise success | 12 | 91 | negative |
| Concise failure | 7 | 85 | negative |
| Unrecognized noisy output | 843 | 428 | 49.2% |
| Repeated warnings | 1,034 | 102 | 90.1% |
| Compiler-policy flag noise | 2,556 | 332 | 87.0% |
| Sanitized historical ScriptBoard native build | 3,200 | 1,483 | 53.7% |

IDs vary slightly between runs, so exact counts may vary by a few tokens. Do not
average these hand-selected cases into a production savings claim. All required
spans survived; the Cargo failure and unknown-output cases needed one evidence
page each. The ScriptBoard replay retained all five required unique warning /
completion spans without another page; legacy capture needed one focused read.
That log records a successful build, not a test execution or current-head gate.
Local paths and UUIDs were replaced before comparison. No raw ScriptBoard log or
private fixture was copied into this repository.

The initial exploratory ScriptBoard retrieval schedule fetched redundant pages
and cost more than legacy capture. With only missing-target windows, the original
selector used 2,443 vs 3,200 tokens, requiring two pages. A failing regression
then established that compiler policy flags (`-warnings-as-errors` and
`SWIFT_TREAT_WARNINGS_AS_ERRORS`) were incorrectly prioritized as diagnostics.
Narrow diagnostic recognition fixed this without changing response fields or
ordinary capture; the final 1,483-token result used the same missing-target plan.

Harness refresh (2026-10-12): the current checkout's debug executable was rebuilt
with `cargo build --locked`, then all nine synthetic fixture cases passed required
span, declared status, and expected effective-profile checks. Eleven Rust evidence
tests and six Python harness tests passed. Across seven noisy fixtures, full response counts totaled about
10,500 fewer `o200k_base` reference tokens than the legacy replay; the two concise
fixtures cost 77 and 78 more tokens. Two noisy cases needed one detail page.
The result supports selective opt-in and explicitly excludes concise commands.
These are hand-selected synthetic cases and do not establish task-wide savings,
native failure recovery, hosted validation, or a release gate. The next adoption
evidence remains an approved naturally occurring native failure log or materially
different toolchain output during independently required validation.

Two harmless live ScriptBoard CLI probes also preserved status and evidence:
help used 143 direct vs 235 evidence tokens; invalid-profile rejection used 56
vs 138. Those commands should remain direct. Neither executed a validation suite,
and tracked/untracked non-ignored content plus Git status matched before/after.
The project was already dirty and was not edited.

Reproduce with an isolated optional measurement environment:

```bash
python3 -m venv /tmp/xshelf-meter-venv
/tmp/xshelf-meter-venv/bin/python -m pip install -r scripts/evidence_requirements.txt
/tmp/xshelf-meter-venv/bin/python scripts/evidence_tokens.py \
  --binary rust/cxrs/target/debug/cxrs \
  --manifest rust/cxrs/tests/fixtures/evidence_study.json --encoding o200k_base
/tmp/xshelf-meter-venv/bin/python test/evidence_tokens_test.py
```

Tokenizer assets may be downloaded on first load; warm `TIKTOKEN_CACHE_DIR`
before an offline run. No log content is sent to a provider. An explicit local
manifest may select `approval: explicit-local-validation-output`, an `input_file`,
optional `sanitize_local_paths: true`, required spans and targeted windows; use
`--workdir` to exercise replay from that project's actual directory. An approval
label records the operator's decision; it is not an automated privacy assessment.
Temporary copied input and evidence stores are scoped to each run and removed
at completion. Reports contain metrics, provenance labels and hashes, not raw
output; keep manifests with private paths outside public source.

Focused follow-up validation: 11 evidence integration tests on macOS and Linux,
125 host unit tests, all-target Clippy, three measurement tests (needed reads,
Unicode page-boundary recovery, and rejection of absent source evidence), and
format/integration checks. The previous full host/Docker pass is retained as
baseline evidence; it was not rerun for this narrow reducer correction. Refresh
broader gates if storage, process execution, schemas, or shared contracts change.
