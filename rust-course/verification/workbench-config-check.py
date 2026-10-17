#!/usr/bin/env python3
"""Unix native path/configuration regression; owns a temporary journal and child."""
import argparse
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import tempfile
import urllib.request

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--binary',type=Path,required=True)
a=p.parse_args()
binary=a.binary.resolve()
checks=[]
with tempfile.TemporaryDirectory(prefix='rust-config-check-') as temporary:
    root=os.fsencode(temporary)
    for address in (b'\xff', b'not-an-address', b'0.0.0.0:0'):
        journal=root+b'/must-not-create.jsonl'
        env=os.environb.copy();env[b'WORKBENCH_LOG']=journal;env[b'WORKBENCH_ADDR']=address
        result=subprocess.run([binary],env=env,cwd=temporary,capture_output=True,timeout=5)
        assert result.returncode!=0 and not os.path.exists(journal),(address,result.returncode)
        checks.append({'invalid_address_hex':address.hex(),'exit_code':result.returncode,'journal_created':False})
    unsupported=root+b'/journal-\xff.jsonl'
    try:
        fd=os.open(unsupported,os.O_CREAT|os.O_EXCL|os.O_WRONLY,0o600);os.close(fd)
        journal=unsupported
    except OSError as error:
        env=os.environb.copy();env[b'WORKBENCH_LOG']=unsupported;env[b'WORKBENCH_ADDR']=b'127.0.0.1:0'
        result=subprocess.run([binary],env=env,cwd=temporary,capture_output=True,timeout=5)
        assert result.returncode!=0 and not (Path(temporary)/'workbench-events.jsonl').exists()
        checks.append({'filesystem_rejects_non_utf8':True,'filesystem_errno':error.errno,'program_exit':result.returncode,'fallback_journal_created':False})
        journal=root+'/journal-🦀.jsonl'.encode()
    env=os.environb.copy();env[b'WORKBENCH_LOG']=journal;env[b'WORKBENCH_ADDR']=b'127.0.0.1:0'
    child=subprocess.Popen([binary],env=env,cwd=temporary,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
    try:
        with selectors.DefaultSelector() as ready:
            ready.register(child.stdout,selectors.EVENT_READ)
            assert ready.select(5),'No startup announcement'
        line=child.stdout.readline().strip();assert line.startswith('LISTEN_ADDR=127.0.0.1:')
        with urllib.request.urlopen('http://'+line.split('=',1)[1]+'/health',timeout=3) as response:assert response.status==200
        assert os.path.exists(journal),'Native path was not used'
        assert not (Path(temporary)/'workbench-events.jsonl').exists(),'Unexpected fallback journal'
        child.send_signal(signal.SIGINT);child.wait(timeout=5);assert child.returncode==0
        checks.append({'native_path_hex':journal.hex(),'health_status':200,'graceful_exit':0,'fallback_journal_created':False})
    finally:
        if child.poll() is None:child.kill();child.wait()
        child.communicate()
record={'date':'2026-10-09','binary':str(binary),'checks':checks,'scope':'Executed on macOS ARM. Native-byte environment/path checks are Unix-specific; no mutation of user configuration or journals.'}
Path(__file__).with_suffix('.json').write_text(json.dumps(record,indent=2))
print(json.dumps(record,indent=2))
