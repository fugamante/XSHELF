# Task Ledger Design

Status: local integration candidate; distributed consensus is out of scope

## Decision

XSHELF needs one authoritative local ordering point for task mutations. The
existing `tasks.json` whole-file read/modify/write path can lose successful
concurrent task additions or independent status changes. A synchronized
post-read reproduction lost both classes of update, and a 24-process CLI run
reported 24 successful exits while retaining only 6 tasks.

The smallest contract-preserving response is a local replicated-state-machine
shape without network consensus:

1. take an exclusive process lock;
2. replay immutable commands in revision order;
3. validate the next command against the current revision and state digest;
4. durably publish the command;
5. replace the backward-compatible `tasks.json` snapshot atomically.

Paxos or Raft membership, leader election, quorum operation, remote workers,
and network protocols are explicit non-goals. Model-output `majority`
convergence is also unrelated to this storage ordering contract.

## Owned Runtime State

- `.cx/task_ledger/NNNNNNNNNNNNNNNNNNNN.json` is authoritative after the first
  successful task mutation. Each immutable file contains one command.
- `.cx/tasks.lock` serializes local processes. The lock file carries no state.
- `.cx/tasks.json` remains the exact JSON array consumed by existing tooling.
  It is a derived snapshot once the ledger exists.
- `.cx/tasks.snapshot.json` records the ledger revision and digest represented by
  the derived snapshot. It lets recovery distinguish an interrupted snapshot
  replacement from an uncoordinated legacy writer.

An existing `tasks.json` is migrated lazily. Migration is quiescent: operators
must stop older XSHELF task writers before the first ledger-backed mutation,
because those binaries do not honor `.cx/tasks.lock`. The first successful
mutation writes a deterministic `bootstrap` command containing the prior array
and then writes the requested command. A repository with no tasks starts
directly at revision 1. No command, environment variable, alias, public JSON
response, or task record field changes.

## TaskCommand Envelope

Every entry uses internal contract `task-command.v1` and records:

- `operation_id`: unique identifier for one committed CLI attempt;
- `revision` and `expected_revision`: a monotonic compare-and-swap boundary;
- `transition`: `bootstrap`, `add`, `fanout`, or `status`;
- `task_id`: the affected task or fanout parent where applicable;
- `worker_id` and `lease_epoch`: fencing metadata;
- `policy_digest`: reserved nullable field for a future exact policy binding;
- `prior_digest`, `payload_digest`, and `result_digest`: SHA-256 task-state
  integrity inputs;
- `prior_command_digest` and `command_digest`: a canonical envelope chain that
  binds operation, transition, worker, lease, policy, timestamp, and payload
  metadata;
- `at`: diagnostic timestamp;
- `payload.tasks`: the complete resulting task array.

The full-state payload is intentional for this first local implementation. It
makes each revision a checkpoint, keeps replay deterministic, and avoids a
second snapshot schema. It is appropriate for the current small local task
queue, but ledger compaction must precede materially larger or remote queues.

Duplicate operation IDs are accepted internally only when their complete
semantic intent is identical. A reused operation ID with different transition,
task, worker, lease, policy, or payload fails closed. The current single-attempt
CLI generates a fresh ID and does not expose automatic retry. Before any
retrying API or remote invocation is introduced, callers must supply and
preserve IDs and the store must return the original persisted result before
rerunning a mutation closure.

## Transition And Fence Rules

`add` appends exactly one unique task. `fanout` only appends a parent and its
children. `status` changes only the selected task's status and update time and
accepts the four existing statuses: `pending`, `in_progress`, `complete`, and
`failed`. The current permissive status-transition behavior is preserved.

Lease epoch `0` is compatibility mode and is what existing commands use. A
nonzero fenced claim must advance the task's epoch and identify its worker.
Subsequent fenced completion must present the same epoch and worker; stale or
foreign workers are rejected. The validator and tests exist now, but scheduler
claim wiring is deliberately deferred so this storage fix does not change task
execution semantics.

## Durability, Replay, And Corruption

Command and snapshot files are written to unique sibling temporary files,
flushed with `sync_all`, renamed, and followed by a parent-directory sync. A
successful ledger rename is the visible local commit point. Command publication
precedes snapshot replacement. Therefore:

- a crash before command rename leaves only an ignored non-JSON temporary file;
- a crash after command publication replays the committed command;
- read-only commands replay the ledger without repairing projection files and
  warn rather than fail when a derived projection has drifted;
- the next mutation rebuilds a missing, stale, or malformed snapshot before it
  commits;
- intact historical projection pairs are checked against their exact recorded
  revision, including revisions with identical state digests; this permits deliberate
  historical restoration without accepting mismatched revision/digest metadata;
- pre-existing unknown projection drift fails closed before mutation;
- projection drift detected after command publication is preserved and reported
  as a committed/degraded warning rather than overwritten;
- snapshot metadata without any ledger entries blocks reads and mutations; restore
  the authoritative ledger instead of silently re-bootstrapping its projection;
- an intact unsupported snapshot metadata version is preserved and blocks mutation;
- a malformed entry, revision gap, digest mismatch, or illegal transition
  stops replay and leaves the last snapshot in place.

Ledger corruption is not silently truncated or skipped. Recovery must preserve
the offending entry as evidence, establish the last valid revision, and then
move or replace only the exact bad entry under an explicit operator procedure.
The prototype does not add that destructive repair command.

After ledger publication, projection or directory-sync problems return the
normal command result and an explicit stderr warning. Operators must inspect
authoritative task state before manually resubmitting after a warning or an
interrupted response. Process death after ledger rename but before the response
remains ambiguous; caller-supplied operation IDs are required before automatic
retry. A concurrent old writer can also race after the final projection check,
which is why mixed-version operation is unsupported rather than claimed as
fully fail-closed.

This design provides ordered local crash recovery and corruption detection,
not exactly-once external side effects, Byzantine tolerance, cryptographic
authorship, actual power-loss proof, or tamper-proof storage.

## Run-All Outcome Boundary

A task enters `in_progress` before its worker is launched. Completion, failure,
and worker errors are persisted before outcome events or successful summaries
are emitted. An authoritative persistence failure is fatal even with
`--continue-on-critical`; remaining active workers are joined before return,
without launching more tasks or claiming unpersisted outcomes. Inspect and
reconcile any remaining `in_progress` tasks before a manual retry.

A normal critical execution halt also waits for already launched workers, but
records their final statuses and includes their outcomes in the summary. It
stops future launches rather than abandoning ownership of active work. This is
containment by waiting, not cancellation: running workers can continue external
side effects until they finish or their existing timeout expires.

## Writer Map

| State | Current writers | Coordination and decision |
| --- | --- | --- |
| `tasks.json` | task add, fanout, claim/complete/fail, task run, run-all parent | Moved behind the task ledger transaction and process lock. |
| `state.json` | state/settings, runtime task context, sandbox and backend preferences | Separate whole-file atomic writer; concurrent lost-update risk remains, but it is not authoritative task history and is outside this bundle. |
| `task_events.jsonl` | optional run-all queue/start/result/summary events | Observational append-only stream; not used to decide task state. |
| `runs.jsonl` and `schema_failures.jsonl` | execution and schema-failure logging | Observational append-only streams; ordering is not promoted to task authority. |
| quarantine records | schema-failure path, one JSON file per generated quarantine id | Independent evidence records; no task-ledger ownership. |
| signed publication inventory | `scripts/sign_packages.py` restricted staging and immutable directory publication | Separate artifact-identity transaction; no shared runtime publication-state file and no ledger integration. |
| release publication claims | release documents, Git history, CI and external hosting | Outside local runtime authority and unchanged. |

## Validation And Revisit Triggers

Accountable role: XSHELF runtime maintainer.

Acceptance evidence:

- synchronized concurrent add and status tests retain every mutation;
- stale revision, duplicate operation, stale worker, crash-prefix, replay,
  snapshot repair, and corruption tests pass;
- task lifecycle, scheduling, telemetry, quarantine, replay, compatibility, and
  full Rust guardrails remain green.

Freshness: rerun the focused concurrency and ledger tests whenever task storage,
task scheduling, filesystem durability, or supported platforms change.

Revisit this design when remote workers are introduced, queues become large
enough to require compaction, network filesystems become supported, status
semantics are tightened, publication is coordinated by task state, or an
incident shows that state/config/log/quarantine writers need the same ordering
boundary. Only then assess a maintained consensus implementation.
