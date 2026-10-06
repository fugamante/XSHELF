#!/usr/bin/env python3
"""Exercise audit decisions without credentials or GitHub mutations."""

import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import unittest
from unittest.mock import patch


sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "branch_audit", ROOT / "scripts/branch_protection_audit.py"
)
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)
WRITER = {"login": "fixture-writer", "type": "User", "permissions": {"push": True}}
FLOOR = {
    "dismiss_stale_reviews": True,
    "require_code_owner_reviews": False,
    "require_last_push_approval": False,
    "required_approving_review_count": 1,
}


class BranchAudit(unittest.TestCase):
    def invoke(self, writers=(), gate=None, **extra):
        calls = []

        def request(token, method, path, payload=None):
            self.assertEqual(token, "synthetic-token")
            calls.append((method, path, payload))
            if method == "GET" and "/collaborators?" in path:
                return list(writers)
            if method == "GET" and path.endswith("required_pull_request_reviews"):
                if gate is None:
                    raise AUDIT.ApiError(404, "fixture missing gate")
                return dict(gate)
            if method == "PATCH" and path.endswith("required_pull_request_reviews"):
                return dict(payload)
            self.fail(f"unexpected API call: {method} {path}")

        env = {
            "GITHUB_REPOSITORY": "fixture-owner/fixture-repo",
            "GITHUB_REPOSITORY_OWNER": "fixture-owner",
            "GITHUB_TOKEN": "synthetic-token",
            "GITHUB_ACTIONS": "true",
            "BRANCH_PROTECTION_TOKEN_PRESENT": "true",
        }
        env.update(extra)
        stdout, stderr = io.StringIO(), io.StringIO()
        with patch.dict(os.environ, env, clear=True), patch.object(
            AUDIT, "api_request", side_effect=request
        ), contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            code = AUDIT.main()
        return code, calls, stdout.getvalue(), stderr.getvalue()

    def test_inactive_actions(self):
        for value in ["false", "", "invalid"]:
            with self.subTest(value=value):
                code, calls, out, _ = self.invoke(
                    [WRITER], BRANCH_PROTECTION_TOKEN_PRESENT=value
                )
                self.assertEqual(code, 0)
                self.assertEqual(calls, [])
                self.assertIn("audit is inactive", out)

    def test_solo_no_mutation(self):
        owner = {"login": "fixture-owner", "type": "User", "permissions": {"admin": True}}
        bot = {"login": "fixture-bot", "type": "Bot", "permissions": {"push": True}}
        code, calls, out, _ = self.invoke([owner, bot])
        self.assertEqual(code, 0)
        self.assertEqual([c[0] for c in calls], ["GET", "GET"])
        self.assertIn("no branch-protection loosening", out)

    def test_writer_restores(self):
        code, calls, out, _ = self.invoke([WRITER])
        self.assertEqual(code, 0)
        self.assertEqual([c[0] for c in calls], ["GET", "GET", "PATCH"])
        self.assertEqual(calls[-1][2], FLOOR)
        self.assertIn("restored required", out)

    def test_dry_no_mutation(self):
        code, calls, out, _ = self.invoke([WRITER], BRANCH_PROTECTION_DRY_RUN="true")
        self.assertEqual(code, 0)
        self.assertEqual([c[0] for c in calls], ["GET", "GET"])
        row = json.loads(out.splitlines()[0])
        self.assertTrue(row["dry_run"])
        self.assertFalse(row["review_gate_present"])
        self.assertIn("would restore", out)

    def test_existing_no_mutation(self):
        code, calls, out, _ = self.invoke([WRITER], FLOOR)
        self.assertEqual(code, 0)
        self.assertEqual([c[0] for c in calls], ["GET", "GET"])
        self.assertTrue(json.loads(out.splitlines()[0])["review_gate_matches"])

    def test_stricter_no_mutation(self):
        for count in [2, 6]:
            with self.subTest(count=count):
                gate = dict(FLOOR, required_approving_review_count=count,
                            require_code_owner_reviews=True, require_last_push_approval=True)
                code, calls, out, _ = self.invoke([WRITER], gate)
                self.assertEqual(code, 0)
                self.assertEqual([c[0] for c in calls], ["GET", "GET"])
                self.assertTrue(json.loads(out.splitlines()[0])["review_gate_matches"])

    def test_partial_preserves_stricter(self):
        gate = dict(FLOOR, required_approving_review_count=2,
                    dismiss_stale_reviews=False, require_code_owner_reviews=True,
                    require_last_push_approval=True)
        code, calls, _, _ = self.invoke([WRITER], gate)
        self.assertEqual(code, 0)
        self.assertEqual(calls[-1][0], "PATCH")
        self.assertEqual(calls[-1][2], dict(gate, dismiss_stale_reviews=True))

    def test_zero_preserves_flags(self):
        gate = dict(FLOOR, required_approving_review_count=0,
                    require_code_owner_reviews=True, require_last_push_approval=True)
        code, calls, _, _ = self.invoke([WRITER], gate)
        self.assertEqual(code, 0)
        self.assertEqual(calls[-1][2], dict(gate, required_approving_review_count=1))

    def test_malformed_no_mutation(self):
        for key, value in [("required_approving_review_count", True),
                           ("required_approving_review_count", "2"),
                           ("required_approving_review_count", -1),
                           ("required_approving_review_count", 7),
                           ("dismiss_stale_reviews", "true"),
                           ("require_code_owner_reviews", 1),
                           ("require_last_push_approval", None)]:
            with self.subTest(key=key, value=value):
                gate = dict(FLOOR, **{key: value})
                with self.assertRaisesRegex(RuntimeError, "protection unchanged"):
                    self.invoke([WRITER], gate)

        for key in FLOOR:
            with self.subTest(missing=key):
                gate = dict(FLOOR)
                del gate[key]
                with self.assertRaisesRegex(RuntimeError, "protection unchanged"):
                    self.invoke([WRITER], gate)

    def test_local_dry_control(self):
        code, calls, out, _ = self.invoke(
            [WRITER], GITHUB_ACTIONS="false", BRANCH_PROTECTION_TOKEN_PRESENT="false",
            BRANCH_PROTECTION_DRY_RUN="true"
        )
        self.assertEqual(code, 0)
        self.assertEqual([c[0] for c in calls], ["GET", "GET"])
        self.assertIn("would restore", out)

    def test_missing_token(self):
        code, calls, _, err = self.invoke(GITHUB_TOKEN="")
        self.assertEqual(code, 2)
        self.assertEqual(calls, [])
        self.assertIn("GITHUB_TOKEN is required", err)

    def test_invalid_repository(self):
        code, calls, _, err = self.invoke(GITHUB_REPOSITORY="fixture-repo")
        self.assertEqual(code, 2)
        self.assertEqual(calls, [])
        self.assertIn("OWNER/REPO", err)

    def test_workflow_boundary(self):
        text = (ROOT / ".github/workflows/branch-protection-audit.yml").read_text()
        self.assertIn("\n    environment: branch-protection-audit\n", text)
        self.assertIn("if: github.ref == 'refs/heads/main' && "
                      "vars.BRANCH_PROTECTION_AUDIT_ENABLED == 'true'", text)
        self.assertIn("\n          ref: ${{ github.sha }}\n", text)
        self.assertIn("\n          persist-credentials: false\n", text)
        self.assertIn("\n  contents: read\n", text)
        self.assertIn("BRANCH_PROTECTION_DRY_RUN: ${{ inputs.dry_run || 'false' }}", text)


if __name__ == "__main__":
    unittest.main()
