#!/usr/bin/env python3
"""Real HTTP acceptance audit; owns only temporary journals and child processes.
Run from any directory with --binary PATH. No browser or database is accessed.
"""
import argparse,datetime,hashlib,http.client,json,os,pathlib,selectors,signal,socket,subprocess,tempfile,time,traceback,urllib.error,urllib.request
COURSE=pathlib.Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser();parser.add_argument('--binary',required=True);parser.add_argument('--output',default=str(COURSE/'verification/final-runtime-audit.json'));args=parser.parse_args()
BINARY=pathlib.Path(args.binary).resolve();checks=[];requests=[];children=[];process_events=[]
started=datetime.datetime.now(datetime.timezone.utc).isoformat()
def record(name,**facts):checks.append({'check':name,'result':'passed',**facts});print('PASS',name,flush=True)
class Server:
 def __init__(self,journal):
  self.journal=pathlib.Path(journal);self.proc=subprocess.Popen([str(BINARY)],env=dict(os.environ,WORKBENCH_LOG=str(journal),WORKBENCH_ADDR='127.0.0.1:0'),stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True);children.append(self)
  selector=selectors.DefaultSelector();selector.register(self.proc.stdout,selectors.EVENT_READ)
  if not selector.select(8):self.stop();raise RuntimeError('startup did not announce address within8s')
  line=self.proc.stdout.readline().strip();selector.close()
  if not line.startswith('LISTEN_ADDR=127.0.0.1:'):
   self.proc.wait(timeout=3);raise RuntimeError('startup failed: '+line+' '+self.proc.stderr.read())
  self.address=line.split('=',1)[1];self.base='http://'+self.address;self.port=int(self.address.rsplit(':',1)[1]);process_events.append({'event':'started','pid':self.proc.pid,'journal':self.journal.name,'address':self.address})
 def stop(self):
  if self.proc.poll() is None:
   self.proc.send_signal(signal.SIGCONT)
   self.proc.send_signal(signal.SIGINT)
   try:self.proc.wait(timeout=5)
   except subprocess.TimeoutExpired:self.proc.kill();self.proc.wait();raise RuntimeError('owned server failed graceful shutdown')
  process_events.append({'event':'stopped','pid':self.proc.pid,'exit':self.proc.returncode})
 def call(self,method,path,body=None,role='demo-operator',expect=200,raw=None):
  data=raw if raw is not None else(None if body is None else json.dumps(body).encode())
  headers={'Content-Type':'application/json'}
  if role is not None:headers['Authorization']='Bearer '+role
  request=urllib.request.Request(self.base+path,data=data,headers=headers,method=method)
  try:response=urllib.request.urlopen(request,timeout=8)
  except urllib.error.HTTPError as error:response=error
  with response:
   status=response.status;payload=response.read(1_048_577)
  try:result=json.loads(payload)
  except json.JSONDecodeError:result=payload.decode(errors='replace')
  requests.append({'method':method,'path':path,'status':status,'expected':expect,'body':body})
  assert status==expect,(method,path,status,expect,result)
  return result
 def snapshot(self):return self.call('GET','/api/snapshot',role='demo-reader')
 def jobs(self):return self.call('GET','/api/jobs',role='demo-reader')
 def job(self,id):return next(j for j in self.jobs()if j['job_id']==id)
 def wait_job(self,id,predicate,timeout=10):
  deadline=time.monotonic()+timeout
  while time.monotonic()<deadline:
   job=self.job(id)
   if predicate(job):return job
   time.sleep(.03)
  raise AssertionError(('job did not reach expected state',id,job))
def tick(id,symbol,price,sequence):return dict(event_id=id,symbol=symbol,price_cents=price,sequence=sequence)
def order(id,quantity,side='buy'):return dict(order_id=id,symbol='RUST',side=side,quantity=quantity)
def replay(id,symbol,count=64):return dict(job_id=id,ticks=[tick(f'{id}-{n}',symbol,100+n,n)for n in range(1,count+1)])
error=None
temporary=tempfile.TemporaryDirectory(prefix='rust-course-runtime-audit-')
try:
 directory=temporary.name
 if directory:
  root=pathlib.Path(directory);journal=root/'main.jsonl';server=Server(journal)
  server.call('GET','/health',role=None)
  server.call('GET','/api/snapshot',role=None,expect=401)
  server.call('POST','/api/orders',order('reader-denied',1),role='demo-reader',expect=403)
  server.call('POST','/api/orders',order('missing-quote',1),expect=422)
  assert server.snapshot()['events']==0;record('identity and definite input rejection before mutation')
  base_tick=tick('tick-1','RUST',125,1)
  assert server.call('POST','/api/ticks',base_tick)['applied']
  assert not server.call('POST','/api/ticks',base_tick)['applied']
  server.call('POST','/api/ticks',tick('tick-1','RUST',126,1),expect=409)
  server.call('POST','/api/ticks',tick('older','RUST',123,0),expect=422)
  assert server.snapshot()['events']==1;record('tick identity conflict and out-of-order rejection')
  assert server.call('POST','/api/orders',order('buy-1',3))['applied']
  assert not server.call('POST','/api/orders',order('buy-1',3))['applied']
  server.call('POST','/api/orders',order('buy-1',4),expect=409)
  server.call('POST','/api/orders',order('oversell',4,'sell'),expect=422)
  server.call('POST','/api/orders',order('sell-1',2,'sell'))
  snap=server.snapshot();assert snap['cash_cents']==999875;assert snap['positions']==[{'symbol':'RUST','quantity':1}]
  fill=server.call('GET','/api/orders/buy-1',role='demo-reader');assert fill['quantity']==3 and fill['price_cents']==125
  server.call('GET','/api/orders/absent',role='demo-reader',expect=404)
  record('orders duplicate retry conflict ledger and exact lookup',cash_cents=snap['cash_cents'],holdings=1)
  server.call('PUT','/api/watchlist',{'symbols':['RUST','LAB']})
  before=server.snapshot();server.call('PUT','/api/watchlist',{'symbols':['RUST','RUST']},expect=422);assert server.snapshot()==before
  history=server.call('GET','/api/history?after=0&limit=1',role='demo-reader');assert len(history['points'])==1
  after=history['points'][0]['event_position'];assert server.call('GET',f'/api/history?after={after}&limit=1',role='demo-reader')['points']==[]
  server.call('GET','/api/history?limit=257',role='demo-reader',expect=400)
  server.call('POST','/api/orders',raw=b'x'*17000,expect=413)
  record('watchlist and bounded historical page/body contracts')
  # Hold all16 admitted request bodies incomplete. They reach middleware but no domain operation.
  held=[]
  try:
   for _ in range(16):
    client=socket.create_connection(('127.0.0.1',server.port),timeout=2);client.sendall(b'POST /api/orders HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer demo-operator\r\nContent-Type: application/json\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{');held.append(client)
   time.sleep(.15);server.call('GET','/api/snapshot',role='demo-reader',expect=429)
  finally:
   for client in held:client.close()
  time.sleep(.1);assert server.snapshot()==before
  record('sixteen incomplete bodies trigger overload429 without domain mutation')
  slow=socket.create_connection(('127.0.0.1',server.port),timeout=8)
  slow.sendall(b'POST /api/orders HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer demo-operator\r\nContent-Type: application/json\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{')
  begin=time.monotonic();response=slow.recv(2048);duration=time.monotonic()-begin;slow.close();assert response.startswith(b'HTTP/1.1 408'),response
  assert server.snapshot()==before;record('real five-second slow-body deadline',elapsed_seconds=duration)
  cancelled_spec=replay('cancel-job','LAB');server.call('POST','/api/jobs/replay',cancelled_spec)
  assert not server.call('POST','/api/jobs/replay',cancelled_spec)['applied']
  server.wait_job('cancel-job',lambda j:j['next']>=1)
  server.call('POST','/api/jobs/cancel-job/cancel',{})
  stopped=server.job('cancel-job');time.sleep(.25);assert server.job('cancel-job')==stopped and stopped['status']=='cancelled'
  record('persistent replay identity and cancellation boundary',committed_before_cancel=stopped['next'])
  recovery_spec=replay('recover-job','RECOVER');server.call('POST','/api/jobs/replay',recovery_spec)
  observed=server.wait_job('recover-job',lambda j:j['next']>=2)
  # Stop at an inspected complete prefix, then kill only our owned child. This tests
  # abrupt process loss; it is deliberately not a torn-write or power-loss guarantee.
  if hasattr(signal,'SIGSTOP'):
   server.proc.send_signal(signal.SIGSTOP);time.sleep(.02);data=journal.read_bytes()
   assert data.endswith(b'\n');[json.loads(line)for line in data.splitlines()]
   server.proc.kill();server.proc.wait(timeout=3);process_events.append({'event':'controlled SIGKILL at complete observed journal prefix','pid':server.proc.pid,'observed_job_next':observed['next'],'bytes':len(data)})
  else:
   server.stop();process_events.append({'event':'SIGSTOP unsupported; graceful restart used'})
  server=Server(journal);recovered=server.wait_job('recover-job',lambda j:j['status']=='completed')
  assert recovered['next']==64;assert server.job('cancel-job')['status']=='cancelled'
  assert not server.call('POST','/api/jobs/replay',recovery_spec)['applied']
  conflict=replay('recover-job','OTHER');server.call('POST','/api/jobs/replay',conflict,expect=409)
  restored=server.snapshot();assert restored['cash_cents']==999875 and restored['watchlist']==['RUST','LAB']
  assert server.call('GET','/api/orders/buy-1',role='demo-reader')==fill
  record('abrupt owned-process restart replays orders and resumes job cursor',observed_before_restart=observed['next'],completed_after_restart=recovered['next'])
  server.stop()
  legacy=root/'legacy.jsonl';legacy.write_bytes((COURSE/'projects/reference/p16-workbench/fixtures/legacy-v1.jsonl').read_bytes())
  server=Server(legacy);assert server.snapshot()['cash_cents']==999750
  server.call('PUT','/api/watchlist',{'symbols':['RUST','LAB']});server.stop()
  records=[json.loads(line)for line in legacy.read_text().splitlines()];assert [r['version']for r in records]==[1,1,2] and records[-1]['position']==3
  server=Server(legacy);assert server.snapshot()['watchlist']==['RUST','LAB'];server.stop()
  record('legacy v1 journal reads append v2 position3 and restart')
  corrupt=root/'bad-position.jsonl';records[-1]['position']=99;corrupt.write_text('\n'.join(json.dumps(r)for r in records)+'\n')
  rejected=subprocess.run([str(BINARY)],env=dict(os.environ,WORKBENCH_LOG=str(corrupt),WORKBENCH_ADDR='127.0.0.1:0'),capture_output=True,text=True,timeout=5)
  assert rejected.returncode!=0 and 'non-contiguous journal position' in rejected.stderr;record('corrupt record position fails startup without silent repair',exit_code=rejected.returncode)
  server=Server(root/'admission.jsonl')
  for n in range(16):server.call('POST','/api/jobs/replay',dict(job_id=f'budget-{n:02d}',ticks=[tick(f'budget-tick-{n}','BUDGET',100,n+1)]))
  server.call('POST','/api/jobs/replay',dict(job_id='budget-17',ticks=[tick('budget-tick-17','BUDGET',100,17)]),expect=422)
  assert len(server.jobs())==16;record('retained replay job budget rejects seventeenth job')
  server.stop()
except Exception as failure:
 error={'type':type(failure).__name__,'message':str(failure),'traceback':traceback.format_exc()};print(error['traceback'],flush=True)
finally:
 for child in children:
  if child.proc.poll()is None:
   try:child.stop()
   except Exception as failure:process_events.append({'cleanup_error':str(failure)})
 temporary.cleanup()
 result={'started_utc':started,'finished_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'command':['python3',str(pathlib.Path(__file__).resolve()),'--binary',str(BINARY),'--output',args.output],'binary_sha256':hashlib.sha256(BINARY.read_bytes()).hexdigest(),'overall':'passed'if error is None else'failed','checks':checks,'requests':requests,'process_events':process_events,'failure':error,'review':{'author_runtime_review':'performed','independent_review':'pending'},'limits':['Direct real HTTP backend audit; browser rendering and same-origin proxy handled separately by root.','Only temporary synthetic journals and owned child processes used; no user database accessed.','Controlled process crash used an inspected complete journal prefix; torn-write/power-loss recovery is not claimed.','Append I/O failure and poisoned-store503 are covered by existing Rust regression tests, not simulated disk damage here.','Public local fixture identities are not production authentication; this audit does not certify deployment security.']}
 pathlib.Path(args.output).write_text(json.dumps(result,indent=2)+'\n')
 print(json.dumps({'overall':result['overall'],'passed_checks':len(checks),'http_requests':len(requests),'output':args.output}),flush=True)
if error:raise SystemExit(1)
