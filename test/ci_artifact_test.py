#!/usr/bin/env python3
"""Exercise the CI failure steps with checkout-controlled symlinks."""

import os
import pathlib
import re
import stat
import subprocess
import tempfile
import textwrap
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
WORKFLOW = (ROOT / ".github/workflows/cxrs-compat.yml").read_text()
STAGER = ROOT / "scripts/ci_artifacts.py"
OS_NAME = "ubuntu-latest"


def step_script(name: str) -> str:
    marker = f"      - name: {name}\n"
    if marker not in WORKFLOW:
        raise AssertionError(f"missing workflow step: {name}")
    step = WORKFLOW.split(marker, 1)[1].split("\n      - name:", 1)[0]
    if "        run: |\n" not in step:
        raise AssertionError(f"missing shell body: {name}")
    body = textwrap.dedent(step.split("        run: |\n", 1)[1])
    return body.replace("${{ matrix.os }}", OS_NAME)


def run_step(name: str, env: dict[str, str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["bash", "-c", step_script(name)],
        env=env,
        text=True,
        capture_output=True,
        check=False,
    )


class CiArtifactTests(unittest.TestCase):
    def fixture(self, root: pathlib.Path) -> tuple[pathlib.Path, pathlib.Path, dict[str, str]]:
        workspace = root / "checkout"
        runner = root / "runner-temp"
        workspace.mkdir()
        runner.mkdir()
        match = re.search(r"(?m)^      CXRS_ARTIFACT_DIR: ([\w-]+)$", WORKFLOW)
        self.assertIsNotNone(match)
        env = os.environ.copy()
        env.update(
            GITHUB_WORKSPACE=str(workspace),
            RUNNER_TEMP=str(runner),
            CXRS_ARTIFACT_DIR=match.group(1),
            GITHUB_REPOSITORY="synthetic/XSHELF",
            GITHUB_RUN_ID="1",
            GITHUB_RUN_ATTEMPT="1",
            GITHUB_SHA="a" * 40,
        )
        prepared = run_step("Prepare failure artifacts", env)
        self.assertEqual(prepared.returncode, 0, prepared.stderr)
        artifacts = runner / env["CXRS_ARTIFACT_DIR"]
        self.assertTrue(artifacts.is_dir())
        self.assertEqual(stat.S_IMODE(artifacts.stat().st_mode), 0o700)
        return workspace, artifacts, env

    def stage(self, artifacts: pathlib.Path, env: dict[str, str]) -> pathlib.Path:
        upload = pathlib.Path(env["RUNNER_TEMP"]) / "cxrs-compat-upload"
        result = subprocess.run(
            ["python3", str(STAGER), str(artifacts), str(upload), OS_NAME],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return upload

    def test_checkout_symlinks_cannot_seed_failure_artifact(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            workspace, artifacts, env = self.fixture(root)
            outside = root / "outside.txt"
            outside.write_text("SYNTHETIC_OUTSIDE_MARKER\n")
            planted = workspace / ".github/ci-artifacts"
            planted.mkdir(parents=True)
            for name in (
                "pr-controlled.log",
                f"rust_check_{OS_NAME}.log",
                f"summary_{OS_NAME}.txt",
                f"reliability_{OS_NAME}.txt",
            ):
                (planted / name).symlink_to(outside)

            summary = run_step("Build failure summary", env)
            self.assertEqual(summary.returncode, 0, summary.stderr)
            generated = (artifacts / f"summary_{OS_NAME}.txt").read_text()
            self.assertIn("captured logs:\n(none)", generated)
            self.assertNotIn("SYNTHETIC_OUTSIDE_MARKER", generated)
            report = run_step("Extract reliability failure report", env)
            self.assertEqual(report.returncode, 0, report.stderr)
            self.assertIn(
                "rust check log unavailable",
                (artifacts / f"reliability_{OS_NAME}.txt").read_text(),
            )
            upload = self.stage(artifacts, env)
            self.assertEqual(
                {item.name for item in upload.iterdir()},
                {f"summary_{OS_NAME}.txt", f"reliability_{OS_NAME}.txt"},
            )
            self.assertEqual(outside.read_text(), "SYNTHETIC_OUTSIDE_MARKER\n")
            self.assertIn(
                "path: ${{ runner.temp }}/cxrs-compat-upload", WORKFLOW
            )

    def test_generated_logs_preserved_and_unlisted_entries_skipped(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            workspace, artifacts, env = self.fixture(root)
            outside = root / "outside.txt"
            outside.write_text("SYNTHETIC_OUTSIDE_MARKER\n")
            (artifacts / f"rust_check_{OS_NAME}.log").write_text(
                "Running tests/reliability_integration.rs\n"
                "test owned_failure ... FAILED\n"
                "test result: FAILED. 0 passed; 1 failed\n"
            )
            (artifacts / f"compat_check_{OS_NAME}.log").symlink_to(outside)
            (artifacts / "unlisted.log").write_text("UNLISTED_MARKER\n")
            script = workspace / "rust/cxrs/scripts/reliability_report.py"
            script.parent.mkdir(parents=True)
            script.write_bytes((ROOT / "rust/cxrs/scripts/reliability_report.py").read_bytes())

            summary = run_step("Build failure summary", env)
            self.assertEqual(summary.returncode, 0, summary.stderr)
            generated = (artifacts / f"summary_{OS_NAME}.txt").read_text()
            self.assertIn("test owned_failure ... FAILED", generated)
            self.assertNotIn("SYNTHETIC_OUTSIDE_MARKER", generated)
            self.assertNotIn("UNLISTED_MARKER", generated)
            report = run_step("Extract reliability failure report", env)
            self.assertEqual(report.returncode, 0, report.stderr)
            self.assertIn(
                "test owned_failure ... FAILED",
                (artifacts / f"reliability_{OS_NAME}.txt").read_text(),
            )
            upload = self.stage(artifacts, env)
            self.assertEqual(
                {item.name for item in upload.iterdir()},
                {
                    f"rust_check_{OS_NAME}.log",
                    f"summary_{OS_NAME}.txt",
                    f"reliability_{OS_NAME}.txt",
                },
            )
            self.assertTrue(all(item.is_file() and not item.is_symlink() for item in upload.iterdir()))
            self.assertEqual(outside.read_text(), "SYNTHETIC_OUTSIDE_MARKER\n")

    def test_preexisting_output_symlinks_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            _, artifacts, env = self.fixture(root)
            outside = root / "outside.txt"
            outside.write_text("SYNTHETIC_OUTSIDE_MARKER\n")
            summary = artifacts / f"summary_{OS_NAME}.txt"
            summary.symlink_to(outside)
            result = run_step("Build failure summary", env)
            self.assertNotEqual(result.returncode, 0)
            summary.unlink()
            report = artifacts / f"reliability_{OS_NAME}.txt"
            report.symlink_to(outside)
            result = run_step("Extract reliability failure report", env)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(outside.read_text(), "SYNTHETIC_OUTSIDE_MARKER\n")

    def test_preexisting_upload_path_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            _, artifacts, env = self.fixture(root)
            (artifacts / f"summary_{OS_NAME}.txt").write_text("owned summary\n")
            outside = root / "outside"
            outside.mkdir()
            marker = outside / "marker.txt"
            marker.write_text("SYNTHETIC_OUTSIDE_MARKER\n")
            upload = pathlib.Path(env["RUNNER_TEMP"]) / "cxrs-compat-upload"
            upload.symlink_to(outside, target_is_directory=True)
            result = subprocess.run(
                ["python3", str(STAGER), str(artifacts), str(upload), OS_NAME],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(marker.read_text(), "SYNTHETIC_OUTSIDE_MARKER\n")
            self.assertEqual({item.name for item in outside.iterdir()}, {"marker.txt"})
            upload.unlink()
            upload.mkdir()
            result = subprocess.run(
                ["python3", str(STAGER), str(artifacts), str(upload), OS_NAME],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(list(upload.iterdir()), [])
            self.assertIn(
                "if: failure() && steps.artifact_stage.outcome == 'success'",
                WORKFLOW,
            )
            self.assertEqual(
                WORKFLOW.count("if: failure() && steps.artifact_setup.outcome == 'success'"),
                3,
            )


if __name__ == "__main__":
    unittest.main()
