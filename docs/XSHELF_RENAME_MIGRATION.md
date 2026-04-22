# XSHELF Rename Migration

## Purpose

Move project identity from `CX` to `XSHELF` without breaking the working runtime, machine contracts, or downstream automation that still depends on:

- `bin/cx`
- `CX_*` environment variables
- `.codex/` repo-local state
- `cx`-named examples and compatibility surfaces

This is a staged compatibility migration, not a flag-day rename.

## Current State

What is already true:

- project branding has started moving to `XSHELF (formerly CX)`
- `bin/xshelf` already exists as an alias entrypoint
- `bin/cx` is still the active canonical command surface
- `CX_*` env vars remain the stable runtime configuration contract
- `.codex/` remains the runtime state directory

What that means:

- identity is partially renamed
- runtime compatibility is intentionally still `cx`-shaped
- the repo needs an explicit migration policy so future edits do not drift

## Goals

- establish `XSHELF` as the canonical project name
- reduce namespace ambiguity with other `cx` usages
- preserve runtime stability for users, CI, and downstream repos
- avoid quality or operational regressions caused by a branding-first rename

## Non-Goals

- do not break `bin/cx` in the first migration
- do not rename `.codex/` in the first migration
- do not remove `CX_*` env compatibility in the first migration
- do not silently mutate machine-readable contract keys just for branding
- do not perform repo-wide search-and-replace without a compatibility plan

## Impact Surface

The rename affects multiple layers.

### 1. Branding and Discovery

- repo title and description
- README and docs intros
- changelog/release notes
- issue templates and contributor docs

### 2. Command Surface

- `bin/cx`
- `bin/xshelf`
- shell wrappers
- install/uninstall flows
- examples in docs and manuals

### 3. Runtime Configuration and State

- `CX_*` env vars
- `.codex/` paths
- telemetry/log strings mentioning `cx`
- shell compatibility shims

### 4. Contracts and Integrations

- machine-readable JSON fields and examples
- downstream repos such as `cx-eval-lab`
- CI scripts and wrappers
- contract bundle/export docs

### 5. Social Migration Cost

- existing users know `cx`
- old docs and commands will persist in notes, issues, and scripts
- the migration must explain both names during the transition

## Recommended Migration Path

### Phase 1: Identity-First Rename

Goal:

- make `XSHELF` the canonical product name
- keep the technical compatibility surface stable

Do now:

- use `XSHELF (formerly CX)` in README and top-level docs
- keep `bin/cx` fully supported
- keep `bin/xshelf` as the forward-facing alias
- document the migration policy in contributor docs

Do not do now:

- do not remove `cx` examples wholesale
- do not rename env vars
- do not rename `.codex/`

Exit criteria:

- branding/docs consistently present `XSHELF` first
- no user-facing ambiguity about `XSHELF` vs `cx`
- compatibility promise is explicit

### Phase 2: Dual-Surface Command Migration

Goal:

- make `xshelf` the recommended CLI spelling
- allow `xs` as an intentional short alias
- keep `cx` as a compatibility alias

Do in this phase:

- document `bin/xshelf` first in install and quick-start material
- allow `bin/xs` as the compact shorthand where it improves ergonomics
- keep `bin/cx` examples where compatibility matters
- add explicit docs for:
  - canonical command: `xshelf`
  - short alias: `xs`
  - compatibility alias: `cx`
- update wrappers/installers so both names are supported intentionally

Current status:

- top-level README now leads with `bin/xshelf`
- `bin/xs` is now available as a short runtime alias
- install flow now supports `bin/xshelf-install`
- install flow now supports `bin/xs-install`
- man-page install now publishes `xshelf.1`, `xs.1`, and `cx.1`
- top-level help and usage errors now follow the invoked command name
- `cx` remains fully supported as the compatibility path

Exit criteria:

- new users can start with `xshelf`
- power users can use `xs` when a shorter command is preferable
- existing users can continue with `cx` without breakage
- docs distinguish canonical vs compatibility command names clearly

### Phase 3: Optional Deep Rename Review

Goal:

- decide whether internal identifiers should ever be renamed

Review targets:

- `CX_*` env vars
- `.codex/`
- telemetry field names
- shell function names
- contract labels and examples

Default recommendation:

- do not rename these unless there is a clear long-term payoff
- compatibility cost is higher here than branding benefit

A deep rename should require:

- explicit proposal
- migration notes
- compatibility shim or alias policy
- downstream impact review for `cx-eval-lab` and automation consumers

## Compatibility Rules

These rules apply unless a later approved phase explicitly changes them.

- `bin/cx` remains supported
- `bin/xs` remains a short alias, not the canonical product name
- `CX_*` environment variables remain supported
- `.codex/` remains the runtime state location
- machine-readable JSON contracts are not renamed purely for branding
- downstream repos may continue consuming `cx` compatibility surfaces

## What Should Change Immediately

- project naming in docs and top-level narrative
- migration documentation
- canonical wording:
  - `XSHELF (formerly CX)`
- contributor guidance about what not to rename casually

## What Should Stay Stable For Now

- `bin/cx`
- `CX_*`
- `.codex/`
- machine contract keys and compatibility fixtures
- most shell/test/runtime file names unless they already have a clear `xshelf` alias path

## Downstream Coordination

Any deeper rename phase must review:

- `cx-eval-lab`
- contract bundle/export docs
- CI jobs that invoke `./bin/cx`
- local wrappers, helper scripts, and manuals

The rename only counts as successful if it improves identity without increasing operational drift.

## Acceptance Criteria

A successful migration path:

- makes `XSHELF` the name people see first
- keeps existing `cx` users and automation working
- avoids breaking contracts for aesthetic reasons
- keeps the runtime substrate deterministic and reviewable
- gives maintainers a clear decision rule for future rename-related edits
