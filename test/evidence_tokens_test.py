"""Focused checks for the optional offline token measurement tool."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
import evidence_tokens


class MeasurementTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        import tiktoken
        cls.enc = tiktoken.get_encoding('o200k_base')
        cls.binary = Path(__file__).resolve().parents[1] / 'rust/cxrs/target/debug/cxrs'

    def measure(self, raw, required, windows):
        case = dict(name='fixture', profile='cargo', status=1,
                    provenance='synthetic test', required=required, detail_windows=windows)
        with tempfile.TemporaryDirectory() as temp:
            return evidence_tokens.measure(self.binary, Path(temp).resolve(), case, raw, self.enc)

    def test_only_missing_details(self):
        raw = 'Compiling synthetic\n' * 700 + 'thread broken panicked\n'
        raw += 'context\n' * 8 + 'observed: 17\nexpected: 23\n' + 'context\n' * 8
        raw += 'test result: FAILED. 1 failed\n'
        row = self.measure(raw, ['test result:', 'observed: 17', 'expected: 23'], [
            dict(start=720, lines=1, targets=['test result:']),
            dict(start=701, lines=20, targets=['observed: 17', 'expected: 23'])])
        self.assertEqual(row['evidence_detail_requests'], 1)
        self.assertEqual(row['legacy_detail_requests'], 0)
        self.assertTrue(row['evidence_required_kept'])
        self.assertTrue(row['evidence_status_kept'])
        self.assertTrue(row['evidence_profile_kept'])
        self.assertEqual(row['saved_tokens'], row['legacy_total_tokens']-row['evidence_total_tokens'])

    def test_page_boundary_evidence(self):
        raw = '界' * 11995 + 'CROSS_BOUNDARY_TARGET' + '界' * 500
        row = self.measure(raw, ['CROSS_BOUNDARY_TARGET'], [dict(start=1, lines=1, targets=['CROSS_BOUNDARY_TARGET'])])
        self.assertEqual(row['evidence_detail_requests'], 2)
        self.assertTrue(row['evidence_required_kept'])

    def test_absent_source_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / 'manifest.json'
            path.write_text(json.dumps(dict(schema_version=1, approval='synthetic', cases=[
                dict(name='invalid', profile='generic', status=0, provenance='synthetic',
                     suffix='ordinary output', required=['missing'], detail_windows=[])])))
            out = subprocess.run([sys.executable, str(Path(evidence_tokens.__file__)),
                                  '--binary', str(self.binary), '--manifest', str(path),
                                  '--encoding', 'o200k_base'], capture_output=True, text=True)
            self.assertEqual(out.returncode, 2)
            self.assertEqual(out.stdout, '')


class VerdictTests(unittest.TestCase):
    def test_concise_regression_and_status_gate(self):
        base = dict(legacy_required_kept=True, evidence_required_kept=True,
                    evidence_status_kept=True, evidence_profile_kept=True)
        rows = [dict(base, name='noisy', raw_chars=2000, saved_tokens=100),
                dict(base, name='short', raw_chars=20, saved_tokens=-10)]
        gate = evidence_tokens.verdict(rows)
        self.assertEqual(gate['recommendation'], 'selective_opt_in')
        self.assertEqual(gate['concise_regressions'], ['short'])
        rows[0]['evidence_status_kept'] = False
        self.assertEqual(evidence_tokens.verdict(rows)['recommendation'], 'hold')
        rows[0]['evidence_status_kept'] = True
        rows[0]['saved_tokens'] = -1
        self.assertEqual(evidence_tokens.verdict(rows)['recommendation'], 'hold')

    def test_noisy_regression_cannot_hide_in_aggregate(self):
        base = dict(legacy_required_kept=True, evidence_required_kept=True,
                    evidence_status_kept=True, evidence_profile_kept=True, raw_chars=2000)
        rows = [dict(base, name='gain', saved_tokens=100),
                dict(base, name='loss', saved_tokens=-1)]
        gate = evidence_tokens.verdict(rows)
        self.assertEqual(gate['noisy_saved_tokens'], 99)
        self.assertEqual(gate['noisy_regressions'], ['loss'])
        self.assertEqual(gate['recommendation'], 'hold')

    def test_manifest_rejects_empty_duplicate_and_bad_window(self):
        case = dict(name='cargo_case', profile='cargo', status=0,
                    provenance='synthetic', required=['test result: ok'],
                    suffix='test result: ok\n', detail_windows=[])
        manifest = dict(schema_version=1, approval='synthetic', cases=[])
        with self.assertRaisesRegex(ValueError, 'at least one case'):
            evidence_tokens.validate_manifest(manifest)
        manifest['cases'] = [case, case.copy()]
        with self.assertRaisesRegex(ValueError, 'unique lowercase'):
            evidence_tokens.validate_manifest(manifest)
        manifest['cases'] = [dict(case, detail_windows=[dict(start=0, lines=1, targets=['test result: ok'])])]
        with self.assertRaisesRegex(ValueError, 'invalid detail range'):
            evidence_tokens.validate_manifest(manifest)


if __name__ == '__main__':
    unittest.main()
