"""Synthetic filename and fail-closed scanner regressions."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SCANNER = Path(__file__).resolve().parents[1] / 'rust/cxrs/scripts/leak_scan.sh'


class LeakSecurity(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        target = self.root / 'rust/cxrs/scripts/leak_scan.sh'
        target.parent.mkdir(parents=True)
        shutil.copyfile(SCANNER, target)
        self.script = target
        self.git('init', '-q')

    def git(self, *args):
        return subprocess.run(['git', *args], cwd=self.root, check=True,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE)

    def scan(self, mode='--repo', env=None):
        return subprocess.run(['bash', str(self.script), mode], cwd=self.root,
                              env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                              timeout=10)

    def add(self, name, text):
        (self.root / name).write_text(text)
        self.git('add', '--', name)

    def test_literal_names(self):
        for name in ('touch SENTINEL |', '-eprint BAD', '-', 'space name',
                     'tab\tname', 'line\nname'):
            self.add(name, 'ordinary\n')
        result = self.scan()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((self.root / 'SENTINEL').exists())
        self.assertEqual(self.scan('--staged').returncode, 0)
        self.add('line\nname', 'ordinary\n' + 'operator' + '@' + 'sample.test\r\n')
        result = self.scan()
        self.assertEqual(result.returncode, 1)
        self.assertIn(b'line\nname:2:operator', result.stderr)
        self.assertNotIn(b'\r', result.stderr)

    def test_index_bytes(self):
        self.add('file', 'operator' + '@' + 'sample.test\n')
        (self.root / 'file').write_text('ordinary\n')
        self.assertEqual(self.scan('--staged').returncode, 1)
        self.assertEqual(self.scan().returncode, 0)
        self.git('add', '--', 'file')
        (self.root / 'file').write_text('operator' + '@' + 'sample.test\n')
        self.assertEqual(self.scan('--staged').returncode, 0)
        self.assertEqual(self.scan().returncode, 1)

    def test_rename_and_allowlist(self):
        self.add('old', 'reader@example.com\nreader@users.noreply.github.com\n')
        self.git('-c', 'user.name=Maintainer', '-c', 'user.email=reader@example.com',
                 'commit', '-qm', 'Fixture')
        self.git('mv', '--', 'old', 'renamed\nfile')
        self.assertEqual(self.scan('--staged').returncode, 0)
        self.add('renamed\nfile', 'operator' + '@' + 'sample.test\n')
        self.assertEqual(self.scan('--staged').returncode, 1)

    def test_input_failure(self):
        self.add('file', 'ordinary\n')
        (self.root / 'file').unlink()
        self.assertEqual(self.scan().returncode, 1)
        outside = self.root / 'outside'
        outside.write_text('ordinary\n')
        (self.root / 'file').symlink_to(outside)
        self.assertEqual(self.scan().returncode, 1)
        (self.root / 'file').unlink()
        os.mkfifo(self.root / 'file')
        self.assertEqual(self.scan().returncode, 1)

    def test_git_failure(self):
        self.add('file', 'ordinary\n')
        shutil.rmtree(self.root / '.git')
        for mode in ('--repo', '--staged'):
            result = self.scan(mode)
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn(b'scan PASS', result.stderr)

    def test_read_failure(self):
        self.add('file', '')
        (self.root / 'ReaderFault.pm').write_text(
            'BEGIN { *CORE::GLOBAL::readline = sub { $! = 5; return undef; }; } 1;')
        env = os.environ.copy()
        env.update(PERL5LIB=str(self.root), PERL5OPT='-MReaderFault')
        result = self.scan(env=env)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertNotIn(b'scan PASS', result.stderr)

    def test_partial_failure(self):
        self.add('file', '')
        (self.root / 'ReaderPartial.pm').write_text(
            'our $n = 0; BEGIN { *CORE::GLOBAL::readline = sub { '
            'if (!$n++) { $! = 5; return "ordinary\\n"; } $! = 0; return undef; }; } 1;')
        env = os.environ.copy()
        env.update(PERL5LIB=str(self.root), PERL5OPT='-MReaderPartial')
        result = self.scan(env=env)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertNotIn(b'scan PASS', result.stderr)

    def test_link_text(self):
        (self.root / 'link').symlink_to('operator' + '@' + 'sample.test')
        self.git('add', '--', 'link')
        self.assertEqual(self.scan().returncode, 1)
        self.assertEqual(self.scan('--staged').returncode, 1)


if __name__ == '__main__':
    unittest.main()
