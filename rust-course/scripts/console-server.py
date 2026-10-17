#!/usr/bin/env python3
"""Loopback static UI with bounded same-origin proxy to the local teaching backend."""
from http.server import ThreadingHTTPServer, SimpleHTTPRequestHandler
from pathlib import Path
from urllib.request import Request,urlopen
from urllib.error import HTTPError,URLError
import argparse
from threading import BoundedSemaphore
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--port',type=int,default=8011)
p.add_argument('--project',type=Path,default=Path(__file__).resolve().parents[1]/'projects/ui-console',help='Client directory to serve')
a=p.parse_args()
root=a.project.resolve()
if not (root/'index.html').is_file() or not (root/'Cargo.toml').is_file():
    p.error('Choose a course client directory containing index.html and Cargo.toml.')
class BoundedServer(ThreadingHTTPServer):
    # Admission occurs before ThreadingMixIn starts a thread. This is a loopback
    # teaching proxy, not a public HTTP server or a production TLS terminator.
    daemon_threads=True
    def __init__(self,*args,**kwargs):
        self.slots=BoundedSemaphore(16)
        super().__init__(*args,**kwargs)
    def get_request(self):
        request,address=super().get_request()
        request.settimeout(5) # idle socket deadline; not a total request deadline
        return request,address
    def process_request(self,request,address):
        if not self.slots.acquire(blocking=False):
            try:request.sendall(b'HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n')
            except OSError:pass
            finally:self.shutdown_request(request)
            return
        try:super().process_request(request,address)
        except BaseException:
            self.slots.release()
            raise
    def process_request_thread(self,request,address):
        try:super().process_request_thread(request,address)
        finally:self.slots.release()
class Handler(SimpleHTTPRequestHandler):
    def __init__(self,*args,**kw):super().__init__(*args,directory=str(root),**kw)
    def proxy(self):
        try:
            size=int(self.headers.get('Content-Length','0'))
            if size<0 or size>16384:self.send_error(413);return
            origin=self.headers.get('Origin')
            if origin and origin!=f'http://127.0.0.1:{a.port}':self.send_error(403);return
            endpoint=self.path.split('?')[0]
            allowed=endpoint in ('/api/snapshot','/api/orders','/api/ticks','/api/watchlist','/api/history','/api/jobs','/api/jobs/replay','/health') or (endpoint.startswith('/api/orders/') and len(endpoint)<120 and all(c.isalnum() or c in '-_' for c in endpoint.removeprefix('/api/orders/'))) or (endpoint.startswith('/api/jobs/') and endpoint.endswith('/cancel') and len(endpoint)<120)
            if not allowed:self.send_error(404);return
            headers={k:self.headers[k] for k in ('Authorization','Content-Type') if k in self.headers}
            data=self.rfile.read(size) if self.command in ('POST','PUT') else None
            try:r=urlopen(Request('http://127.0.0.1:4116'+self.path,data=data,headers=headers,method=self.command),timeout=5)
            except HTTPError as e:r=e
            payload=r.read(1048577)
            if len(payload)>1048576:self.send_error(502,'Upstream response limit');return
            self.send_response(r.status);self.send_header('Content-Type',r.headers.get('Content-Type','application/json'));self.send_header('Content-Length',str(len(payload)));self.end_headers();self.wfile.write(payload)
        except (URLError,TimeoutError,OSError):self.send_error(502,'Start the local workbench API on port 4116')
        except ValueError:self.send_error(400)
    def do_GET(self):self.proxy() if self.path.startswith(('/api/','/health')) else super().do_GET()
    def do_POST(self):self.proxy()
    def do_PUT(self):self.proxy()
server=BoundedServer(('127.0.0.1',a.port),Handler)
a.port=server.server_address[1]
print(f'Rust console: http://127.0.0.1:{a.port}',flush=True)
try:server.serve_forever()
except KeyboardInterrupt:pass
finally:server.server_close()
