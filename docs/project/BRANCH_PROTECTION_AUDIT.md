# Branch Protection Audit

This repository supports a solo-maintainer mode for `main`.

Solo-maintainer mode keeps CI/status checks as the practical merge gate while
omitting the required approving-review gate. GitHub does not allow a pull
request author to approve their own pull request, so requiring one approval
blocks a repository that has only one write-capable human maintainer.

The `branch-protection-audit` workflow is a conservative recovery guard:

- when explicitly enabled, it runs daily and can be run manually from `main`;
- it checks for a non-owner direct collaborator with write, maintain, or admin
  access;
- when such a collaborator exists, it restores the minimum review floor on `main`
  after trusted maintainer approval;
- it preserves stricter settings observed during its read. Coordinate external
  protection changes: the GitHub read/PATCH pair is not an atomic transaction.

## Credential boundary

The job uses the `branch-protection-audit` environment. Configure its server-side
rules before enabling the audit or adding a credential:

1. Select branches and tags with exactly one rule: **Branch**, name **`main`**.
   Do not allow tags, wildcards, pull-request refs, or other branches.
2. Require approval by the trusted repository owner only. Disable administrator
   bypass. Allow the owner to approve their own run for solo-maintainer use.
3. Store `BRANCH_PROTECTION_TOKEN` exclusively as an environment secret. Remove
   any repository or inherited organization copy before activation. Check other
   environments for unintended copies as well.
4. Set repository variable `BRANCH_PROTECTION_AUDIT_ENABLED=true` only after
   verifying these controls. Unset or any other value leaves the audit skipped.

Environment rules apply to the workflow run's ref, independently of checkout.
Modified branch YAML cannot remove those server rules to receive the environment
credential. The YAML ref condition is an operational guard, not authorization.
The checkout uses the reviewed run's immutable `github.sha` so its code cannot
silently follow a moving branch tip after approval. Before approving a deployment,
the owner must inspect that run's workflow and source revision; a modified `main`
workflow also requires this approval. Do not approve unreviewed code or grant
another writer authority to bypass the environment gate.

The token must be able to read repository collaborators and update branch
protection. A fine-scoped GitHub token or GitHub App installation token is
preferred over a broad personal token.

No credential is provisioned by the source change. An unset activation variable
keeps scheduled and manual runs inactive without pending approval requests.
Once enabled, each privileged run requires owner approval before execution;
if its environment token is missing, the script retains its successful inactive
notice and makes no API calls. The fallback workflow token remains read-only.

Environment settings are separate from Git source. Before activation and after
permissions, default-branch, visibility/plan, reviewer, or secret-scope changes,
read back the environment rules, branch policy and secret-name inventories.
Default-branch renaming requires an explicit policy/workflow update. Repository
administrators remain trusted and can change these settings. See GitHub's
[environment rules](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments)
and [environment management](https://docs.github.com/en/actions/how-tos/deploy/configure-and-manage-deployments/manage-environments).

## Review floor and compatibility

A missing review gate receives these defaults:

```json
{
  "dismiss_stale_reviews": true,
  "require_code_owner_reviews": false,
  "require_last_push_approval": false,
  "required_approving_review_count": 1
}
```

A stronger approval count or code-owner/last-push requirement is preserved.
Repairs enable stale-review dismissal and raise a zero approval count to one;
they retain the other observed protections. Malformed or incomplete known fields
fail before mutation. Non-target dismissal and bypass settings are not sent in
the PATCH payload.

The script's existing JSON keys and types remain unchanged. `review_gate_matches`
now means the minimum floor is satisfied, including stronger settings, rather
than exact equality with defaults. Dry runs remain non-mutating. Concurrent
external administration changes are not isolated by the read/PATCH sequence.

## Manual dry run

```text
Actions -> branch-protection-audit -> Run workflow -> Branch main -> dry_run=true
```

When activated, inspect and approve the corresponding protected-environment
deployment. A dry run still requires credential approval because collaborator
inspection uses that credential, although no branch-protection PATCH is made.

## Environment probe

`branch-audit-probe` is a manual, inert admission probe with no checkout, secret
expressions or administrative calls. Before adding a credential, verify that a
`main` run waits for owner approval, while branch and tag runs are rejected before
any step. Inspect the probe revision before approving its benign `main` run.
The probe deliberately has no removable YAML ref guard: it exercises the
environment's server rule. A branch-modified workflow cannot weaken that rule.
Do not interpret a mocked policy test as hosted rejection evidence.
