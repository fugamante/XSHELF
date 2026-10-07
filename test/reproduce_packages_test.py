#!/usr/bin/env python3
"""Focused safety tests for the canonical native reproduction harness."""

from __future__ import annotations

import importlib.util
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "reproduce_packages", ROOT / "scripts/reproduce_packages.py"
)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("unable to load reproduction harness")
reproduce = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = reproduce
SPEC.loader.exec_module(reproduce)


class CanonicalRootTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.prefix = Path(self.temp.name) / "approved"
        self.prefix.mkdir()
        self.prefix.chmod(0o700)
        self.root = self.prefix / "canonical"

    def tearDown(self) -> None:
        self.temp.cleanup()

    def own_root(self) -> None:
        root, prefix = reproduce.validate_root(self.root, self.prefix)
        self.root = root
        self.prefix = prefix
        reproduce.create_root(root)

    def test_unowned_root_is_refused(self) -> None:
        self.root.mkdir(parents=True)
        with self.assertRaisesRegex(reproduce.ReproductionError, "unsafe owner or permissions"):
            reproduce.validate_root(self.root, self.prefix)

    def test_forged_marker_in_writable_root_is_refused(self) -> None:
        self.root.mkdir(mode=0o777)
        self.root.chmod(0o777)
        (self.root / reproduce.MARKER).write_text(
            reproduce._marker_text(self.root.resolve(), "0" * 64), encoding="utf-8"
        )
        with self.assertRaisesRegex(reproduce.ReproductionError, "unsafe owner or permissions"):
            reproduce.validate_root(self.root, self.prefix)

    def test_wrong_owner_and_writable_marker_are_refused(self) -> None:
        self.own_root()
        marker = self.root / reproduce.MARKER
        marker.chmod(0o666)
        with self.assertRaisesRegex(reproduce.ReproductionError, "invalid marker"):
            reproduce.validate_root(self.root, self.prefix)
        marker.chmod(0o600)
        original = reproduce.os.fstat

        def other_owner(fd: int):
            info = original(fd)
            if info.st_ino != marker.lstat().st_ino:
                return info
            values = list(info)
            values[4] = os.geteuid() + 1
            return os.stat_result(values)

        with mock.patch.object(reproduce.os, "fstat", side_effect=other_owner):
            with self.assertRaisesRegex(reproduce.ReproductionError, "invalid marker"):
                reproduce.validate_root(self.root, self.prefix)

    def test_wrong_root_owner_is_refused(self) -> None:
        self.own_root()
        original = Path.lstat

        def other_owner(path: Path):
            info = original(path)
            if path == self.root:
                values = list(info)
                values[4] = os.geteuid() + 1
                return os.stat_result(values)
            return info

        with mock.patch.object(Path, "lstat", other_owner):
            with self.assertRaisesRegex(reproduce.ReproductionError, "unsafe owner or permissions"):
                reproduce.assert_owned_root(self.root)

    def test_shared_writable_prefix_is_refused(self) -> None:
        self.prefix.chmod(0o777)
        with self.assertRaisesRegex(reproduce.ReproductionError, "unsafe owner or permissions"):
            reproduce.validate_root(self.root, self.prefix)

    def test_foreign_owned_ancestor_is_refused(self) -> None:
        ancestor = Path(self.temp.name).resolve()
        original = Path.lstat

        def other_owner(path: Path):
            info = original(path)
            if path == ancestor:
                values = list(info)
                values[4] = os.geteuid() + 1
                return os.stat_result(values)
            return info

        with mock.patch.object(Path, "lstat", other_owner):
            with self.assertRaisesRegex(reproduce.ReproductionError, "unsafe ancestor"):
                reproduce.validate_root(self.root, self.prefix)

    def test_private_child_under_runner_temp_is_accepted(self) -> None:
        runner = Path(self.temp.name) / "runner"
        runner.mkdir(mode=0o755)
        runner.chmod(0o755)
        private = runner / "native"
        private.mkdir(mode=0o700)
        private.chmod(0o700)
        root, prefix = reproduce.validate_root(private / "canonical", private)
        self.assertEqual(root.parent, prefix)

    @unittest.skipUnless(sys.platform == "darwin", "macOS ACL syntax")
    def test_extended_acl_root_is_refused(self) -> None:
        self.own_root()
        user = subprocess.check_output(["id", "-un"], text=True).strip()
        subprocess.run(["chmod", "+a", f"user:{user} allow read", str(self.root)], check=True)
        try:
            with self.assertRaisesRegex(reproduce.ReproductionError, "ACL"):
                reproduce.assert_owned_root(self.root)
        finally:
            subprocess.run(["chmod", "-N", str(self.root)], check=True)

    @unittest.skipUnless(sys.platform == "darwin", "macOS ACL syntax")
    def test_deny_only_acl_ancestor_is_accepted(self) -> None:
        ancestor = Path(self.temp.name)
        subprocess.run(["chmod", "+a", "group:everyone deny delete", str(ancestor)], check=True)
        try:
            root, prefix = reproduce.validate_root(self.root, self.prefix)
            self.assertEqual(root.parent, prefix)
        finally:
            subprocess.run(["chmod", "-N", str(ancestor)], check=True)

    def test_valid_private_root_can_be_reused(self) -> None:
        self.own_root()
        self.assertEqual(reproduce.validate_root(self.root, self.prefix), (self.root, self.prefix))

    def test_private_forged_marker_without_receipt_is_refused(self) -> None:
        self.root.mkdir(mode=0o700)
        self.root.chmod(0o700)
        (self.root / reproduce.MARKER).write_text(
            reproduce._marker_text(self.root.resolve(), "0" * 64), encoding="utf-8"
        )
        with self.assertRaisesRegex(reproduce.ReproductionError, "receipt"):
            reproduce.validate_root(self.root, self.prefix)

    def test_unsafe_path_is_refused(self) -> None:
        unsafe = self.prefix / "nested" / "canonical"
        with self.assertRaisesRegex(reproduce.ReproductionError, "direct child"):
            reproduce.validate_root(unsafe, self.prefix)

    def test_filesystem_root_prefix_is_refused(self) -> None:
        with self.assertRaisesRegex(reproduce.ReproductionError, "filesystem root"):
            reproduce.validate_root(Path("/xshelf-canonical-test"), Path("/"))

    def test_output_outside_prefix_is_refused(self) -> None:
        outside = Path(self.temp.name) / "outside"
        with self.assertRaisesRegex(reproduce.ReproductionError, "approved prefix"):
            reproduce.validate_output(outside, self.prefix, self.root)

    def test_output_under_unsafe_parent_is_refused(self) -> None:
        self.root, self.prefix = reproduce.validate_root(self.root, self.prefix)
        parent = self.prefix / "shared"
        parent.mkdir()
        parent.chmod(0o777)
        with self.assertRaisesRegex(reproduce.ReproductionError, "unsafe owner or permissions"):
            reproduce.validate_output(parent / "evidence", self.prefix, self.root)

    def test_output_is_private_with_permissive_umask(self) -> None:
        self.root, self.prefix = reproduce.validate_root(self.root, self.prefix)
        output = reproduce.validate_output(self.prefix / "nested" / "evidence", self.prefix, self.root)
        old_umask = os.umask(0)
        try:
            reproduce.create_private_output(output, self.prefix)
        finally:
            os.umask(old_umask)
        self.assertEqual(output.stat().st_mode & 0o777, 0o700)
        self.assertEqual(output.parent.stat().st_mode & 0o777, 0o700)

    def test_symlink_marker_is_refused(self) -> None:
        self.own_root()
        (self.root / reproduce.MARKER).unlink()
        marker_target = Path(self.temp.name) / "marker"
        marker_target.write_text(
            reproduce._marker_text(self.root, reproduce._read_receipt(self.root)), encoding="utf-8"
        )
        (self.root / reproduce.MARKER).symlink_to(marker_target)
        with self.assertRaisesRegex(reproduce.ReproductionError, "unowned"):
            reproduce.validate_root(self.root, self.prefix)

    def test_concurrent_lock_is_refused(self) -> None:
        with reproduce.root_lock(self.prefix, self.root):
            with self.assertRaisesRegex(reproduce.ReproductionError, "locked"):
                with reproduce.root_lock(self.prefix, self.root):
                    pass

    def test_symlink_and_writable_lock_are_refused(self) -> None:
        lock = self.prefix / f"{self.root.name}{reproduce.LOCK}"
        target = self.prefix / "other"
        target.write_text("other", encoding="utf-8")
        lock.symlink_to(target)
        with self.assertRaises(reproduce.ReproductionError):
            with reproduce.root_lock(self.prefix, self.root):
                pass
        lock.unlink()
        lock.write_text("", encoding="utf-8")
        lock.chmod(0o666)
        with self.assertRaisesRegex(reproduce.ReproductionError, "unsafe lock"):
            with reproduce.root_lock(self.prefix, self.root):
                pass

    def test_replaced_lock_is_not_removed(self) -> None:
        lock = self.prefix / f"{self.root.name}{reproduce.LOCK}"
        with reproduce.root_lock(self.prefix, self.root):
            lock.rename(self.prefix / "old-lock")
            lock.write_text("replacement", encoding="utf-8")
        self.assertEqual(lock.read_text(encoding="utf-8"), "replacement")

    def test_full_state_cleanup(self) -> None:
        self.own_root()
        (self.root / "target/deps").mkdir(parents=True)
        (self.root / "target/deps/object.o").write_bytes(b"compiled")
        (self.root / "cargo").mkdir()
        (self.root / "cargo/cache").write_text("cached\n", encoding="utf-8")
        reproduce.clear_root(self.root)
        self.assertEqual([path.name for path in self.root.iterdir()], [reproduce.MARKER])

    def test_cleanup_unlinks_symlink_without_touching_target(self) -> None:
        self.own_root()
        target = Path(self.temp.name) / "target.txt"
        target.write_text("keep", encoding="utf-8")
        (self.root / "link").symlink_to(target)
        reproduce.clear_root(self.root)
        self.assertEqual(target.read_text(encoding="utf-8"), "keep")

    def test_root_replacement_does_not_clear_replacement(self) -> None:
        self.own_root()
        (self.root / "old.txt").write_text("old", encoding="utf-8")
        old = self.prefix / "old-root"
        original = reproduce.os.scandir
        changed = False

        def replace(path):
            nonlocal changed
            if isinstance(path, int) and not changed:
                changed = True
                self.root.rename(old)
                self.root.mkdir(mode=0o700)
                self.root.chmod(0o700)
                (self.root / reproduce.MARKER).write_text(
                    reproduce._marker_text(self.root, reproduce._read_receipt(self.root)),
                    encoding="utf-8",
                )
                (self.root / "replacement.txt").write_text("keep", encoding="utf-8")
            return original(path)

        with mock.patch.object(reproduce.os, "scandir", side_effect=replace):
            with self.assertRaisesRegex(reproduce.ReproductionError, "root changed"):
                reproduce.clear_root(self.root)
        self.assertEqual((self.root / "replacement.txt").read_text(encoding="utf-8"), "keep")

    def test_mounted_state_is_refused(self) -> None:
        self.own_root()
        mounted = self.root / "target"
        mounted.mkdir()
        original = Path.is_mount

        def is_mount(path: Path) -> bool:
            return path == mounted or original(path)

        with mock.patch.object(Path, "is_mount", is_mount):
            with self.assertRaisesRegex(reproduce.ReproductionError, "mounted"):
                reproduce.clear_root(self.root)

    def test_nested_mount_is_refused(self) -> None:
        self.own_root()
        mounted = self.root / "target" / "nested"
        mounted.mkdir(parents=True)
        original = Path.is_mount

        with mock.patch.object(Path, "is_mount", lambda path: path == mounted or original(path)):
            with self.assertRaisesRegex(reproduce.ReproductionError, "mounted"):
                reproduce.clear_root(self.root)

    def test_native_intel_missing_translation_oid_is_zero(self) -> None:
        missing = subprocess.CompletedProcess(
            ["sysctl"], 1, stdout="", stderr="sysctl: unknown oid 'sysctl.proc_translated'\n"
        )
        with (
            mock.patch.object(reproduce.platform, "system", return_value="Darwin"),
            mock.patch.object(reproduce.platform, "machine", return_value="x86_64"),
            mock.patch.object(reproduce.platform, "mac_ver", return_value=("fixture", (), "")),
            mock.patch.object(reproduce.subprocess, "run", return_value=missing),
            mock.patch.object(reproduce, "run", return_value="fixture"),
        ):
            self.assertEqual(reproduce.host_identity()["translated"], "0")

    def test_dirty_source_is_refused(self) -> None:
        source = Path(self.temp.name) / "source"
        source.mkdir()
        subprocess.run(["git", "init", "-q"], cwd=source, check=True)
        (source / "tracked").write_text("clean\n", encoding="utf-8")
        subprocess.run(["git", "add", "tracked"], cwd=source, check=True)
        subprocess.run(
            [
                "git",
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=package@example.com",
                "commit",
                "-qm",
                "fixture",
            ],
            cwd=source,
            check=True,
        )
        revision = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=source, text=True
        ).strip()
        (source / "tracked").write_text("dirty\n", encoding="utf-8")
        with self.assertRaisesRegex(reproduce.ReproductionError, "dirty"):
            reproduce.assert_clean_source(source, revision)

    def test_checksum_mismatch_is_refused(self) -> None:
        first = Path(self.temp.name) / "first"
        second = Path(self.temp.name) / "second"
        first.write_bytes(b"one")
        second.write_bytes(b"two")
        with self.assertRaisesRegex(reproduce.ReproductionError, "checksum mismatch"):
            reproduce.require_equal(first, second, "archive")

    def test_adhoc_codesign_state_requires_cdhash(self) -> None:
        observed = reproduce.parse_codesign(
            0,
            "Executable=/tmp/xshelf\nSignature=adhoc\n"
            "CDHash=0123456789abcdef0123456789abcdef01234567\n",
        )
        self.assertEqual(
            observed,
            {
                "signature_state": "adhoc",
                "cdhash": "0123456789abcdef0123456789abcdef01234567",
            },
        )

    def test_unsigned_codesign_state_has_null_cdhash(self) -> None:
        observed = reproduce.parse_codesign(
            1, "/tmp/xshelf: code object is not signed at all\n"
        )
        self.assertEqual(observed, {"signature_state": "unsigned", "cdhash": None})

    def test_signing_identity_mismatch_is_refused(self) -> None:
        first = {"signature_state": "adhoc", "cdhash": "a" * 40}
        second = {"signature_state": "unsigned", "cdhash": None}
        with self.assertRaisesRegex(reproduce.ReproductionError, "identity mismatch"):
            reproduce.require_same_build_identity(first, second)

    def test_unexpected_codesign_output_is_refused(self) -> None:
        cases = (
            (0, "Authority=Developer ID Application: Example\nCDHash=" + "a" * 40),
            (0, "Signature=adhoc\n"),
            (1, "/tmp/xshelf: invalid signature\n"),
            (1, "/tmp/xshelf: code object is not signed at all\nCDHash=" + "a" * 40),
        )
        for returncode, details in cases:
            with self.subTest(returncode=returncode, details=details):
                with self.assertRaisesRegex(reproduce.ReproductionError, "unexpected|invalid"):
                    reproduce.parse_codesign(returncode, details)

    def test_build_command_has_no_dirty_escape(self) -> None:
        text = (ROOT / "scripts/reproduce_packages.py").read_text(encoding="utf-8")
        self.assertNotIn("--allow-dirty", text)
        self.assertNotIn("-no_uuid", text)
        self.assertNotIn("-no_adhoc_codesign", text)


if __name__ == "__main__":
    unittest.main()
