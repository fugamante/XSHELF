#!/usr/bin/env python3
"""Focused tests for XSHELF Developer ID signing and notarization controls."""

from __future__ import annotations

import argparse
import importlib.util
import io
import json
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


package_release = load_module("package_release_signing", ROOT / "scripts/package_release.py")
sign_packages = load_module("sign_packages", ROOT / "scripts/sign_packages.py")


class PackageSigningTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.base = Path(self.temp.name)
        self.repo = self.base / "repo"
        (self.repo / ".cx/schemas").mkdir(parents=True)
        (self.repo / "docs/man").mkdir(parents=True)
        (self.repo / "rust/cxrs/src").mkdir(parents=True)
        (self.repo / "rust/cxrs/tests/fixtures").mkdir(parents=True)
        (self.repo / "scripts").mkdir(parents=True)
        fixtures = {
            "LICENSE": "MIT fixture\n",
            "README.md": "# fixture\n",
            "VERSION": "2026.08.29\n",
            "docs/man/cx.1": ".TH XSHELF 1\n",
            "rust/cxrs/Cargo.toml": "[package]\nname='cxrs'\n",
            "rust/cxrs/Cargo.lock": "# fixture\n",
            "rust/cxrs/rust-toolchain.toml": "[toolchain]\nchannel='1.95.0'\n",
            "rust/cxrs/src/main.rs": "fn main() {}\n",
            "rust/cxrs/tests/fixtures/eval_lab_bundle.json": "{}\n",
            "scripts/build_packages.sh": "#!/bin/sh\n",
            "scripts/package_release.py": "# fixture\n",
        }
        for name, content in fixtures.items():
            path = self.repo / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")
        for name in package_release.SCHEMA_NAMES:
            (self.repo / ".cx/schemas" / name).write_text("{}\n", encoding="utf-8")
        self.binary = self.base / "xshelf"
        self.binary.write_bytes(b"fixture executable")
        self.binary.chmod(0o755)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def build(self, target: str, output: Path) -> Path:
        archive, _ = package_release.build_archive(
            repo_root=self.repo,
            binary=self.binary,
            output_dir=output,
            version="2026.08.29",
            target=target,
            source_revision="a" * 40,
            source_dirty=False,
            source_fingerprint="b" * 64,
            cargo_toolchain="cargo 1.95.0 (fixture)",
            rust_toolchain="rustc 1.95.0 (fixture)",
            macos_min_version="11.0",
            verify_architecture=False,
        )
        return archive

    def test_unsigned_package_is_loaded_and_bound(self) -> None:
        archive = self.build("aarch64-apple-darwin", self.base / "packages")
        package = sign_packages.load_package(archive)
        self.assertEqual(package.provenance["architecture"], "aarch64-apple-darwin")
        self.assertFalse(package.provenance["signed"])
        self.assertFalse(package.provenance["notarized"])

    def test_checksum_and_manifest_tampering_fail_closed(self) -> None:
        archive = self.build("aarch64-apple-darwin", self.base / "tamper")
        archive.with_name(archive.name + ".sha256").write_text(
            f"{'0' * 64}  {archive.name}\n", encoding="utf-8"
        )
        with self.assertRaisesRegex(sign_packages.SignError, "checksum does not match"):
            sign_packages.load_package(archive)

    def test_unsafe_archive_member_fails_closed(self) -> None:
        archive = self.build("aarch64-apple-darwin", self.base / "unsafe")
        rewritten = archive.with_name("rewritten.tar.gz")
        with tarfile.open(archive, "r:gz") as incoming, tarfile.open(rewritten, "w:gz") as outgoing:
            for member in incoming.getmembers():
                stream = incoming.extractfile(member) if member.isreg() else None
                outgoing.addfile(member, stream)
            info = tarfile.TarInfo("../escape")
            info.size = 1
            outgoing.addfile(info, io.BytesIO(b"x"))
        rewritten.replace(archive)
        archive.with_name(archive.name + ".sha256").write_text(
            f"{sign_packages.sha256_file(archive)}  {archive.name}\n", encoding="utf-8"
        )
        with self.assertRaisesRegex(sign_packages.SignError, "escapes package root"):
            sign_packages.load_package(archive)

    def test_notary_log_requires_matching_ticket(self) -> None:
        job = "12345678-1234-1234-1234-123456789abc"
        cdhash = "a" * 40
        value = {
            "jobId": job,
            "status": "Accepted",
            "ticketContents": [
                {"path": "payload/xshelf", "arch": "arm64", "cdhash": cdhash}
            ],
        }
        self.assertEqual(
            sign_packages.validate_notary_log(value, "aarch64-apple-darwin", cdhash), job
        )
        value["ticketContents"][0]["cdhash"] = "b" * 40
        with self.assertRaisesRegex(sign_packages.SignError, "does not cover"):
            sign_packages.validate_notary_log(value, "aarch64-apple-darwin", cdhash)

    def test_submit_records_id_before_wait_and_validates_log(self) -> None:
        job = "12345678-1234-1234-1234-123456789abc"
        signature = sign_packages.Signature("a" * 40, "io.example.xshelf", "private")
        binary = self.base / "payload" / "xshelf"
        binary.parent.mkdir()
        binary.write_bytes(b"signed")
        work = self.base / "notary"
        work.mkdir()

        def fake_run(command, *, capture=True, allow_failure=False):
            if "submit" in command:
                return subprocess.CompletedProcess(command, 0, json.dumps({"id": job}), "")
            if "wait" in command:
                receipt = json.loads(
                    (work / "aarch64-apple-darwin.submission.json").read_text(encoding="utf-8")
                )
                self.assertEqual(receipt["submission_id"], job)
                return subprocess.CompletedProcess(
                    command, 0, json.dumps({"id": job, "status": "Accepted"}), ""
                )
            if "log" in command:
                Path(command[-1]).write_text(
                    json.dumps(
                        {
                            "jobId": job,
                            "issues": None,
                            "status": "Accepted",
                            "ticketContents": [
                                {
                                    "arch": "arm64",
                                    "cdhash": signature.cdhash,
                                    "path": "payload/xshelf",
                                }
                            ],
                        }
                    ),
                    encoding="utf-8",
                )
            return subprocess.CompletedProcess(command, 0, "", "")

        with mock.patch.object(sign_packages, "run", side_effect=fake_run):
            observed = sign_packages.submit(
                binary,
                "profile",
                "aarch64-apple-darwin",
                signature,
                work,
                "30m",
            )
        self.assertEqual(observed, job)

    def test_final_archive_records_sanitized_accepted_evidence(self) -> None:
        archive = self.build("aarch64-apple-darwin", self.base / "unsigned")
        package = sign_packages.load_package(archive)
        signature = sign_packages.Signature("c" * 40, "io.example.xshelf", "private-team")
        job = "12345678-1234-1234-1234-123456789abc"
        final, sidecar, evidence = sign_packages.finalize(
            package, b"signed executable", signature, job, self.base / "signed"
        )
        self.assertTrue(final.is_file())
        self.assertTrue(sidecar.is_file())
        self.assertTrue(evidence.is_file())
        public = json.loads(evidence.read_text(encoding="utf-8"))
        self.assertEqual(public["notarization_status"], "Accepted")
        self.assertNotIn("team", public)
        with tarfile.open(final, "r:gz") as bundle:
            root = final.name[: -len(".tar.gz")]
            manifest = json.load(bundle.extractfile(f"{root}/manifest.json"))
            provenance = json.load(bundle.extractfile(f"{root}/provenance.json"))
        binary_row = next(row for row in manifest["files"] if row["path"] == "bin/xshelf")
        self.assertEqual(binary_row["sha256"], sign_packages.sha256_bytes(b"signed executable"))
        self.assertTrue(provenance["signed"])
        self.assertTrue(provenance["notarized"])
        self.assertEqual(provenance["notarization"]["submission_id"], job)
        self.assertNotIn("team", provenance["signing"])

    def test_preflight_requires_both_targets_and_explicit_identifier(self) -> None:
        arm = sign_packages.load_package(
            self.build("aarch64-apple-darwin", self.base / "arm")
        )
        intel = sign_packages.load_package(
            self.build("x86_64-apple-darwin", self.base / "intel")
        )
        with mock.patch.object(sign_packages, "_tool_ready", return_value=True):
            sign_packages._preflight([arm, intel], "io.example.xshelf")
            with self.assertRaisesRegex(sign_packages.SignError, "reverse-DNS"):
                sign_packages._preflight([arm, intel], "xshelf")
            with self.assertRaisesRegex(sign_packages.SignError, "exactly one"):
                sign_packages._preflight([arm], "io.example.xshelf")

    def test_run_requires_profile_team_confirmation_before_credentials(self) -> None:
        arm = self.build("aarch64-apple-darwin", self.base / "arm-confirm")
        intel = self.build("x86_64-apple-darwin", self.base / "intel-confirm")
        args = argparse.Namespace(
            command="run",
            archives=[arm, intel],
            identifier="io.example.xshelf",
            identity="certificate-hash",
            keychain_profile="profile",
            confirm_profile_team=False,
            output_dir=self.base / "output",
        )
        with mock.patch.object(sign_packages, "_tool_ready", return_value=True):
            with self.assertRaisesRegex(sign_packages.SignError, "confirm-profile-team"):
                sign_packages.execute(args)

    def test_codesign_evidence_requires_runtime_and_timestamp(self) -> None:
        details = "\n".join(
            [
                "Executable=/tmp/xshelf",
                "Identifier=io.example.xshelf",
                "CodeDirectory v=20500 size=1 flags=0x10000(runtime)",
                "Authority=Developer ID Application: Private",
                "TeamIdentifier=PRIVATE",
                "CDHash=" + "d" * 40,
                "Timestamp=Aug 29, 2026 at 1:00:00 PM",
            ]
        )
        responses = [
            subprocess.CompletedProcess([], 0, "", ""),
            subprocess.CompletedProcess([], 0, "", details),
        ]
        with mock.patch.object(sign_packages, "run", side_effect=responses):
            signature = sign_packages._codesign_details(Path("/tmp/xshelf"))
        self.assertEqual(signature.identifier, "io.example.xshelf")
        self.assertEqual(signature.cdhash, "d" * 40)


if __name__ == "__main__":
    unittest.main()
