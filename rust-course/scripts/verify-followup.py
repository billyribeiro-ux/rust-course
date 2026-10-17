#!/usr/bin/env python3
"""Repeat the October 17 checks previously blocked by runtime selection.

Uses existing managed tools and project dependencies only; never installs tools.
"""
import datetime
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
CACHE = ROOT.parent / '.course-cache'
PROJECT = ROOT / 'experiments/svelte-bridge'
commands = [
    ('rust-version', ['mise', 'exec', '--', 'rustc', '--version'], ROOT),
    ('node-version', ['mise', 'exec', '--', 'node', '--version'], PROJECT),
    ('fuzz-version', ['mise', 'exec', '--', 'cargo', 'fuzz', '--version'], ROOT),
    ('nightly-version', ['mise', 'exec', '--', 'rustup', 'run', 'nightly-2026-10-17', 'rustc', '--version'], ROOT),
    ('fuzz', ['python3', 'scripts/fuzz-expression.py', '--cache-dir', str(CACHE / 'expression-fuzz-oct17')], ROOT),
    ('svelte-tests', ['mise', 'exec', '--', 'npm', 'test'], PROJECT),
    ('svelte-check', ['mise', 'exec', '--', 'npm', 'run', 'check'], PROJECT),
    ('svelte-build', ['mise', 'exec', '--', 'npm', 'run', 'build'], PROJECT),
    ('svelte-audit', ['mise', 'exec', '--', 'npm', 'audit', '--json'], PROJECT),
    ('svelte-http', ['python3', 'verification/svelte-http-audit.py', '--binary', str(CACHE / 'oct17-native/debug/p16-workbench')], ROOT),
]
records = []
for name, command, cwd in commands:
    print('RUN', name, flush=True)
    started = time.monotonic()
    env = dict(os.environ)
    if name == 'fuzz':
        env['CARGO_BUILD_BUILD_DIR'] = str(CACHE / 'expression-fuzz-oct17/target')
        env['CARGO_TARGET_DIR'] = env['CARGO_BUILD_BUILD_DIR']
    log = ROOT / 'verification' / ('oct17-resumed-' + name + '.log')
    with log.open('w') as output:
        output.write('$ ' + subprocess.list2cmdline(command) + '\nworking_directory=' + str(cwd) + '\n')
        output.flush()
        try:
            result = subprocess.run(command, cwd=cwd, env=env, stdout=output, stderr=subprocess.STDOUT, timeout=600)
            code = result.returncode
        except subprocess.TimeoutExpired:
            code = 124
        output.write('\nexit=' + str(code) + '\n')
    records.append(dict(name=name, command=command, working_directory=str(cwd), exit_code=code,
                        passed=code == 0, seconds=round(time.monotonic()-started, 2), log=log.name))
    report = dict(date=datetime.datetime.now(datetime.timezone.utc).isoformat(), checks=records,
                  completed=len(records) == len(commands), passed=len(records) == len(commands) and all(x['passed'] for x in records),
                  scope='Managed runtime versions, 10,000-input parser campaign, optional Svelte unit/compiler/build/advisory/real HTTP checks. No browser rendering or tool installation.')
    (ROOT / 'verification/oct17-resumed-checks.json').write_text(json.dumps(report, indent=2) + '\n')
    print(name, code, flush=True)
raise SystemExit(any(not x['passed'] for x in records))
