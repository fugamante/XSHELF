# Contract Compatibility Policy

Last updated: 2026-10-09

## Scope

This policy defines compatibility guarantees for machine-readable XSHELF outputs used by automation and CI.
Canonical command examples use `xshelf`; `cx` remains the supported compatibility alias for existing automation.

Covered JSON surfaces:
- `xshelf version --json`
- `xshelf core --json`
- `xshelf diag --json`
- `xshelf scheduler --json`
- `xshelf optimize --json`
- `xshelf logs stats --json` (and `xshelf telemetry --json`)
- `xshelf broker benchmark --json`
- `xshelf broker show --json`
- `xshelf policy show --json`
- `xshelf task check --json`
- `xshelf task run-plan --json`
- `xshelf task run-all --json`
- `xshelf task list --json`
- `xshelf task show <id> --json`
- `xshelf task run <id> --json`
- `xshelf llm verify mlx --json`
- `xshelf llm resident show --json`
- `xshelf llm resident probe-models --json`

## Version Markers

Each covered payload includes a top-level `contract_version` field.

Current versions:
- `version.v1`
- `core.v1`
- `diag.v1`
- `scheduler.v1`
- `optimize.v1`
- `telemetry.v1`
- `broker-benchmark.v1`
- `broker-show.v1`
- `policy-show.v1`
- `task-check.v1`
- `task-run-plan.v1`
- `task-run-all.v1`
- `task-list.v1`
- `task-show.v1`
- `task-run.v1`
- `llm-verify.v1`
- `llm-resident.v1`
- actions extension: `actions.v1` (`actions_contract_version`)

## Stability Rules

Patch releases:
- no key removals on stable contracts
- no type changes for existing keys
- additive keys are allowed only with fixture/test updates
- shared additive guidance objects, such as `operator_context`, may appear on
  multiple inspection surfaces without a version bump when existing keys and
  exit semantics are preserved

Minor releases:
- additive fields allowed with changelog notes
- behavior changes must preserve existing strict/exit-code semantics unless explicitly documented

Major releases:
- breaking contract changes allowed only with migration notes and version bump

The fix-run policy hardening changes command acceptance and execution. The
`xshelf policy show --json` `rules` list adds launcher/interpreter and leading
assignment restrictions. Its `policy-show.v1` marker, keys, and types remain
unchanged; clients should treat rule entries as an extensible list.

## Task Command Admission

Repository command-style task objectives now require the process-only
`CX_TASK_TRUST_COMMANDS=1` opt-in. This intentional security default change applies
equally to text, JSON, overrides, replicas and managed workers. Denied runs retain
`task-run.v1` with status `failed`, null execution_id and nonzero exit status;
run-all retains existing failed accounting and task status/event transitions.
No stable keys or types change. Ordinary prompt tasks and direct operator capture
remain unchanged. Authorized command fixtures cover the previous behavior.
See the migration in `docs/orchestration/PHASE_VI_EXECUTION_GUIDANCE.md`; provider
trust and command trust are separate, and Docker trust remains unresolved.

The planned `task run-all` dependency/resource waves now gate provider launch in
mixed and parallel modes. If a scheduled prerequisite has not completed
successfully, its dependent is persisted as failed and reported with the
`dependency_blocked` failure class and a blocked task event. Existing
`task-run-all.v1` keys and types remain. Blocked dependents count in the failed
and blocked totals and produce a nonzero exit; the failure-class value is
additive. Independent tasks in a compatible wave retain parallel execution,
and explicit backend selection remains authoritative.

## Local Model Authority

Repository `.cx/state.json` preferences and `.cx/local_models.json` records remain
readable data, but they cannot select an executable backend, model, or alias
target solely by being present in a checkout. The operator must reselect legacy
repository choices with `llm use`, `llm set-backend`, or `llm set-model`, and
approve registry aliases with `llm models add`; these actions create a private
user-home approval bound to the checkout and exact approved values. A changed
repository value fails closed at execution. Explicit process environment values
and authorized task provider overrides remain separate authority paths. Existing
JSON keys and version markers are unchanged; denied provider execution returns
an error instead of silently switching to an unapproved model.

Parity mock schema setup accepts regular `.json` entries opened without
following symlinks. It requires a trusted temporary-directory parent and
private Unix directory access, limits each copy to 64 MiB and all copies to
256 MiB, and removes partial output on failure. These checks do not change the
parity report schema.

## CI Enforcement

Contract stability is enforced by:
- `xshelf contracts export --profile full --json` for the declared compatibility
  surface manifest
- the `eval-lab` validation fixture is embedded in the Rust binary so packaged
  `xshelf contracts validate --profile eval-lab --json` preserves the declared
  action set, contract versions, required keys, and strict drift result without
  depending on a source checkout; embedding changes fixture availability, not
  the contract surface
- fixture-backed integration tests under `rust/cxrs/tests/fixtures/*_contract.json` for the fixture-locked surfaces
- targeted integration assertions for typed JSON surfaces that do not yet have standalone fixture manifests (`policy show`, `llm verify`, `llm resident`)
- fixture-backed local sidecar assertions for `llm resident probe-models --json`
  to preserve the loopback `/v1/models` path and visible HTTP boundary fields
- strict run-log validation requires HTTP provenance keys
  (`http_request_profile`, `http_provider_format`, `http_parser_mode`) on every
  modern row; `xshelf logs migrate` backfills unknown historical values as
  nullable fields
- run logs may carry additive nullable command-provenance fields such as
  `system_status`; these fields must be preserved by migration but are not
  required for historical rows when validation uses `--legacy-ok`
- `CX_LOG_FILE` may relocate the run-log destination for `capture`, `budget`,
  and `trace` without changing the JSONL row contract; when unset, repository
  state remains under `.cx/cxlogs/runs.jsonl`
- modern `capture` run-log rows must include integer `system_status` so
  `xshelf logs validate --strict` can catch regressions where wrapped command
  exit status telemetry is lost
- quarantine records are read through an integrity guard: `quarantine show` and
  `replay` reject records whose embedded id or prompt/raw hashes do not match
  the requested record and payload
- strict run-log validation follows modern schema-failure `quarantine_id`
  references and reports unreadable or integrity-invalid quarantine records
- command-surface docs gates that include covered contract producer/version and
  fixture files
- strict lint/test gates in `.github/workflows/cxrs-compat.yml`
- `cargo test --tests -- --test-threads=1`

## Change Process

The execution-policy hardening retains `policy-show.v1`, its keys/types, and
existing rule strings; its extensible rule list gains privilege-launcher and
system/disk-control restrictions. Parsed arguments now govern suggested-command
classification and relative write targets use the execution directory. Callers
that intentionally suggest system-control or disk-management tools must explicitly
opt into unsafe execution. Invalid command quoting fails closed. Diagnostic
`policy check` retains its advisory repository-local redirection behavior.
`watch` delegates execution and requires an unsafe override, as do disruptive
`systemctl` sleep actions. Copy source operands may be external reads while copy
destinations remain repository-contained; source-derived `--parents` forms, directory sources and hardlink creation retain
conservative checks. Move sources still require containment because they are removed.

Log migration retains normalized run-log fields, summary counters, and the
`in`, `out`, `backup`, and status labels. Filesystem behavior changes intentionally:
symlink descendants fail closed; output equal to input requires `--in-place`;
backup names are unique and consumers must use the printed path. In-place `--out`
selects staging, preserving any preexisting output. Migration requires quiescent
log writers and anchored filesystem support. Errors after publication identify
that replacement occurred without claiming rollback or confirmed durability.
New files use `0600` (including the replaced source), and new directories use
`0700`. Publication sync does not guarantee persistence of newly created ancestors.

MLX verification preserves `llm-verify.v1` keys/types and stored model metadata.
`preferred_args` presence does not imply execution: execution is disabled unless
`CX_MLX_TRUST_REGISTRY_ARGS` explicitly enables it. Operator `CX_MLX_ARGS` remains
the final override. Benchmark now requires nonblank `CX_MLX_VERIFY_SCRIPT`;
otherwise it returns an actionable stderr error with no JSON payload or interpreter
invocation. Temporary benchmark storage is private and removed on return.

When changing a covered JSON contract:
1. Update producing code.
2. Update fixture contract file(s).
3. Update tests validating contract keys/types.
4. Update `CHANGELOG.md`.
5. Bump `contract_version` only for breaking changes.

## Docker authority migration (2026-10-06)

Repository sandbox requests no longer authorize image or repository executable
execution. Runtime and readiness require process sandbox approval and a reviewed
full local immutable image ID; repository wrappers and named credential sharing
have separate opt-ins. Disabled or denied readiness executes no container, and
runtime denial does not fall back to host execution. See orchestration execution
guidance for migration and limits. Task, readiness, status, event and log keys,
types and contract versions remain unchanged. Existing readiness booleans are
false when checks are not admitted; issues/recommended_action explain denial.
The existing container lane-detail string now records the approved immutable
image ID instead of a mutable tag. No new credential values enter log contracts.
