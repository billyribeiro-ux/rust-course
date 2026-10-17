#!/usr/bin/env python3
"""Run a bounded parser fuzz experiment; keep evolving corpora outside source."""
import argparse
from pathlib import Path
import shutil
import subprocess

root = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--target', choices=('expression', 'bad-oracle'), default='expression')
p.add_argument('--runs', type=int, default=10000)
p.add_argument('--cache-dir', type=Path, default=root / '.local/expression-fuzz')
a = p.parse_args()
if not 1 <= a.runs <= 1000000:
    p.error('Use a finite run count between 1 and 1000000.')
cache = a.cache_dir.resolve()
corpus = cache / 'corpus' / a.target
artifacts = cache / 'artifacts'
corpus.mkdir(parents=True, exist_ok=True)
artifacts.mkdir(parents=True, exist_ok=True)
for seed in (root / 'experiments/expression-fuzz/seeds').iterdir():
    destination = corpus / seed.name
    if not destination.exists():
        shutil.copyfile(seed, destination)
command = [
    'mise', 'exec', 'cargo:cargo-fuzz@0.13.2', '--', 'rustup', 'run',
    'nightly-2026-10-17', 'cargo', 'fuzz', 'run',
    '--fuzz-dir', str(root / 'experiments/expression-fuzz'),
    '--target-dir', str(cache / 'target'), a.target, str(corpus), '--',
    f'-runs={a.runs}', '-max_len=4097', '-timeout=2', '-rss_limit_mb=512',
    '-seed=20261009', '-artifact_prefix=' + str(artifacts) + '/',
]
print('Command:', ' '.join(command), flush=True)
result = subprocess.run(command, cwd=root)
if a.target == 'bad-oracle':
    print('This target contains an intentionally false assertion. A finding is expected; inspect its input and oracle before blaming the parser.')
raise SystemExit(result.returncode)
