#!/usr/bin/env python3
"""Exercise only owned local child processes; never connect database-backed services."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import select
import signal
import socket
import struct
import subprocess
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--bin-dir', required=True, type=Path)
args = parser.parse_args()
if os.name != 'posix':
    raise SystemExit('These closed-pipe and signal checks require macOS or Linux.')
bin_dir = args.bin_dir.resolve()
report = {'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'kind': 'author-team real child-process verification',
          'independent_review': 'pending', 'checks': [], 'binary_sha256': {}}


def binary(name):
    path = bin_dir / name
    report['binary_sha256'][name] = hashlib.sha256(path.read_bytes()).hexdigest()
    return str(path)


def finish(child, timeout=8):
    try:
        return child.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        child.kill()  # Only this owned child, never a process selected by port or name.
        child.communicate()
        raise AssertionError('owned child exceeded the check deadline')


def cleanup(child):
    if child.poll() is None:
        child.kill()
    child.wait(timeout=8)


def ready_line(child):
    readable, _, _ = select.select([child.stdout], [], [], 8)
    assert readable, 'owned child did not announce readiness'
    line = child.stdout.readline()
    assert line, 'owned child exited without a readiness line'
    return line.decode('utf-8').strip()


def frame(stream, text):
    body = text.encode()
    stream.sendall(struct.pack('>I', len(body)) + body)
    def read_exact(count):
        result = b''
        while len(result) < count:
            part = stream.recv(count - len(result))
            assert part, 'frame connection closed early'
            result += part
        return result
    size, = struct.unpack('>I', read_exact(4))
    assert size == len(body)
    assert read_exact(size) == body


try:
    # A preflight refuses to start the fixed-port chat fixture if occupied.
    # It never sends a request to an existing service.
    with socket.socket() as check:
        check.bind(('127.0.0.1', 4109))
    with tempfile.TemporaryDirectory(prefix='rust-service-output-') as owned:
        for name in ['p09-chat', 'p13-unsafe', 'p15-replication', 'p16-workbench']:
            env = os.environ.copy()
            if name == 'p16-workbench':
                env.update(WORKBENCH_LOG=str(Path(owned) / 'closed-output.jsonl'),
                           WORKBENCH_ADDR='127.0.0.1:0')
            writer, reader = socket.socketpair()
            reader.close()
            try:
                child = subprocess.Popen([binary(name)], cwd=owned, env=env,
                                         stdout=writer, stderr=subprocess.PIPE)
            finally:
                writer.close()
            try:
                _, error = finish(child)
                assert child.returncode != 0, (name, child.returncode)
                assert b'panicked' not in error, (name, error)
                assert b'BrokenPipe' in error or b'Broken pipe' in error, (name, error)
                if name == 'p16-workbench':
                    assert (Path(owned) / 'closed-output.jsonl').read_bytes() == b''
                report['checks'].append({'name': name + ': closed required stdout',
                                         'result': 'passed', 'exit_code': child.returncode,
                                         'stderr': error.decode('utf-8', errors='replace')})
            finally:
                cleanup(child)
        for name, variable in [('p10-issues', 'DATABASE_URL'),
                               ('p14-scheduler', 'SCHEDULER_DATABASE_URL')]:
            for kind in ['missing', 'non_unicode']:
                env = os.environ.copy()
                env.pop(variable, None)
                if kind == 'non_unicode':
                    env[variable] = b'postgresql://fixture:PRIVATE_MARKER_\xff@127.0.0.1/unused'
                child = subprocess.Popen([binary(name)], cwd=owned, env=env,
                                         stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                try:
                    output, error = finish(child)
                    assert child.returncode != 0
                    assert b'PRIVATE_MARKER' not in output + error
                    assert b'panicked' not in error
                    assert variable.encode() in error and b'UTF-8' in error
                    report['checks'].append({'name': name + ': ' + kind + ' URL',
                                             'result': 'passed', 'exit_code': child.returncode,
                                             'stderr': error.decode('utf-8', errors='replace')})
                finally:
                    cleanup(child)
        # Exchange a frame before SIGINT, which also lets the main select register its signal future.
        child = subprocess.Popen([binary('p09-chat')], cwd=owned,
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            assert '4109' in ready_line(child)
            with socket.create_connection(('127.0.0.1', 4109), timeout=3) as client:
                frame(client, 'owned lifecycle check')
                child.send_signal(signal.SIGINT)
                _, error = finish(child)
                assert child.returncode == 0, error
            report['checks'].append({'name': 'p09-chat: real frame then owned SIGINT',
                                     'result': 'passed', 'exit_code': child.returncode})
        finally:
            cleanup(child)
        env = os.environ.copy()
        env.update(WORKBENCH_LOG=str(Path(owned) / 'graceful.jsonl'), WORKBENCH_ADDR='127.0.0.1:0')
        child = subprocess.Popen([binary('p16-workbench')], cwd=owned, env=env,
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            line = ready_line(child)
            assert line.startswith('LISTEN_ADDR=')
            address = line.split('=', 1)[1]
            with urllib.request.urlopen('http://' + address + '/health', timeout=3) as response:
                assert response.status == 200
            child.send_signal(signal.SIGINT)
            _, error = finish(child)
            assert child.returncode == 0, error
            report['checks'].append({'name': 'p16-workbench: real health then owned SIGINT',
                                     'result': 'passed', 'exit_code': child.returncode})
        finally:
            cleanup(child)
    report['passed'] = True
except Exception as error:
    report['passed'] = False
    report['failure'] = str(error)
finally:
    report['finished_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    report['limits'] = ['No browser execution or independent acceptance.',
                        'Database-backed services were not connected; URL failure checks exit before connection.',
                        'SIGINT checks cover the executed graceful paths, not forced OS crashes or every accept error.',
                        'The process bounds are test timeouts, not operational service guarantees.',
                        'P13 unsafe library source is unchanged; this check does not replace the recorded Miri evidence.']
    (ROOT / 'verification/service-output-process-checks.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=2))
raise SystemExit(0 if report['passed'] else 1)
