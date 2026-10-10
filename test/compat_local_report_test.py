#!/usr/bin/env python3
"""Exercise the local compatibility report path with cheap synthetic steps."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


SOURCE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SOURCE / "scripts"))
import compat_report


class CompatLocalReportTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="compat-local-report-")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.repo = self.base / "repo"
        scripts = self.repo / "scripts"
        scripts.mkdir(parents=True)
        for name in ("compat_local.sh", "compat_report.py"):
            shutil.copy2(SOURCE / "scripts" / name, scripts / name)

        fakebin = self.base / "fakebin"
        fakebin.mkdir()
        bash = fakebin / "bash"
        bash.write_text(
            '#!/bin/sh\nif [ "${1:-}" = "-c" ]; then exit "${FAKE_STEP_RC:-0}"; fi\n'
            'exec /bin/bash "$@"\n',
            encoding="utf-8",
        )
        cargo = fakebin / "cargo"
        cargo.write_text(
            '#!/bin/sh\nif [ "${1:-}" = "--version" ]; then echo "cargo 1.95.0 (synthetic)"; fi\n',
            encoding="utf-8",
        )
        bash.chmod(0o700)
        cargo.chmod(0o700)
        self.env = dict(os.environ)
        self.env["PATH"] = f"{fakebin}{os.pathsep}{os.environ.get('PATH', '')}"
        self.env["PYTHONDONTWRITEBYTECODE"] = "1"

    def run_local(self, *args, step_rc=0):
        env = dict(self.env)
        env["FAKE_STEP_RC"] = str(step_rc)
        return subprocess.run(
            ["/bin/bash", str(self.repo / "scripts" / "compat_local.sh"), "--json", *args],
            cwd=self.repo,
            env=env,
            capture_output=True,
            text=True,
            timeout=20,
            check=False,
        )

    def test_default_new_reuse_and_failed_steps(self):
        report = self.repo / ".cx" / "compat" / "latest.json"
        first = self.run_local()
        self.assertEqual(first.returncode, 0, first.stderr)
        self.assertEqual(json.loads(first.stdout), json.loads(report.read_text()))
        self.assertEqual(report.stat().st_mode & 0o777, 0o600)

        second = self.run_local(step_rc=7)
        self.assertEqual(second.returncode, 1, second.stderr)
        self.assertEqual(json.loads(second.stdout)["status"], "failed")
        self.assertEqual(json.loads(second.stdout), json.loads(report.read_text()))
        self.assertEqual(sorted(p.name for p in report.parent.iterdir()), ["latest.json"])

    def test_default_leaf_symlink_does_not_overwrite_referent(self):
        report = self.repo / ".cx" / "compat" / "latest.json"
        report.parent.mkdir(parents=True)
        victim = self.base / "victim.json"
        victim.write_text("leaf sentinel", encoding="utf-8")
        report.symlink_to(victim)

        result = self.run_local()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertEqual(victim.read_text(), "leaf sentinel")
        self.assertTrue(report.is_symlink())
        self.assertEqual(sorted(p.name for p in report.parent.iterdir()), ["latest.json"])

    def test_default_parent_symlinks_do_not_redirect_report(self):
        outside = self.base / "outside"
        outside.mkdir()
        report = outside / "latest.json"
        report.write_text("parent sentinel", encoding="utf-8")
        (self.repo / ".cx").mkdir()
        (self.repo / ".cx" / "compat").symlink_to(outside, target_is_directory=True)

        result = self.run_local()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertEqual(report.read_text(), "parent sentinel")

        (self.repo / ".cx" / "compat").unlink()
        (self.repo / ".cx").rmdir()
        (outside / "compat").mkdir()
        (outside / "compat" / "latest.json").write_text("ancestor sentinel")
        (self.repo / ".cx").symlink_to(outside, target_is_directory=True)
        result = self.run_local()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((outside / "compat" / "latest.json").read_text(), "ancestor sentinel")

    def test_special_hardlinked_and_readonly_leaf_fail(self):
        report = self.repo / ".cx" / "compat" / "latest.json"
        report.parent.mkdir(parents=True)
        os.mkfifo(report)
        result = self.run_local()
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(report.exists())
        report.unlink()

        victim = self.base / "hardlink-victim"
        victim.write_text("hardlink sentinel")
        os.link(victim, report)
        result = self.run_local()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(victim.read_text(), "hardlink sentinel")
        report.unlink()

        if os.geteuid() != 0:
            report.write_text("readonly sentinel")
            report.chmod(0o444)
            try:
                result = self.run_local()
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(report.read_text(), "readonly sentinel")
            finally:
                report.chmod(0o600)

    def test_unwritable_parent_fails_without_report(self):
        if os.geteuid() == 0:
            self.skipTest("root bypasses directory permission bits")
        parent = self.repo / ".cx"
        parent.mkdir()
        parent.chmod(0o500)
        try:
            result = self.run_local()
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((parent / "compat").exists())
        finally:
            parent.chmod(0o700)

    def test_explicit_output_keeps_operator_selected_paths(self):
        path = self.base / "outside" / "custom.json"
        result = self.run_local("--out", str(path))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), json.loads(path.read_text()))

        relative = Path("custom") / "relative.json"
        result = self.run_local("--out", str(relative))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), json.loads((self.repo / relative).read_text()))

        # --out is an explicit override and retains its prior symlink semantics.
        link = self.base / "outside" / "selected-link.json"
        link.symlink_to(path)
        result = self.run_local("--out", str(link))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), json.loads(path.read_text()))

    def test_parent_swap_after_open_cannot_redirect_default(self):
        payload = '{"status":"ok"}'
        outside = self.base / "outside"
        outside.mkdir()
        (outside / "latest.json").write_text("race sentinel")
        original_replace = compat_report.os.replace

        def swap_parent(src, dst, **kwargs):
            current = self.repo / ".cx" / "compat"
            current.rename(self.repo / ".cx" / "moved-compat")
            current.symlink_to(outside, target_is_directory=True)
            return original_replace(src, dst, **kwargs)

        with mock.patch.object(compat_report.os, "replace", side_effect=swap_parent):
            compat_report.write_default(str(self.repo), "latest.json", payload)

        self.assertEqual((outside / "latest.json").read_text(), "race sentinel")
        self.assertEqual((self.repo / ".cx" / "moved-compat" / "latest.json").read_text(), payload)

    def test_leaf_swap_before_replace_does_not_overwrite_referent(self):
        payload = '{"status":"ok"}'
        victim = self.base / "race-victim"
        victim.write_text("race sentinel")
        original_replace = compat_report.os.replace

        def swap_leaf(src, dst, **kwargs):
            report = self.repo / ".cx" / "compat" / "latest.json"
            report.symlink_to(victim)
            return original_replace(src, dst, **kwargs)

        with mock.patch.object(compat_report.os, "replace", side_effect=swap_leaf):
            compat_report.write_default(str(self.repo), "latest.json", payload)

        self.assertEqual(victim.read_text(), "race sentinel")
        self.assertEqual((self.repo / ".cx" / "compat" / "latest.json").read_text(), payload)

    def test_writer_cli_supports_default_and_explicit_reports(self):
        helper = self.repo / "scripts" / "compat_report.py"
        payload = '{"status":"ok"}'
        default = subprocess.run(
            [sys.executable, str(helper), "--default-root", str(self.repo), "--leaf", "all_latest.json"],
            input=payload,
            capture_output=True,
            text=True,
            timeout=10,
            check=False,
        )
        self.assertEqual(default.returncode, 0, default.stderr)
        self.assertEqual(default.stdout, "")
        self.assertEqual((self.repo / ".cx" / "compat" / "all_latest.json").read_text(), payload)

        root_link = self.base / "repo-link"
        root_link.symlink_to(self.repo, target_is_directory=True)
        linked = subprocess.run(
            [sys.executable, str(helper), "--default-root", str(root_link), "--leaf", "linked.json"],
            input=payload,
            capture_output=True,
            text=True,
            timeout=10,
            check=False,
        )
        self.assertNotEqual(linked.returncode, 0)
        self.assertFalse((self.repo / ".cx" / "compat" / "linked.json").exists())

        explicit_path = self.base / "other" / "report.json"
        explicit = subprocess.run(
            [sys.executable, str(helper), "--out", str(explicit_path)],
            input=payload,
            capture_output=True,
            text=True,
            timeout=10,
            check=False,
        )
        self.assertEqual(explicit.returncode, 0, explicit.stderr)
        self.assertEqual(explicit_path.read_text(), payload)


if __name__ == "__main__":
    unittest.main()
