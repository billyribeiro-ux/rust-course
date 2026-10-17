#!/usr/bin/env python3
"""Check our portable archive, its current sources, and an isolated extraction."""
from pathlib import Path, PurePosixPath
import hashlib
import json
import subprocess
import sys
import tempfile
import zipfile
from datetime import datetime, timezone

root = Path(__file__).resolve().parents[1]
archive = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else root.parent / 'rust-course-review.zip'
excluded = {'.local', 'target', 'pkg', 'dist', 'learner-work', '.git', 'node_modules', '__pycache__'}


def portable(path):
    return (not any(part in excluded for part in path.parts)
            and path.name != '.env' and not path.name.startswith('.env.')
            and path.suffix not in ('.db', '.sqlite', '.sqlite3', '.pyc')
            and not path.name.startswith('workbench-events'))


def require(condition, message):
    if not condition:
        raise ValueError(message)


cache = root.parent / '.course-cache'
cache.mkdir(exist_ok=True)
with zipfile.ZipFile(archive) as bundle:
    members = bundle.namelist()
    require(len(members) == len(set(members)), 'Duplicate archive members')
    require(bundle.testzip() is None, 'ZIP checksum failure')
    expected = {str(PurePosixPath('rust-course') / p.relative_to(root).as_posix())
                for p in root.rglob('*') if p.is_file() and portable(p.relative_to(root))}
    require(set(members) == expected, 'Archive differs from the current portable file inventory')
    for member in members:
        path = PurePosixPath(member)
        require(not path.is_absolute() and '..' not in path.parts, 'Unsafe archive path')
        require(path.parts[0] == 'rust-course' and portable(path), 'Unexpected archive member')
        require((bundle.getinfo(member).external_attr >> 16) & 0o170000 != 0o120000, 'Symlink member')
        require(bundle.read(member) == (root / Path(*path.parts[1:])).read_bytes(), f'Source mismatch: {member}')
    with tempfile.TemporaryDirectory(prefix='archive-verification-', dir=cache) as folder:
        bundle.extractall(folder)
        extracted = Path(folder) / 'rust-course'
        command = [sys.executable, 'scripts/verify-site.py']
        result = subprocess.run(command, cwd=extracted, capture_output=True, text=True, check=True)
        static = json.loads(result.stdout)
        require(not static['issues'], str(static))
        checkpoint_count = 0
        for manifest in (extracted / 'projects/checkpoints').glob('*/checkpoint.json'):
            data = json.loads(manifest.read_text())
            for checkpoint in data['checkpoints']:
                require((extracted / checkpoint['source'] / 'Cargo.toml').is_file(), 'Missing checkpoint manifest')
                lock = checkpoint.get('lockfile')
                lock_path = extracted / lock if lock else extracted / checkpoint['source'] / 'Cargo.lock'
                require(lock_path.is_file(), 'Missing checkpoint lockfile')
                checkpoint_count += 1

digest = hashlib.sha256(archive.read_bytes()).hexdigest()
report = {
    'verified_at_utc': datetime.now(timezone.utc).isoformat(),
    'command': [sys.executable, str(Path(__file__).resolve()), str(archive)],
    'archive': str(archive), 'archive_bytes': archive.stat().st_size,
    'sha256': digest, 'files': len(members),
    'all_members_equal_current_portable_files': True,
    'zip_integrity': 'passed', 'forbidden_entries': [],
    'fresh_extraction_static_check': static,
    'checkpoint_sources_and_locks_present': checkpoint_count,
    'temporary_extraction': 'Removed only the fresh owned extraction after checks completed.',
    'limits': 'No browser rendering or new compiler checks. Scoped compiler, database, HTTP and Miri evidence is inside the archive.',
}
report_path = archive.with_name('rust-course-archive-verification.json')
report_path.write_text(json.dumps(report, indent=2) + '\n')
archive.with_suffix(archive.suffix + '.sha256').write_text(f'{digest}  {archive.name}\n')
print(json.dumps(report, indent=2))
