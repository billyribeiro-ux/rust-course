#!/usr/bin/env python3
"""Verify portable milestones and auxiliary crates against the dated course baseline."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--refresh-locks', action='store_true')
parser.add_argument('--mode', choices=('checkpoints', 'extras'), default='checkpoints')
args = parser.parse_args()
target = ROOT.parent / '.course-cache/oct17-checkpoint-native'
wasm = ROOT.parent / '.course-cache/oct17-checkpoint-wasm'
env = dict(os.environ, CARGO_BUILD_BUILD_DIR=str(target), CARGO_TARGET_DIR=str(target))
label = 'oct17-' + args.mode + ('-locks' if args.refresh_locks else '')
records = []
log = (ROOT / 'verification' / (label + '.log')).open('w')


def run(command, *, expected=0, contains=None, environment=None):
    log.write('$ ' + subprocess.list2cmdline(list(map(str, command))) + '\n')
    log.flush()
    result = subprocess.run(list(map(str, command)), cwd=ROOT, env=environment or env,
                            capture_output=True, text=True, timeout=600)
    log.write(result.stdout + result.stderr + f'\nexit={result.returncode}\n')
    log.flush()
    passed = (result.returncode == expected if expected is not None else result.returncode != 0)
    passed = passed and (contains is None or contains in result.stdout + result.stderr)
    records.append({'command': list(map(str, command)), 'exit_code': result.returncode,
                    'expected_exit': expected if expected is not None else 'nonzero',
                    'expected_text': contains, 'passed': passed})
    if not passed:
        raise RuntimeError(f'Failed: {command}; inspect {label}.log')
    return result


def cargo(action, manifest, *extra, environment=None):
    return run(['cargo', '+1.99.1', action, '--manifest-path', manifest, *extra], environment=environment)


def source_digest(folder):
    digest = hashlib.sha256()
    for path in sorted(folder.rglob('*')):
        if path.is_file():
            relative = path.relative_to(folder).as_posix().encode()
            content = path.read_bytes()
            digest.update(len(relative).to_bytes(8, 'big'))
            digest.update(relative)
            digest.update(len(content).to_bytes(8, 'big'))
            digest.update(content)
    return digest.hexdigest()


completed = False
try:
    if args.mode == 'checkpoints':
        # Keep reconstructed paths stable: test binaries embed CARGO_MANIFEST_DIR.
        # A shared Cargo cache can otherwise reuse an executable referring to a
        # removed temporary fixture directory. Never share this cache with the
        # canonical workspace or throw away source paths while reusing binaries.
        import contextlib
        source_cache = ROOT.parent / '.course-cache/oct17-checkpoint-sources'
        source_cache.mkdir(exist_ok=True)
        context = tempfile.TemporaryDirectory(prefix='course-oct17-locks-') if args.refresh_locks else contextlib.nullcontext(str(source_cache))
        with context as temporary:
            for specification in sorted((ROOT / 'projects/checkpoints').glob('*/checkpoint.json')):
                data = json.loads(specification.read_text())
                for checkpoint in data['checkpoints']:
                    print(data['project'], checkpoint['id'], flush=True)
                    destination = Path(temporary) / data['project'] / checkpoint['id']
                    if not destination.exists():
                        run(['python3', ROOT / 'scripts/start-checkpoint.py', data['project'], checkpoint['id'],
                             '--destination', destination])
                    else:
                        # Stable paths alone are insufficient: never test stale
                        # cached source after a canonical lesson or lock changes.
                        with tempfile.TemporaryDirectory(prefix='course-source-check-') as fresh:
                            candidate = Path(fresh) / 'source'
                            run(['python3', ROOT / 'scripts/start-checkpoint.py', data['project'], checkpoint['id'],
                                 '--destination', candidate])
                            if source_digest(candidate) != source_digest(destination):
                                raise RuntimeError(f'Cached checkpoint differs from current source: {destination}. '
                                                   'Move this owned cache directory aside before rerunning.')
                    manifest = destination / 'Cargo.toml'
                    if args.refresh_locks:
                        cargo('update', manifest, '--offline')
                        canonical = ROOT / checkpoint['lockfile'] if checkpoint.get('lockfile') else ROOT / checkpoint['source'] / 'Cargo.lock'
                        shutil.copyfile(destination / 'Cargo.lock', canonical)
                    elif data['project'] == 'p11-console':
                        cargo('check', manifest, '--locked', '--target', 'wasm32-unknown-unknown',
                              '--target-dir', wasm, environment=dict(env, CARGO_BUILD_BUILD_DIR=str(wasm)))
                    else:
                        cargo('test', manifest, '--locked', '--target-dir', target)
    else:
        for rel in ('projects/experiments/p07-rayon', 'exercises/specializations/no-std', 'projects/scaffolds/p11-console', 'experiments/expression-fuzz'):
            manifest = ROOT / rel / 'Cargo.toml'
            if args.refresh_locks:
                cargo('update', manifest)
            elif 'p11-console' in rel:
                cargo('check', manifest, '--locked', '--target', 'wasm32-unknown-unknown', '--target-dir', wasm,
                      environment=dict(env, CARGO_BUILD_BUILD_DIR=str(wasm)))
            elif 'fuzz' in rel:
                cargo('check', manifest, '--locked', '--all-targets')
            else:
                cargo('test', manifest, '--locked')
                cargo('clippy', manifest, '--locked', '--all-targets', '--', '-D', 'warnings')
        if not args.refresh_locks:
            for manifest in sorted((ROOT / 'projects/scaffolds').glob('*/Cargo.toml')):
                if manifest.parent.name != 'p11-console':
                    cargo('check', manifest, '--locked', '--all-targets')
            with tempfile.TemporaryDirectory(prefix='course-diagnostics-') as temporary:
                for item in json.loads((ROOT / 'exercises/intentional-failures.json').read_text()):
                    if item['kind'] != 'compile-failure':
                        continue
                    expected = re.search(r'E\d{4}', item['expected'])
                    run(['rustc', '+1.99.1', '--edition=2024', '--emit=metadata', '--out-dir', temporary,
                         ROOT / item['path']], expected=None, contains=expected.group() if expected else None)
                for kind in ('broken', 'solution'):
                    binary = Path(temporary) / kind
                    run(['rustc', '+1.99.1', '--edition=2024', '--test',
                         ROOT / f'exercises/services/partial-read-{kind}.rs', '-o', binary])
                    run([binary], expected=101 if kind == 'broken' else 0)
            for file in (ROOT / 'projects/reference').rglob('*.rs'):
                if re.search(r'\b(?:todo|unimplemented)!', file.read_text()):
                    raise RuntimeError(f'Unfinished reference: {file}')
    completed = True
finally:
    log.close()
    report = {'date': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'toolchain': '1.99.1',
              'mode': args.mode, 'refreshed_locks': args.refresh_locks, 'checks': records,
              'log': label + '.log', 'passed': completed and bool(records) and all(x['passed'] for x in records),
              'limits': 'Scaffolds are learner assignments: compilation is checked, TODO acceptance tests are intentionally unsolved. Database and browser execution are separate.'}
    (ROOT / 'verification' / (label + '.json')).write_text(json.dumps(report, indent=2) + '\n')
