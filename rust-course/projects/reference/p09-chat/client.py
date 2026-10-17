#!/usr/bin/env python3
"""Standard-library local client; start two terminals to observe broadcast."""
import socket,struct,threading
s=socket.create_connection(('127.0.0.1',4109))
def exact(n):
 b=b''
 while len(b)<n:
  chunk=s.recv(n-len(b))
  if not chunk: raise EOFError('server closed')
  b+=chunk
 return b
def receive():
 try:
  while True: print('\nchat:',exact(struct.unpack('!I',exact(4))[0]).decode())
 except (OSError,EOFError) as e: print(e)
threading.Thread(target=receive,daemon=True).start()
try:
 while True:
  text=input('> ').encode()
  if not 1<=len(text)<=1024: print('Use 1..1024 UTF-8 bytes');continue
  s.sendall(struct.pack('!I',len(text))+text)
except (EOFError,KeyboardInterrupt): s.close()
