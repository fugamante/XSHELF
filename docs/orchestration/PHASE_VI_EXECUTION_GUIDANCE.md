# Phase VI: Execution Guidance Contract

Status: active

## Objective

Define the stable operator-guidance contract produced by the early Phase VI substrate work.

This document is about surfaced guidance, not scheduler semantics. The scheduler stays explicit and conservative. The contract here exists so text surfaces, JSON consumers, and future UI/session layers read the same execution state without recomputing policy independently.

## Stable Concepts

### Task Readiness

`task_readiness` answers whether a selected task set can run, how parallel it really is, and which mode is currently recommended.

Current stable fields:

- `can_run`
- `can_run_mixed`
- `can_run_parallel`
- `strict_plan_ok`
- `strict_plan_reason`
- `sequential_waves`
- `parallel_waves`
- `largest_parallel_wave`
- `recommended_mode`
- `recommended_reason`

Current surfaces:

- `xshelf task check --json`
- `xshelf task run-all --json`
- `xshelf diag --json`
- `xshelf scheduler --json`
- `xshelf doctor`
- `cx-lean-session`

### Task Execution

`task_execution` answers what happened recently during `task run-all`, what the primary next step is, and whether recent queue pressure implies a narrower rerun.

Current stable fields:

- `last_mode`
- `halted_remaining`
- `backend_fallback_rows`
- `advice`
- `recommendations`
- `next_action`
  - `kind`
  - `command`
  - `reason`
- `wave_pressure`
  - `kind`
  - `suggested_mode`
  - `latest_wave_index`
  - `max_queue_wave_index`
  - `max_queue_wave_ms`

Current surfaces:

- `xshelf diag --json`
- `xshelf telemetry N --json`
- `xshelf scheduler --json`
- `xshelf optimize N --json --actions`
- `xshelf doctor`
- `cx-lean-session`

### Per-Task Run Readiness

`run_readiness` answers whether one task is runnable now, delayed to a later wave, blocked, or inspect-only.

Current stable fields:

- `status_filter`
- `selected_status_count`
- `runnable_now`
- `wave_index`
- `wave_mode`
- `blocked_reason`
- `dependencies`
- `resource_keys`
- `recommended_command`
- `recommended_reason`

Current surfaces:

- `xshelf task show <id>`
- `xshelf task list --json`

### List Readiness

`list_readiness` summarizes the currently selected task list without requiring row iteration.

Current stable fields:

- `selected_count`
- `runnable_now_count`
- `blocked_now_count`
- `inspect_only_count`
- `wave_count`
- `blocked_count`
- `next_wave`
  - `index`
  - `mode`
  - `size`

Current surfaces:

- `xshelf task list --json`
- `xshelf task list`

## Single-Source Rule

Execution guidance must remain single-sourced.

That means:

- text surfaces should format shared guidance objects
- JSON surfaces should expose shared guidance objects directly
- no UI/session layer should invent its own recommendation policy when a typed contract already exists

## Phase VI Boundaries

Early Phase VI work is complete only if these remain true:

- no silent scheduler behavior change is introduced through guidance work
- `parallel` stays explicit
- `mixed` stays honest about wave serialization
- recent wave pressure can narrow advice, but cannot silently override requested execution mode

## Merge Expectations

Any future change to these objects must include:

1. JSON contract test updates where applicable
2. text-surface validation where applicable
3. an explicit note in `docs/project/ROADMAP.md` or the relevant phase doc if the semantic contract changed

## Task Provider Trust and Migration

Task records remain repository data. `task run` and `task run-all` ignore stored
`backend` and `model` overrides unless the operator explicitly supplies
`CX_TASK_TRUST_PROVIDER=1` in the process environment. No repository state or
configuration key enables this trust. Review the task records and local model
registry before opting in:

```bash
CX_TASK_TRUST_PROVIDER=1 xshelf task run <id>
CX_TASK_TRUST_PROVIDER=1 xshelf task run-all
```

Without that opt-in, execution uses the operator's configured backend and model.
Explicit `task run --backend` and `task run-all --backend-pool` remain authoritative;
stored model overrides still require the opt-in. Existing task keys and JSON
contract versions are preserved. Existing workflows that intentionally use stored
provider/model selection must add the process environment opt-in after review.

## Task Command Authority and Migration

Repository task objectives are data, not operator authorization. Recognized
command objectives require `CX_TASK_TRUST_COMMANDS=1` in the invoking process
environment, after reviewing the tasks and commands they can execute:

```bash
CX_TASK_TRUST_COMMANDS=1 xshelf task run <id>
CX_TASK_TRUST_COMMANDS=1 xshelf task run-all
```

Only the value `1` (with surrounding whitespace allowed) grants authority.
Repository preferences, provider trust, backend/mode overrides and sandbox-active
markers do not grant it. The grant applies to all recognized task command
objectives, including commit/diff helpers and the `cx`, `cxj`, `cxo`, next, fix
and fix-run aliases. Existing deliberate command workflows must add this process
opt-in; do not place it in repository state or automatically source untrusted
repository configuration. Inherited grants are operator authority for that
process tree, so prefer per-invocation scope rather than a global shell export.

Without authority, a command task fails before command callbacks, subprocesses,
container handoff, replicas or model judging. It is not reinterpreted as a prompt.
Text and JSON modes enforce the same boundary; stable status/event keys and JSON
versions remain unchanged. Managed run-all failures retain existing generic
failure classification; inspect a single task's stderr for the approval guidance.

Ordinary prompt tasks, including unrecognized top-level command text, still use
the selected provider without command authority. Direct operator-invoked capture
commands are unchanged. Command authority does not grant stored provider/model
trust or bypass existing fix-run policy/unsafe controls. It is not a command
sandbox, environment scrubber or protection from code already running as the
same user. Docker image, readiness-probe and credential-sharing trust require
separate controls; this grant alone does not establish safe container operation.

## Docker Task Authority and Migration

Repository sandbox settings are requested configuration. Before runtime or a
readiness container, the operator must approve the image and set process-only
`CX_TASK_TRUST_SANDBOX=1` and `CX_TASK_SANDBOX_IMAGE` to a full lowercase
`sha256:` image ID (64 hexadecimal digits). Obtain that ID by inspecting a
reviewed local image; tags, missing IDs, whitespace, option-like operands and
invalid process overrides are denied without fallback to repository settings.
The local inspect result must match the approved ID; no image is pulled.

The same admission controls apply before replicas and model judging, including
text, JSON, managed and run-all paths. A missing grant fails the task before
execution; it does not silently use the host. `CX_TASK_SANDBOX_ACTIVE` controls
recursion only and cannot grant authority. Disabled readiness is diagnostics
only and never starts Docker. Sandbox show/check/task JSON keys and versions
remain stable; denied readiness returns issues and existing false availability
fields. Process image overrides appear in the existing image field.

Runtime and readiness use `/bin/bash` without login/profile startup, fixed PATH
and startup variables, and no image healthcheck. Default image application paths
are `/usr/local/bin/xshelf` then `/usr/local/bin/cx`. A different installation
requires process `CX_TASK_SANDBOX_EXECUTABLE` with a reviewed absolute image path
outside `/work`. No PATH search or repository wrapper is automatic. The
compatibility image contains a toolchain, not XSHELF: after reviewing repository
code, `CX_TASK_TRUST_REPO_EXEC=1` permits `./bin/xshelf` / `./bin/cx` fallback.
Image applications remain preferred. Both probes and runtime share this rule.

No broad CX, provider or proxy environment group is forwarded. To share
necessary credentials or provider command/model configuration, explicitly select
comma-separated exact variable names with `CX_TASK_SANDBOX_SHARE_ENV` in the
invoking process, after reviewing both names and values. Empty means no sharing;
unset selected variables, malformed/empty names and non-Unicode values fail.
Wildcard/prefix selectors are unsupported. Task authority/configuration,
execution provenance, backend/unsafe controls, Docker client controls and
shell/path/startup keys are reserved; selectors cannot regrant authority.
Selected values are passed through Docker's client environment by name and are
not placed in its command arguments. They are still visible to the approved
container and Docker daemon. Readiness shares no selected credentials.

Fixed transport preserves the operator's command/provider grants and unsafe
controls, normalized backend and replica metadata. Absent grants are explicit
zero so image environment defaults cannot enable them. Inner task markers and
sandbox authority are constructed by the admitted parent; inherited process
variables still represent operator authority, so prefer per-invocation scope.
Host-specific source/log paths are not implicitly forwarded. Provider execution
inside the image requires deliberately installed/configured tooling or explicit
selected configuration; sharing is not authorization to use stored providers.

Authorized runtime deliberately mounts the repository read-write with normal
Docker networking. Readiness uses a read-only root/repository, no network and
executable-access tests; its host permission check does not prove runtime writes.
Container logs record `docker:<immutable ID>` in the existing lane-detail string.
This boundary does not sandbox reviewed images/repository code, prevent container
escapes, or scrub files/credentials already inside an approved image. Linked
worktree external Git metadata and prior log concurrency/durability limitations
remain. These controls do not weaken `CX_TASK_TRUST_COMMANDS`, provider authority
or deliberate fix-run unsafe overrides.
