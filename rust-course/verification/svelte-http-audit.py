#!/usr/bin/env python3
"""Real local HTTP verification of owned Rust/Vite processes; no browser access."""
import argparse,concurrent.futures,datetime,hashlib,json,os,pathlib,signal,socket,subprocess,tempfile,time,urllib.error,urllib.request
ROOT=pathlib.Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--binary',type=pathlib.Path,required=True);args=p.parse_args();binary=args.binary.resolve();project=ROOT/'experiments/svelte-bridge'
checks=[];requests=[];children=[];failure=None;started=datetime.datetime.now(datetime.timezone.utc).isoformat()
def record(name,**facts):checks.append({'check':name,'result':'passed',**facts});print('PASS',name,flush=True)
def call(base,method,path,body=None,headers=None,expected=200):
 payload=None if body is None else(json.dumps(body).encode()if not isinstance(body,bytes)else body)
 h={'Content-Type':'application/json','Authorization':'Bearer demo-reader'};h.update(headers or {})
 try:r=urllib.request.urlopen(urllib.request.Request(base+path,data=payload,headers=h,method=method),timeout=8)
 except urllib.error.HTTPError as error:r=error
 with r:status=r.status;data=r.read(1_048_577);content_type=r.headers.get('Content-Type','')
 requests.append({'base':base,'method':method,'path':path,'status':status,'expected':expected,'bytes':len(data)})
 assert status==expected,(path,status,expected,data[:200])
 return data,content_type
for port in (4116,4176):
 probe=socket.socket()
 try:probe.bind(('127.0.0.1',port))
 except OSError as error:raise SystemExit(f'Port{port} is already occupied or unavailable; no existing process was touched: {error}')
 finally:probe.close()
with tempfile.TemporaryDirectory(prefix='course-svelte-http-')as tmp:
 tmp=pathlib.Path(tmp);logs=[]
 try:
  for label,cmd,cwd,environment in [
   ('rust',[str(binary)],ROOT,dict(os.environ,WORKBENCH_LOG=str(tmp/'events.jsonl'),WORKBENCH_ADDR='127.0.0.1:4116')),
   ('vite',['mise','exec','--','node',str(project/'node_modules/vite/bin/vite.js')],project,dict(os.environ))]:
   output=(tmp/(label+'.log')).open('w+');logs.append((label,output))
   process=subprocess.Popen(cmd,cwd=cwd,env=environment,stdout=output,stderr=subprocess.STDOUT);children.append((label,process))
   base='http://127.0.0.1:'+('4116'if label=='rust'else'4176');path='/health'if label=='rust'else'/'
   until=time.monotonic()+15
   while True:
    if process.poll()is not None:raise AssertionError(f'{label} exited during startup with{process.returncode}')
    try:call(base,'GET',path);break
    except (urllib.error.URLError,TimeoutError,ConnectionError):
     if time.monotonic()>=until:raise
     time.sleep(.05)
  rust=children[0][1];backend='http://127.0.0.1:4116';proxy='http://127.0.0.1:4176'
  # Populate a separate synthetic journal; no learner or user database is involved.
  symbols=['RUST']+['X'+chr(65+n//26)+chr(65+n%26)for n in range(31)]
  for n,symbol in enumerate(symbols):
   call(backend,'POST','/api/ticks',{'event_id':f'bridge-tick-{n}','symbol':symbol,'price_cents':125+n,'sequence':18446744073709551615 if n==0 else 1},{'Authorization':'Bearer demo-operator'})
  call(backend,'POST','/api/ticks',{'event_id':'too-many','symbol':'LIMIT','price_cents':1,'sequence':1},{'Authorization':'Bearer demo-operator'},expected=422)
  call(backend,'POST','/api/orders',{'order_id':'bridge-buy','symbol':'RUST','side':'buy','quantity':3},{'Authorization':'Bearer demo-operator'})
  payload,_=call(proxy,'GET','/api/snapshot');snapshot=json.loads(payload)
  assert len(snapshot['quotes'])==32 and snapshot['cash_cents']==999625 and snapshot['events']==33
  assert next(q for q in snapshot['quotes']if q['symbol']=='RUST')['sequence']==18446744073709551615
  assert b'18446744073709551615'in payload
  record('real Rust snapshot through Vite preserves integer text, ledger and32-symbol budget',response_bytes=len(payload),events=snapshot['events'])
  # Run the same JS request/decoder module against the real development proxy.
  node="""import assert from 'node:assert/strict';const m=await import(process.argv[1]);const c=m.createSnapshotClient({fetcher:(path,options)=>fetch('http://127.0.0.1:4176'+path,options)});const s=await c.load();assert.equal(s.cash_cents,999625n);assert.equal(s.quotes.find(q=>q.symbol==='RUST').sequence,18446744073709551615n);assert.equal(s.quotes.length,32);console.log('actual client.load decoded bigint snapshot');"""
  result=subprocess.run(['mise','exec','--','node','--input-type=module','-e',node,(project/'src/client.js').as_uri()],cwd=project,capture_output=True,text=True,timeout=10)
  assert result.returncode==0,result.stderr;record('actual JavaScript client and lossless decoder consume real proxy response',stdout=result.stdout.strip())
  for method,path,body,headers,status in [
   ('POST','/api/snapshot',{},None,405),('POST','/api/orders',{},None,404),('GET','/api/jobs',None,None,404),
   ('GET','/api/snapshot',b'x',None,400),('GET','/api/snapshot',None,{'Origin':'https://untrusted.invalid'},403),
   ('GET','/api/snapshot',None,{'Authorization':'Bearer unknown'},401)]:
   call(proxy,method,path,body,headers,expected=status)
  after,_=call(proxy,'GET','/api/snapshot');assert json.loads(after)==snapshot
  record('read-only route, body, origin and identity rejection preserves state')
  for path in ['/', '/src/main.js','/src/App.svelte','/src/client.js','/@vite/client']:
   body,content_type=call(proxy,'GET',path)
   assert body and(('text/html'in content_type)if path=='/'else('javascript'in content_type)),(path,content_type)
   if path=='/':assert b'src/main.js'in body
  record('HTML entry and development module assets are served successfully')
  # Stop only our owned backend so4 proxy upstream reads remain in progress.
  held=[]
  try:
   rust.send_signal(signal.SIGSTOP);os.waitpid(rust.pid,os.WUNTRACED)
   for _ in range(4):
    connection=socket.create_connection(('127.0.0.1',4176),timeout=5)
    connection.sendall(b'GET /api/snapshot HTTP/1.1\r\nHost: 127.0.0.1:4176\r\nAuthorization: Bearer demo-reader\r\nConnection: close\r\n\r\n');held.append(connection)
   time.sleep(.15);call(proxy,'GET','/api/snapshot',expected=429)
   rust.send_signal(signal.SIGCONT)
   for connection in held:
    received=connection.recv(65536);assert received.startswith(b'HTTP/1.1 200'),received[:160]
   record('four real upstream requests enforce fifth-request admission429')
  finally:
   rust.send_signal(signal.SIGCONT)
   for connection in held:connection.close()
  time.sleep(.1);call(proxy,'GET','/api/snapshot');record('capacity returns after real responses close')
  try:
   rust.send_signal(signal.SIGSTOP);os.waitpid(rust.pid,os.WUNTRACED)
   node="""import assert from 'node:assert/strict';const m=await import(process.argv[1]);const c=m.createSnapshotClient({fetcher:(path,options)=>fetch('http://127.0.0.1:4176'+path,options)});const start=performance.now();try{await c.load();throw new Error('unexpected success')}catch(error){assert(error.name==='AbortError'||/HTTP 5/.test(error.message));console.log(JSON.stringify({error:error.name,message:error.message,elapsed_ms:performance.now()-start}));}"""
   result=subprocess.run(['mise','exec','--','node','--input-type=module','-e',node,(project/'src/client.js').as_uri()],cwd=project,capture_output=True,text=True,timeout=8)
   assert result.returncode==0,result.stderr;details=json.loads(result.stdout);assert 2000<=details['elapsed_ms']<=7000;record('real client/proxy timeout produces bounded failure',**details)
  finally:rust.send_signal(signal.SIGCONT)
  time.sleep(.1);final,_=call(proxy,'GET','/api/snapshot');assert json.loads(final)==snapshot;record('server state and proxy recover after timeout')
 except Exception as error:
  failure={'type':type(error).__name__,'message':str(error)};print('FAIL',failure,flush=True)
 finally:
  for label,child in reversed(children):
   if child.poll()is None:
    child.send_signal(signal.SIGCONT);child.send_signal(signal.SIGINT)
    try:child.wait(timeout=5)
    except subprocess.TimeoutExpired:child.kill();child.wait()
  process_logs={}
  for label,handle in logs:
   handle.flush();handle.seek(0);process_logs[label]=handle.read();handle.close()
  evidence={'started_utc':started,'finished_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'command':['python3','verification/svelte-http-audit.py','--binary',str(binary)],'overall':'passed'if failure is None else'failed','checks':checks,'requests':requests,'processes':[{'label':label,'pid':child.pid,'exit':child.returncode}for label,child in children],'process_logs':process_logs,'failure':failure,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'limits':['No browser was opened or automated. These are real HTTP and Node-client module checks, not rendered component interaction.','Only owned Rust/Vite processes and a temporary synthetic journal were used; occupied ports cause refusal.','SIGSTOP/SIGCONT targeted the owned Rust child to create a controlled slow upstream; no user processes or databases were touched.','Screen-reader behavior, keyboard interaction, visual layout and Svelte DOM lifecycle remain unverified.']}
  (ROOT/'verification/svelte-http-audit.json').write_text(json.dumps(evidence,indent=2)+'\n')
print(json.dumps({'overall':evidence['overall'],'checks':len(checks),'http_requests':len(requests)}));raise SystemExit(0 if failure is None else 1)
