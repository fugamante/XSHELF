#!/usr/bin/env python3
"""Protect CI ordering and release-health failure isolation without extra deps."""
import pathlib
import re
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/cxrs-compat.yml"


def job(text: str, name: str) -> str:
    match = re.search(rf"^  {re.escape(name)}:\n(.*?)(?=^  [\w-]+:|\Z)",
                      text, re.MULTILINE | re.DOTALL)
    if match is None:
        raise AssertionError(f"missing job: {name}")
    return match.group(1)


class ReleaseWorkflowTests(unittest.TestCase):
    def test_compat_runs_security_without_age_admission(self) -> None:
        compat = job(WORKFLOW.read_text(), "compat-check")
        self.assertIn("--require-published-status-docs", compat)
        self.assertNotIn("--max-version-age-days", compat)
        self.assertNotIn("--max-published-age-days", compat)
        for step in ("Compat check", "Shell Regression Suite", "Cargo Audit Gate",
                     "Cargo Deny Advisories Gate"):
            self.assertIn(f"- name: {step}", compat)

    def test_health_follows_compat_even_on_failure(self) -> None:
        health = job(WORKFLOW.read_text(), "release-health")
        self.assertIn("needs: compat-check", health)
        self.assertRegex(health, r"(?m)^    if: always\(\)$")
        self.assertIn("--max-version-age-days 14", health)
        self.assertIn("--max-published-age-days 14", health)
        self.assertNotIn("continue-on-error", health)

    def test_publication_runs_after_candidate_failure(self) -> None:
        health = job(WORKFLOW.read_text(), "release-health")
        published = health.split("- name: Published release age", 1)[1]
        published = published.split("- name: Release health summary", 1)[0]
        self.assertIn("if: always()", published)
        self.assertIn("set -euo pipefail", published)
        self.assertIn("gh api", published)
        self.assertNotIn("--cadence-exception-label", published)

    def test_label_changes_refresh_exception_evidence(self) -> None:
        text = WORKFLOW.read_text()
        self.assertIn("types: [opened, synchronize, reopened, labeled, unlabeled]", text)
        self.assertIn("contents: read", text)
        self.assertNotIn("pull_request_target:", text)


if __name__ == "__main__":
    unittest.main()
