#!/usr/bin/env python3
"""Synthetic child-result regressions for the aggregate compatibility runner."""

import json
import subprocess
import tempfile
import unittest
from pathlib import Path


RUNNER = Path(__file__).resolve().parents[1] / "scripts" / "compat_all.sh"


class CompatAllTest(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="compat-all-test-")
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)

    def child(self, name, exit_code, status="ok", steps_failed=0, raw=None):
        repo = self.root / name
        scripts = repo / "scripts"
        scripts.mkdir(parents=True)
        payload = json.dumps(
            {
                "status": status,
                "mode": "quick",
                "summary": {"steps_total": 1, "steps_failed": steps_failed},
            }
        )
        script = scripts / "compat_local.sh"
        if raw is not None:
            payload = raw
        write_report = (
            "cat > \"$out\" <<'JSON'\n" f"{payload}\n" "JSON\n"
            if payload
            else ""
        )
        script.write_text(
            "#!/usr/bin/env bash\n"
            "set -euo pipefail\n"
            "out=\n"
            "while (($#)); do\n"
            "  if [[ \"$1\" == --out ]]; then out=\"$2\"; shift 2; else shift; fi\n"
            "done\n"
            f"{write_report}"
            f"exit {exit_code}\n",
            encoding="utf-8",
        )
        script.chmod(0o755)
        return repo

    def run_all(self, *repos):
        report_path = self.root / "aggregate.json"
        command = [str(RUNNER), "--quick", "--json", "--out", str(report_path)]
        for repo in repos:
            command += ["--repo", str(repo)]
        completed = subprocess.run(command, capture_output=True, text=True, check=False)
        self.assertTrue(report_path.is_file(), completed.stderr)
        report = json.loads(report_path.read_text(encoding="utf-8"))
        self.assertEqual(json.loads(completed.stdout), report)
        return completed.returncode, report

    def run_default(self, repo, caller):
        return subprocess.run(
            [str(RUNNER), "--quick", "--json", "--repo", str(repo)],
            cwd=caller,
            capture_output=True,
            text=True,
            check=False,
        )

    def test_successful_child_passes(self):
        rc, report = self.run_all(self.child("good", 0))
        self.assertEqual(rc, 0)
        self.assertEqual(report["status_final"], "PASS")
        self.assertEqual(report["summary"], {"repos_total": 1, "repos_failed": 0})
        self.assertEqual(report["repos"][0]["exit_code"], 0)

    def test_nonzero_child_exit_overrides_ok_report(self):
        # The child process result is authoritative even when its JSON looks green.
        rc, report = self.run_all(self.child("exits-seven", 7))
        self.assertEqual(rc, 1)
        self.assertEqual(report["status_final"], "FAIL")
        self.assertEqual(report["summary"], {"repos_total": 1, "repos_failed": 1})
        self.assertEqual(report["repos"][0]["exit_code"], 7)
        self.assertEqual(report["repos"][0]["result"]["status"], "ok")

    def test_mixed_children_keep_exit_codes_and_failure_count(self):
        good = self.child("good", 0)
        exited = self.child("exits-three", 3)
        reported = self.child("reports-fail", 0, "failed", 1)
        missing = self.root / "no-script"
        missing.mkdir()

        rc, report = self.run_all(good, exited, reported, missing)
        self.assertEqual(rc, 1)
        self.assertEqual(report["status_final"], "FAIL")
        self.assertEqual(report["summary"], {"repos_total": 4, "repos_failed": 3})
        self.assertEqual([row["exit_code"] for row in report["repos"]], [0, 3, 0, 127])
        self.assertEqual(report["repos"][3]["result"]["error"], "missing compat_local.sh")

    def test_child_exit_without_report_keeps_aggregate(self):
        good = self.child("good", 0)
        empty = self.child("no-report", 7, raw="")
        rc, report = self.run_all(good, empty)
        self.assertEqual(rc, 1)
        self.assertEqual(report["status_final"], "FAIL")
        self.assertEqual(report["summary"], {"repos_total": 2, "repos_failed": 1})
        self.assertEqual([row["exit_code"] for row in report["repos"]], [0, 7])
        self.assertEqual(
            report["repos"][1]["result"]["error"],
            "missing or invalid compat report",
        )

    def test_invalid_child_report_fails_even_with_zero_exit(self):
        bad = self.child("bad-report", 0, raw="{invalid json")
        rc, report = self.run_all(bad)
        self.assertEqual(rc, 1)
        self.assertEqual(report["status_final"], "FAIL")
        self.assertEqual(report["summary"], {"repos_total": 1, "repos_failed": 1})
        self.assertEqual(report["repos"][0]["exit_code"], 0)
        self.assertEqual(
            report["repos"][0]["result"]["error"],
            "missing or invalid compat report",
        )

    def test_wrong_child_report_shape_keeps_aggregate(self):
        bad = self.child("wrong-shape", 0, raw='{"status":"ok","summary":42}')
        rc, report = self.run_all(bad)
        self.assertEqual(rc, 1)
        self.assertEqual(report["summary"], {"repos_total": 1, "repos_failed": 1})
        self.assertEqual(report["repos"][0]["exit_code"], 0)
        self.assertEqual(
            report["repos"][0]["result"]["error"],
            "missing or invalid compat report",
        )

    def test_same_basename_cannot_reuse_previous_child_report(self):
        good = self.child("first/shared", 0)
        silent = self.child("second/shared", 0, raw="")
        rc, report = self.run_all(good, silent)
        self.assertEqual(rc, 1)
        self.assertEqual(report["summary"], {"repos_total": 2, "repos_failed": 1})
        self.assertEqual(report["repos"][0]["result"]["status"], "ok")
        self.assertEqual(
            report["repos"][1]["result"]["error"],
            "missing or invalid compat report",
        )

    def test_default_report_reuses_private_caller_path(self):
        caller = self.root / "caller"
        caller.mkdir()
        child = self.child("good", 0)
        report = caller / ".cx" / "compat" / "all_latest.json"
        for _ in range(2):
            result = self.run_default(child, caller)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout), json.loads(report.read_text()))
        self.assertEqual(report.stat().st_mode & 0o777, 0o600)
        self.assertEqual(sorted(p.name for p in report.parent.iterdir()), ["all_latest.json"])

    def test_default_report_rejects_leaf_and_parent_symlinks(self):
        child = self.child("good", 0)
        caller = self.root / "caller"
        report = caller / ".cx" / "compat" / "all_latest.json"
        report.parent.mkdir(parents=True)
        victim = self.root / "victim.json"
        victim.write_text("leaf sentinel")
        report.symlink_to(victim)
        result = self.run_default(child, caller)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertEqual(victim.read_text(), "leaf sentinel")

        report.unlink()
        report.parent.rmdir()
        outside = self.root / "outside"
        outside.mkdir()
        (outside / "all_latest.json").write_text("parent sentinel")
        report.parent.symlink_to(outside, target_is_directory=True)
        result = self.run_default(child, caller)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertEqual((outside / "all_latest.json").read_text(), "parent sentinel")


if __name__ == "__main__":
    unittest.main()
