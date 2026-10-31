#!/usr/bin/env python3
"""Measure response tokens on explicit approved fixtures; never run fixture commands."""
import argparse
import hashlib
import importlib.metadata
import json
import re
from pathlib import Path
import subprocess
import tempfile

import evidence_measure


def digest(text):
    return hashlib.sha256(text.encode('utf-8')).hexdigest()


def materialize(case, manifest_dir):
    if 'input_file' in case:
        raw = (manifest_dir / case['input_file']).read_text(encoding='utf-8')
        if case.get('sanitize_local_paths'):
            raw = re.sub(r'/(?:Users|private/tmp|private/var/folders|var/folders)/[^\s\"\x27]+', '<LOCAL_PATH>', raw)
            raw = re.sub(r'\b[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}\b', '<UUID>', raw)
        return raw
    return (case.get('prefix', '') + ''.join(case.get('noise', '').replace('{index}', str(i)) for i in range(case.get('repeat', 0)))
            + case.get('suffix', ''))


def contains(texts, required):
    return all(any(span in text for text in texts) for span in required)


def validate_manifest(manifest):
    if not isinstance(manifest, dict) or manifest.get('schema_version') != 1 or manifest.get('approval') not in ('synthetic', 'explicit-local-validation-output'):
        raise ValueError('an explicitly approved v1 fixture manifest is required')
    cases = manifest.get('cases')
    if not isinstance(cases, list) or not cases:
        raise ValueError('fixture manifest must contain at least one case')
    names = set()
    for case in cases:
        if not isinstance(case, dict):
            raise ValueError('each fixture case must be an object')
        name = case.get('name')
        if not isinstance(name, str) or not re.fullmatch(r'[a-z][a-z0-9_]*', name) or name in names:
            raise ValueError('fixture names must be unique lowercase identifiers')
        names.add(name)
        if case.get('profile') not in ('cargo', 'xcode', 'generic') or case.get('expected_profile', case.get('profile')) not in ('cargo', 'xcode', 'generic'):
            raise ValueError(f'{name}: invalid profile')
        status = case.get('status')
        if type(status) is not int or not 0 <= status <= 255:
            raise ValueError(f'{name}: status must be an integer in 0..255')
        required = case.get('required')
        if not isinstance(required, list) or not required or any(not isinstance(s, str) or not s for s in required):
            raise ValueError(f'{name}: required spans must be nonempty strings')
        if not isinstance(case.get('provenance'), str) or not case['provenance']:
            raise ValueError(f'{name}: provenance is required')
        windows = case.get('detail_windows', [])
        if not isinstance(windows, list):
            raise ValueError(f'{name}: detail windows must be a list')
        for window in windows:
            if isinstance(window, dict):
                start, lines = window.get('start'), window.get('lines')
                targets = window.get('targets')
                if not isinstance(targets, list) or not targets or any(t not in required for t in targets):
                    raise ValueError(f'{name}: window targets must name required spans')
            elif isinstance(window, list) and len(window) == 2:
                start, lines = window
            else:
                raise ValueError(f'{name}: invalid detail window')
            if type(start) is not int or start < 1 or type(lines) is not int or not 1 <= lines <= 200:
                raise ValueError(f'{name}: invalid detail range')
    return cases


def recover(binary, project, store, result, case, initial, legacy, caller):
    responses, visible = [initial], [initial if legacy else result['summary']['text']]
    for window in case.get('detail_windows', []):
        if contains(visible, case['required']):
            break
        if isinstance(window, dict):
            if contains(visible, window['targets']):
                continue
            start, lines = window['start'], window['lines']
        else:
            start, lines = window
        if legacy:
            # Same predeclared window, obtained directly from the local fixture.
            out = subprocess.run(['sed', '-n', f'{start},{start + lines - 1}p', str(project / 'input.txt')],
                                 capture_output=True, text=True, check=True)
            responses.append(out.stdout)
            visible.append(out.stdout)
        else:
            args = ['evidence', 'show', '--store', str(store), '--id', result['id'],
                    '--start', str(start), '--lines', str(lines)]
            pieces = []
            for _ in range(100):
                response = evidence_measure.invoke(binary, caller, project.parent / 'runs.jsonl', args)
                detail = json.loads(response)
                responses.append(response)
                pieces.append(detail['text'])
                offset = detail['next_offset']
                if offset is None:
                    break
                args = args[:10] + ['--offset', str(offset)]
            else:
                raise ValueError('detail pagination exceeded study limit')
            visible.append(''.join(pieces))
    return responses, contains(visible, case['required'])


def measure(binary, root, case, raw, enc, workdir=None):
    project = root / case['name']
    project.mkdir()
    (project / 'input.txt').write_text(raw, encoding='utf-8')
    store = root / (case['name'] + '-store')
    store.mkdir(mode=0o700)
    log = root / 'runs.jsonl'
    caller = workdir or project
    legacy = evidence_measure.invoke(binary, caller, log, ['capture', 'cat', str(project / 'input.txt')])
    initial = evidence_measure.invoke(binary, caller, log, ['evidence', 'import', '--store', str(store),
               '--input', str(project / 'input.txt'), '--profile', case['profile'], '--status', str(case['status'])])
    result = json.loads(initial)
    if result.get('schema_version') != 1 or result.get('kind') != 'summary':
        raise ValueError(f"unexpected evidence response for {case['name']}")
    status_kept = result.get('exit_status') == case['status']
    profile_kept = result['summary'].get('profile') == case.get('expected_profile', case['profile'])
    old, old_kept = recover(binary, project, store, result, case, legacy, True, caller)
    new, new_kept = recover(binary, project, store, result, case, initial, False, caller)
    count = lambda responses: sum(len(enc.encode(t, disallowed_special=())) for t in responses)
    old_tokens, new_tokens = count(old), count(new)
    return dict(name=case['name'], provenance=case['provenance'], input_sha256=digest(raw),
                raw_chars=len(raw), status=case['status'], selected_profile=result['summary']['profile'],
                legacy_initial_tokens=count([legacy]), summary_initial_tokens=count([initial]),
                legacy_total_tokens=old_tokens, evidence_total_tokens=new_tokens,
                saved_tokens=old_tokens-new_tokens,
                reduction_percent=round(100*(old_tokens-new_tokens)/old_tokens, 2) if old_tokens else None,
                legacy_detail_requests=len(old)-1, evidence_detail_requests=len(new)-1,
                legacy_required_kept=old_kept, evidence_required_kept=new_kept,
                evidence_status_kept=status_kept, evidence_profile_kept=profile_kept,
                summary_truncated=result['summary']['truncated'],
                decision='go' if new_kept and old_kept and status_kept and profile_kept and new_tokens < old_tokens else 'no-go',
                response_sha256={'legacy': [digest(t) for t in old], 'evidence': [digest(t) for t in new]})


def verdict(rows):
    required = all(r['legacy_required_kept'] and r['evidence_required_kept'] and
                   r['evidence_status_kept'] and r['evidence_profile_kept'] for r in rows)
    noisy = [r for r in rows if r['raw_chars'] >= 1000]
    noisy_regressions = [r['name'] for r in noisy if r['saved_tokens'] <= 0]
    useful = bool(noisy) and not noisy_regressions
    return dict(required_evidence_and_contracts_pass=required,
                noisy_cases=len(noisy), noisy_saved_tokens=sum(r['saved_tokens'] for r in noisy),
                noisy_regressions=noisy_regressions,
                concise_regressions=[r['name'] for r in rows if r['raw_chars'] < 1000 and r['saved_tokens'] < 0],
                recommendation='selective_opt_in' if required and useful else 'hold')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--workdir', type=Path, help='Explicit existing caller project for read-only replay')
    parser.add_argument('--encoding', choices=['o200k_base'], required=True)
    args = parser.parse_args()
    import tiktoken
    if importlib.metadata.version('tiktoken') != '0.12.0':
        parser.error('use the pinned measurement dependency tiktoken==0.12.0')
    try:
        manifest = json.loads(args.manifest.read_text())
        cases = validate_manifest(manifest)
    except (OSError, ValueError) as error:
        parser.error(str(error))
    enc = tiktoken.get_encoding(args.encoding)
    binary = args.binary.resolve(strict=True)
    rows = []
    with tempfile.TemporaryDirectory(prefix='xshelf-token-study-') as temp:
        root = Path(temp).resolve()
        for case in cases:
            raw = materialize(case, args.manifest.resolve().parent)
            if not contains([raw], case['required']):
                parser.error(f"required evidence is absent from fixture {case['name']}")
            rows.append(measure(binary, root, case, raw, enc, args.workdir.resolve(strict=True) if args.workdir else None))
    gate = verdict(rows)
    report = dict(schema_version=1, tokenizer={'package': 'tiktoken', 'version': '0.12.0', 'encoding': enc.name,
                   'special_tokens': 'encoded as ordinary text',
                   'regex_version': importlib.metadata.version('regex')},
                  binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                  manifest_sha256=hashlib.sha256(args.manifest.read_bytes()).hexdigest(),
                  scope='sum of separately encoded complete stdout responses; no tool-call, framing, reasoning, caching or billing measurement',
                  retrieval='predeclared fixture windows only when required evidence is missing; not an autonomous-agent benchmark',
                  evaluation=gate, results=rows)
    print(json.dumps(report, indent=2))
    return 0 if gate['recommendation'] == 'selective_opt_in' else 1


if __name__ == '__main__':
    raise SystemExit(main())
