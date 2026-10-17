#!/usr/bin/env python3
"""Build a selected course Rust/WASM client with its locked dependency graph."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--project', type=Path, default=root / 'projects/ui-console',
                    help='Client folder containing Cargo.toml and index.html')
parser.add_argument('--wasm-bindgen', default=shutil.which('wasm-bindgen'))
parser.add_argument('--target-dir', type=Path, default=root / '.local/wasm-target')
args = parser.parse_args()
project = args.project.resolve()
manifest = project / 'Cargo.toml'
if not manifest.is_file() or not (project / 'index.html').is_file():
    parser.error('The selected client needs Cargo.toml and index.html; copy a course client first.')
if not args.wasm_bindgen:
    parser.error('The matching wasm-bindgen 0.2.129 CLI is missing. On this managed Mac, '
                 'the owner must run: mise-root use -g cargo:wasm-bindgen-cli@0.2.129; '
                 'then run this helper through mise exec. This helper never installs tools.')
version = subprocess.check_output([args.wasm_bindgen, '--version'], text=True).strip()
if version != 'wasm-bindgen 0.2.129':
    parser.error('Expected wasm-bindgen 0.2.129; got ' + version)
metadata = json.loads(subprocess.check_output([
    'mise', 'exec', '--', 'cargo', 'metadata', '--locked', '--no-deps',
    '--format-version', '1', '--manifest-path', str(manifest)
], text=True, cwd=root))
package = next((p for p in metadata['packages'] if Path(p['manifest_path']).resolve() == manifest), None)
libraries = [t for t in package['targets'] if 'cdylib' in t['crate_types']] if package else []
if len(libraries) != 1:
    parser.error('Expected exactly one cdylib target in the selected client.')
target = libraries[0]['name']
target_dir = args.target_dir.resolve()
build_env = {**os.environ, 'CARGO_BUILD_BUILD_DIR': str(target_dir)}
subprocess.run([
    'mise', 'exec', '--', 'cargo', 'build', '--locked', '--manifest-path', str(manifest),
    '--target', 'wasm32-unknown-unknown', '--target-dir', str(target_dir)
], check=True, cwd=root, env=build_env)
subprocess.run([
    args.wasm_bindgen,
    str(target_dir / 'wasm32-unknown-unknown/debug' / (target + '.wasm')),
    '--target', 'web', '--out-dir', str(project / 'pkg')
], check=True)
print(f'Built {package["name"]}: {project / "pkg"}. Serve with console-server.py --project {project}')
