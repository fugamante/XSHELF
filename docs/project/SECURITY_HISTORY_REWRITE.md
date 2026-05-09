# Security History Rewrite Notice

Date: 2026-02-26

## Summary

Repository history was rewritten to remove locally identifying strings and path leakage from prior commits.

Sanitized patterns:

- `<home>/cxcodex` -> `/path/to/cxcodex`
- `<home>` -> `/home/user`
- `<local-user>` -> `user`

Additionally, previously committed build artifacts under `rust/cxrs/target/` were removed from all historical commits.

## Safety Backups

Before rewrite, local recovery bundles were created:

- `.git/history-backups/pre-sanitize-20260226-221249.bundle`
- `.git/history-backups/pre-sanitize-target-prune-20260226-221312.bundle`

## Remote Impact

Because commit SHAs changed, branches/tags were force-pushed:

- `main`
- `codex/rust-spike`
- `codex/rust-refactor`
- rewritten tag refs (including `v2026.02.21-20260225T151634Z`)

## Required Action for Collaborators

All collaborators must resync local clones to the rewritten history.

Option A (recommended): fresh clone

```bash
git clone https://github.com/fugamante/cxcodex.git
```

Option B: hard reset existing clone

```bash
git fetch --all --prune --tags
git checkout main
git reset --hard origin/main
git branch -D codex/rust-spike codex/rust-refactor 2>/dev/null || true
git checkout -b codex/rust-spike origin/codex/rust-spike
git checkout -b codex/rust-refactor origin/codex/rust-refactor
git reflog expire --expire=now --expire-unreachable=now --all
git gc --prune=now --aggressive
```

## Verification Performed

- Scanned current tracked files for local username/path patterns.
- Scanned all reachable commits for local username and path patterns.
- Expired reflogs and pruned unreachable objects locally.

No remaining matches were found in reachable history.
