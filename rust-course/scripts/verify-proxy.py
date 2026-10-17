#!/usr/bin/env python3
"""Exercise the local proxy's actual admission limit; this is not browser QA."""
from pathlib import Path
import json, socket, subprocess, sys, time
from urllib.request import urlopen

root = Path(__file__).resolve().parents[1]
command = [sys.executable, str(root / 'scripts/console-server.py'), '--port', '0']
child = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
connections = []
results = []
try:
    # The server prints only after binding; its assigned port avoids a fixed-port collision.
    address = child.stdout.readline().strip().split()[-1]
    port = int(address.rsplit(':', 1)[1])
    for _ in range(16):
        connection = socket.create_connection(('127.0.0.1', port), timeout=2)
        connection.sendall(b'POST /api/orders HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 10\r\n\r\n')
        connections.append(connection)  # deliberately withhold the announced body
    with socket.create_connection(('127.0.0.1', port), timeout=2) as rejected:
        rejected.sendall(b'GET / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n')
        response = rejected.recv(512)
        assert response.startswith(b'HTTP/1.1 503'), response
    results.append('17th connection rejected with503 while16 bodies remain incomplete')
    # The test challenges an idle deadline, not a claim about a total dripped-input deadline.
    time.sleep(5.5)
    with urlopen(address + '/', timeout=2) as response:
        assert response.status == 200
        assert b'operations_console.js' in response.read()
    results.append('idle inbound bodies release slots; subsequent static request returns200')
finally:
    for connection in connections:
        connection.close()
    child.terminate()
    try:
        child.wait(timeout=3)
    except subprocess.TimeoutExpired:
        child.kill()
        child.wait()
report = {'command': command, 'working_directory': str(root), 'passed': True,
          'checks': results, 'limits': 'Loopback only. Five-second idle socket timeout, not total request deadline. No browser claim.'}
(root / 'verification/proxy-checks.json').write_text(json.dumps(report, indent=2))
print(json.dumps(report, indent=2))
