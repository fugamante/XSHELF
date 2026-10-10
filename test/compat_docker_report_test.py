#!/usr/bin/env python3
"""Exercise the Docker compatibility report sink without starting Docker."""

import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


SOURCE = Path(__file__).resolve().parents[1] / "scripts" / "compat_docker.sh"


class CompatDockerReportTest(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="compat-docker-report-")
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.repo = self.root / "repo"
        scripts = self.repo / "scripts"
        scripts.mkdir(parents=True)
        self.runner = scripts / "compat_docker.sh"
        shutil.copy2(SOURCE, self.runner)
        shutil.copy2(SOURCE.parent / "compat_report.py", scripts / "compat_report.py")
        (self.repo / "fixture.txt").write_text("synthetic repository\n", encoding="utf-8")
        subprocess.run(["git", "init", "-q", str(self.repo)], check=True)
        subprocess.run(["git", "-C", str(self.repo), "add", "fixture.txt"], check=True)
        subprocess.run(
            [
                "git", "-C", str(self.repo),
                "-c", "user.name=Synthetic Test",
                "-c", "user.email=synthetic@example.invalid",
                "-c", "commit.gpgsign=false",
                "commit", "-q", "-m", "synthetic",
            ],
            check=True,
        )

        mockdir = self.root / "mock-bin"
        mockdir.mkdir()
        docker = mockdir / "docker"
        docker.write_text(
            "#!/usr/bin/env bash\n"
            "set -euo pipefail\n"
            "case \"${1:-}\" in\n"
            "  image)\n"
            "    [[ \"${2:-}\" == inspect ]] || exit 91\n"
            "    if [[ \" $* \" == *' --format '* ]]; then\n"
            "      printf 'sha256:synthetic\\n'\n"
            "    fi\n"
            "    ;;\n"
            "  run) exit 0 ;;\n"
            "  --version) printf 'Docker synthetic 1.0\\n' ;;\n"
            "  *) printf 'unexpected docker call: %s\\n' \"$*\" >&2; exit 92 ;;\n"
            "esac\n",
            encoding="utf-8",
        )
        docker.chmod(0o755)
        self.env = os.environ.copy()
        self.env["PATH"] = f"{mockdir}:{self.env['PATH']}"
        self.env["PYTHONDONTWRITEBYTECODE"] = "1"
        self.env.pop("CX_COMPAT_IMAGE", None)

    def run_smoke(self, *args):
        return subprocess.run(
            [str(self.runner), "--smoke", "--json", *args],
            cwd=self.repo,
            env=self.env,
            capture_output=True,
            text=True,
            check=False,
        )

    def test_default_report_reuse_and_json(self):
        report_path = self.repo / ".cx/compat/docker_smoke_latest.json"
        for _ in range(2):
            completed = self.run_smoke()
            self.assertEqual(completed.returncode, 0, completed.stderr)
            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertEqual(json.loads(completed.stdout), report)
            self.assertEqual(report["mode"], "smoke")
            self.assertEqual(report["status"], "ok")
            self.assertEqual(report["summary"], {"steps_total": 3, "steps_failed": 0})
            self.assertEqual(report["docker"]["image_id"], "sha256:synthetic")
            self.assertEqual(report["docker"]["pull_policy"], "never")

    def test_explicit_output_and_json(self):
        report_path = self.root / "operator" / "chosen.json"
        completed = self.run_smoke("--out", str(report_path))
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(json.loads(completed.stdout), json.loads(report_path.read_text()))
        self.assertFalse((self.repo / ".cx/compat/docker_smoke_latest.json").exists())

    def test_default_leaf_symlink_does_not_clobber(self):
        report_dir = self.repo / ".cx/compat"
        report_dir.mkdir(parents=True)
        outside = self.root / "outside-leaf.json"
        sentinel = b"owned leaf sentinel\n"
        outside.write_bytes(sentinel)
        (report_dir / "docker_smoke_latest.json").symlink_to(outside)

        completed = self.run_smoke()
        self.assertTrue(outside.read_bytes() == sentinel, "default report overwrote leaf target")
        self.assertNotEqual(completed.returncode, 0, completed.stdout)

    def test_default_parent_symlink_does_not_clobber(self):
        (self.repo / ".cx").mkdir()
        outside_dir = self.root / "outside-parent"
        outside_dir.mkdir()
        outside = outside_dir / "docker_smoke_latest.json"
        sentinel = b"owned parent sentinel\n"
        outside.write_bytes(sentinel)
        (self.repo / ".cx/compat").symlink_to(outside_dir, target_is_directory=True)

        completed = self.run_smoke()
        self.assertTrue(outside.read_bytes() == sentinel, "default report overwrote parent target")
        self.assertNotEqual(completed.returncode, 0, completed.stdout)


if __name__ == "__main__":
    unittest.main()
