#!/usr/bin/env python3
"""Serve only the portable course, bound to loopback."""
from http.server import ThreadingHTTPServer, SimpleHTTPRequestHandler
from functools import partial
from pathlib import Path
import argparse
p=argparse.ArgumentParser();p.add_argument('--port',type=int,default=8000);a=p.parse_args()
root=Path(__file__).resolve().parents[1]
server=ThreadingHTTPServer(('127.0.0.1',a.port),partial(SimpleHTTPRequestHandler,directory=str(root)))
print(f'Rust Workshop: http://127.0.0.1:{a.port}/ (Ctrl-C stops)',flush=True)
try:server.serve_forever()
except KeyboardInterrupt:pass
finally:server.server_close()
