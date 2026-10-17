#!/usr/bin/env python3
"""Run opt-in tests in a new private PostgreSQL 18.6 cluster; never reuse a database."""
import argparse
import datetime
import getpass
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
from urllib.parse import urlencode

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--target-dir', type=Path, default=ROOT / '.local/native-target')
args = parser.parse_args()
target = args.target_dir.resolve()
if os.name != 'posix':
    raise SystemExit('This helper requires Unix-domain sockets on macOS or Linux.')
programs = {}
for name in ('initdb', 'pg_ctl', 'createdb'):
    program = shutil.which(name)
    if not program:
        raise SystemExit(f'Install PostgreSQL 18.6 client/server tools: {name} is missing.')
    version = subprocess.run([program, '--version'], check=True, capture_output=True, text=True).stdout
    if '(PostgreSQL) 18.6' not in version:
        raise SystemExit(f'{name} must be PostgreSQL 18.6; inspect your PATH.')
    programs[name] = program

# No ambient PostgreSQL connection setting may redirect this owned fixture.
env = {k: v for k, v in os.environ.items() if not k.startswith('PG')}
env['CARGO_BUILD_BUILD_DIR'] = str(target)
fixture = Path(tempfile.mkdtemp(prefix='rust-db-test-', dir='/tmp'))
socket = fixture / 'socket'
socket.mkdir(mode=0o700)
report = {
    'date': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'fixture': str(fixture),
    'unix_socket_only': True,
    'existing_database_touched': False,
    'build_dir': str(target),
    'commands': [],
}
log_path = ROOT / 'verification/database-fixture-checks.log'
start_attempted = False
exit_code = 1
with log_path.open('w') as log:
    def run(command, *, check=True):
        log.write('$ ' + subprocess.list2cmdline(list(map(str, command))) + '\n')
        log.flush()
        result = subprocess.run(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
        report['commands'].append({'argv': list(map(str, command)), 'exit_code': result.returncode})
        if check and result.returncode:
            raise RuntimeError(f'{command[0]} failed; inspect {log_path}')
        return result.returncode

    try:
        run([programs['initdb'], '-D', str(fixture / 'data'), '--auth=trust', '--no-locale', '-E', 'UTF8'])
        start_attempted = True
        run([programs['pg_ctl'], '-D', str(fixture / 'data'), '-l', str(fixture / 'server.log'),
             '-o', f'-c listen_addresses= -c unix_socket_directories={socket} -p 65433', '-w', 'start'])
        for database, variable in [
            ('rust_course_test_issues', 'COURSE_TEST_DATABASE_URL'),
            ('rust_course_test_scheduler', 'COURSE_SCHEDULER_TEST_DATABASE_URL'),
        ]:
            run([programs['createdb'], '-h', str(socket), '-p', '65433', '-U', getpass.getuser(), database])
            env[variable] = f'postgresql:///{database}?' + urlencode({
                'host': str(socket), 'port': '65433', 'user': getpass.getuser(),
            })
        exit_code = run(['mise', 'exec', '--', 'cargo', 'test', '--locked',
                         '--manifest-path', 'projects/Cargo.toml', '--target-dir', str(target),
                         '-p', 'p10-issues', '-p', 'p14-scheduler', '--test', 'postgres',
                         '--', '--ignored', '--nocapture'], check=False)
        report['test_exit_code'] = exit_code
    finally:
        if start_attempted:
            report['stop_exit_code'] = run([programs['pg_ctl'], '-D', str(fixture / 'data'),
                                            '-m', 'fast', '-w', 'stop'], check=False)
        # If startup failed ambiguously, preserve the fixture for inspection.
        safe_to_remove = report.get('stop_exit_code') == 0 or not start_attempted
        if safe_to_remove:
            shutil.rmtree(fixture)
        report['owned_fixture_removed'] = safe_to_remove
        report['passed'] = exit_code == 0 and safe_to_remove
        (ROOT / 'verification/database-fixture-checks.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=2))
raise SystemExit(0 if report['passed'] else 1)
