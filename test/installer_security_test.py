#!/usr/bin/env python3
"""Exercise generated shell operands only in private, minimal install fixtures."""
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class InstallerSecurity(unittest.TestCase):
    def fixture(self, base, name):
        repo = base / name
        (repo / "bin").mkdir(parents=True)
        (repo / "scripts").mkdir()
        for script in ("bin/cx-install", "bin/cx-uninstall", "scripts/xshelf_suite_install.sh"):
            shutil.copy2(ROOT / script, repo / script)
        # No docs/man is copied: tests cannot enter the global manpage installer.
        (repo / "cx.sh").write_text('printf "%s\\n" "$BASH_SOURCE" > "$SOURCE_RESULT"\n')
        home = base / "home"
        home.mkdir(exist_ok=True)
        env = {
            "HOME": str(home),
            "PATH": "/usr/bin:/bin",
            "SOURCE_RESULT": str(base / "sourced"),
            "RUNTIME_RESULT": str(base / "runtime-result"),
        }
        return repo, env

    def run_shell(self, args, base, env):
        result = subprocess.run(
            ["/bin/bash", *map(str, args)], cwd=base, env=env,
            check=False, capture_output=True, text=True, timeout=15,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return result

    def check_source(self, profile, repo, base, env):
        self.run_shell(["-n", profile], base, env)
        self.run_shell(["-c", 'source "$1"', "fixture", profile], base, env)
        self.assertEqual((base / "sourced").read_text(), str(repo / "cx.sh") + "\n")
        self.assertFalse((base / "SENTINEL").exists())
        self.assertFalse((base / "SECOND").exists())

    def test_profile_literals(self):
        names = ["plain", "substitution $(touch SENTINEL) `touch SECOND` $HOME", "literal ' $HOME $(touch SENTINEL) `touch SECOND` \"\npart", "trailing\n"]
        for name in names:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                base = Path(directory)
                repo, env = self.fixture(base, name)
                profile = base / "profile"
                env["CX_SHELL_RC"] = str(profile)
                original = 'export RETAINED=before\n# unrelated trailing content'
                profile.write_text(original)
                self.run_shell([repo / "bin/cx-install"], base, env)
                installed = profile.read_bytes()
                self.check_source(profile, repo, base, env)
                self.run_shell([repo / "bin/cx-install"], base, env)
                self.assertEqual(profile.read_bytes(), installed, "reinstall must be byte-idempotent")
                self.run_shell([repo / "bin/cx-uninstall"], base, env)
                self.assertEqual(profile.read_text(), original + "\n# XSHELF/CX utilities\n")

    def test_legacy_upgrade(self):
        names = ["plain", "legacy ' $HOME $(touch SENTINEL) `touch SECOND` \"\npart", "trailing\n"]
        for name in names:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                base = Path(directory)
                repo, env = self.fixture(base, name)
                profile = base / "profile"
                env["CX_SHELL_RC"] = str(profile)
                before = "export RETAINED=before\n"
                after = "# unrelated final content"
                legacy = 'source "' + str(repo).rstrip("\n") + '/cx.sh"'
                literal = "'" + str(repo / "cx.sh").replace("'", "'\\''") + "'"
                safe = "source " + literal
                profile.write_text(before + legacy + "\n" + after)
                self.run_shell([repo / "bin/cx-install"], base, env)
                self.assertEqual(profile.read_text(), before + safe + "\n" + after)
                self.check_source(profile, repo, base, env)
                upgraded = profile.read_bytes()
                self.run_shell([repo / "bin/cx-install"], base, env)
                self.assertEqual(profile.read_bytes(), upgraded)
                self.run_shell([repo / "bin/cx-uninstall"], base, env)
                self.assertEqual(profile.read_text(), before + after)
                # Remove every exact old registration without executing poisoned content.
                profile.write_text(before + legacy + "\n" + legacy + "\n" + after)
                self.run_shell([repo / "bin/cx-uninstall"], base, env)
                self.assertEqual(profile.read_text(), before + after)
                if "\n" not in name:
                    unrelated = "# " + legacy + " is an example\n"
                    profile.write_text(unrelated + safe + "\n" + after)
                    self.run_shell([repo / "bin/cx-uninstall"], base, env)
                    self.assertEqual(profile.read_text(), unrelated + after)

    def test_profile_symlink(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            repo, env = self.fixture(base, "plain")
            target = base / "managed-profile"
            original = "export RETAINED=before\n# unrelated final content"
            target.write_text(original)
            target.chmod(0o640)
            profile = base / "profile"
            profile.symlink_to(target)
            env["CX_SHELL_RC"] = str(profile)
            target_inode = target.stat().st_ino
            link_inode = profile.lstat().st_ino
            installed = None
            for script in ("cx-install", "cx-install", "cx-uninstall"):
                self.run_shell([repo / "bin" / script], base, env)
                self.assertTrue(profile.is_symlink(), "selected profile link must survive")
                self.assertEqual(profile.readlink(), target)
                self.assertEqual(profile.lstat().st_ino, link_inode)
                self.assertEqual(target.stat().st_ino, target_inode)
                self.assertEqual(target.stat().st_mode & 0o777, 0o640)
                if script == "cx-install":
                    self.check_source(profile, repo, base, env)
                    if installed is None:
                        installed = target.read_bytes()
                    else:
                        self.assertEqual(target.read_bytes(), installed)
                else:
                    self.assertEqual(target.read_text(), original + "\n# XSHELF/CX utilities\n")

    def test_profile_choices(self):
        for choice in ("default", "profile", "bash", "override"):
            with self.subTest(choice=choice), tempfile.TemporaryDirectory() as directory:
                base = Path(directory)
                repo, env = self.fixture(base, "plain")
                home = Path(env["HOME"])
                if choice in ("profile", "bash"):
                    (home / ".profile").write_text("# profile\n")
                if choice == "bash":
                    (home / ".bash_profile").write_text("# bash\n")
                selected = home / (".profile" if choice == "profile" else ".bash_profile")
                if choice == "override":
                    selected = base / "custom"
                    env["CX_SHELL_RC"] = str(selected)
                self.run_shell([repo / "bin/cx-install"], base, env)
                self.assertIn("source ", selected.read_text())
                self.run_shell([repo / "bin/cx-uninstall"], base, env)
                self.assertNotIn("source ", selected.read_text())

    def test_launcher_literals(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            repo, env = self.fixture(base, "runtime")
            mock = '#!/usr/bin/env bash\nprintf "%s\\0" "$0" "$1" "$PATH" > "$RUNTIME_RESULT"\n'
            for name in ("xshelf", "xs", "cx"):
                target = repo / "bin" / name
                target.write_text(mock)
                target.chmod(0o700)
            companion = base / "companion"
            (companion / "scripts").mkdir(parents=True)
            installer = companion / "scripts/install_cxops.sh"
            installer.write_text("#!/usr/bin/env bash\nexit 0\n")
            installer.chmod(0o700)
            install_bin = base / "bin ' $HOME $(touch SENTINEL) `touch SECOND` \"\nend\n"
            launcher = base / "launcher.command"
            self.run_shell([
                repo / "scripts/xshelf_suite_install.sh", "--cx-ops-repo", companion,
                "--bin-dir", install_bin, "--launcher", launcher,
            ], base, env)
            self.run_shell(["-n", launcher], base, env)
            runtime_env = dict(env, HOME=str(base / "runtime-home"))
            self.run_shell([launcher], base, runtime_env)
            records = (base / "runtime-result").read_bytes().split(b"\0")
            self.assertEqual(records[0].decode(), str(install_bin / "xshelf"))
            self.assertEqual(records[1], b"launch")
            self.assertEqual(records[2].decode(), str(install_bin) + ":" + runtime_env["HOME"] + "/.cargo/bin:" + env["PATH"])
            self.assertFalse((base / "SENTINEL").exists())
            self.assertFalse((base / "SECOND").exists())


if __name__ == "__main__":
    unittest.main()
