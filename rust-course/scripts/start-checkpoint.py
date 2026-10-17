#!/usr/bin/env python3
"""Reconstruct one immutable checkpoint into a fresh standalone learner directory.

Only source is copied. Inherited Cargo fields become explicit and local path
crates are copied under _deps. Existing destinations are never overwritten.
"""
import argparse
import copy
import json
import os
from pathlib import Path
import re
import shutil
import shlex
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
SHARED = tomllib.loads((ROOT / 'projects/Cargo.toml').read_text())['workspace']
EXCLUDED = {'target', '.git', '.local', 'node_modules', '__pycache__', 'pkg'}


def inside(path):
    path = path.resolve()
    if not path.is_relative_to(ROOT):
        raise ValueError(f'Source must remain inside the course: {path}')
    return path


def value(item):
    if isinstance(item, bool):
        return 'true' if item else 'false'
    if isinstance(item, str):
        return json.dumps(item, ensure_ascii=False)
    if isinstance(item, (int, float)):
        return str(item)
    if isinstance(item, list):
        return '[' + ', '.join(value(v) for v in item) + ']'
    if isinstance(item, dict):
        return '{ ' + ', '.join(key(k) + ' = ' + value(v) for k, v in item.items()) + ' }'
    raise ValueError(f'Unsupported Cargo value: {type(item).__name__}')


def key(text):
    return text if re.fullmatch(r'[A-Za-z0-9_-]+', text) else json.dumps(text)


def dump(manifest):
    sections = []
    for name, item in manifest.items():
        if isinstance(item, dict):
            sections.append('[' + key(name) + ']\n' + '\n'.join(key(k) + ' = ' + value(v) for k, v in item.items()))
        else:
            sections.insert(0, key(name) + ' = ' + value(item))
    return '# Materialized by start-checkpoint.py; all inherited values are explicit.\n' + '\n\n'.join(sections) + '\n'


def materialize(source, destination, top, copied):
    source = inside(source)
    if source in copied:
        return copied[source]
    copied[source] = destination
    for directory, names, files in os.walk(source):
        names[:] = [name for name in names if name not in EXCLUDED]
        for name in names + files:
            entry = Path(directory) / name
            if entry.is_symlink():
                raise ValueError(f'Refusing source symlink: {entry}')
    shutil.copytree(source, destination, ignore=lambda _, names: [n for n in names if n in EXCLUDED])
    manifest_path = destination / 'Cargo.toml'
    manifest = tomllib.loads(manifest_path.read_text())
    package = manifest.get('package', {})
    for name, setting in list(package.items()):
        if isinstance(setting, dict) and setting.get('workspace') is True:
            package[name] = copy.deepcopy(SHARED['package'][name])
    package.pop('workspace', None)

    def deps(table):
        for section in ('dependencies', 'dev-dependencies', 'build-dependencies'):
            for name, setting in list(table.get(section, {}).items()):
                if not isinstance(setting, dict):
                    continue
                origin = source
                if setting.get('workspace') is True:
                    setting = dict(setting)
                    setting.pop('workspace')
                    inherited = copy.deepcopy(SHARED['dependencies'][name])
                    if isinstance(inherited, str):
                        inherited = {'version': inherited}
                    if 'features' in setting:
                        setting['features'] = sorted(set(inherited.get('features', []) + setting['features']))
                    inherited.update(setting)
                    setting = inherited
                    origin = ROOT / 'projects'
                if 'path' in setting:
                    dependency = inside(origin / setting['path'])
                    dependency_manifest = tomllib.loads((dependency / 'Cargo.toml').read_text())
                    dependency_name = dependency_manifest['package']['name']
                    if not re.fullmatch(r'[A-Za-z0-9_-]+', dependency_name):
                        raise ValueError('Unsupported path dependency package name')
                    dependency_destination = copied.get(dependency, top / '_deps' / dependency_name)
                    if dependency not in copied:
                        materialize(dependency, dependency_destination, top, copied)
                    setting['path'] = os.path.relpath(dependency_destination, destination)
                table[section][name] = setting
        for target in table.get('target', {}).values():
            deps(target)

    deps(manifest)
    if manifest.get('lints', {}).get('workspace') is True:
        manifest['lints'] = copy.deepcopy(SHARED.get('lints', {}))
    if destination == top:
        manifest['workspace'] = {}
    else:
        # Path crates join the reconstructed root; nested workspace roots fail Cargo.
        manifest.pop('workspace', None)
    manifest_path.write_text(dump(manifest))
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('project', help='Project prefix, e.g. p09')
    parser.add_argument('checkpoint', help='Checkpoint ID from projects/checkpoints/index.html')
    parser.add_argument('--destination', type=Path, help='New directory; never overwrite')
    args = parser.parse_args()
    choices = [p for p in (ROOT / 'projects/checkpoints').glob('*/checkpoint.json') if p.parent.name.startswith(args.project)]
    if len(choices) != 1:
        raise ValueError('Project prefix must select exactly one checkpoint manifest')
    specification = json.loads(choices[0].read_text())
    if specification.get('schema_version') != 1:
        raise ValueError('Unsupported checkpoint schema')
    selected = [c for c in specification['checkpoints'] if c['id'] == args.checkpoint]
    if len(selected) != 1:
        raise ValueError('Choose one checkpoint: ' + ', '.join(c['id'] for c in specification['checkpoints']))
    checkpoint = selected[0]
    destination = (args.destination or ROOT / 'learner-work/checkpoints' / specification['project'] / checkpoint['id']).absolute()
    source = inside(ROOT / checkpoint['source'])
    if destination.resolve().is_relative_to(source):
        raise ValueError('Destination must not be inside its immutable source directory')
    if destination.exists() or destination.is_symlink():
        raise ValueError(f'Refusing to overwrite {destination}; choose a new --destination')
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.checkpoint-build-', dir=destination.parent) as temporary:
        staged = Path(temporary) / 'source'
        materialize(ROOT / checkpoint['source'], staged, staged, {})
        if checkpoint.get('lockfile'):
            shutil.copyfile(inside(ROOT / checkpoint['lockfile']), staged / 'Cargo.lock')
        elif not (staged / 'Cargo.lock').exists():
            shutil.copyfile(ROOT / 'projects/Cargo.lock', staged / 'Cargo.lock')
        if not (staged / 'rust-toolchain.toml').exists():
            shutil.copyfile(ROOT / 'rust-toolchain.toml', staged / 'rust-toolchain.toml')
        (staged / 'checkpoint-origin.json').write_text(json.dumps({'schema_version': 1, 'project': specification['project'], 'checkpoint': checkpoint}, indent=2) + '\n')
        # mkdir is the no-overwrite gate even if another process races us.
        destination.mkdir()
        for entry in staged.iterdir():
            shutil.move(str(entry), destination / entry.name)
    print(f'Created {destination}')
    print(checkpoint['contract'])
    print('Working directory: course root; test with the current pinned mise toolchain:')
    if specification['project'] == 'p11-console':
        print('mise exec -- python3 scripts/build-ui.py --project ' + shlex.quote(str(destination)))
        print('python3 scripts/console-server.py --project ' + shlex.quote(str(destination)))
    else:
        print('mise exec -- cargo test --locked --manifest-path ' + shlex.quote(str(destination / 'Cargo.toml')))
    print('Use an explicit --target-dir to reuse your chosen verification cache. No build output was copied.')


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError) as error:
        raise SystemExit(str(error)) from error
