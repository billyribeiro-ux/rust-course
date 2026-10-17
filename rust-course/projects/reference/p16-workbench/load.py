#!/usr/bin/env python3
"""Reproducible serial baseline against a new, isolated child server and journal.
No other service/database is contacted or modified. Results are measurements, not capacity guarantees.
"""
import argparse,json,os,pathlib,platform,signal,subprocess,tempfile,time,urllib.request
parser=argparse.ArgumentParser();parser.add_argument('--binary',required=True);parser.add_argument('--orders',type=int,default=200);parser.add_argument('--output',required=True);args=parser.parse_args()
if not 1<=args.orders<=5000: parser.error('orders must be1..5000')
latencies=[]
with tempfile.TemporaryDirectory(prefix='rust-course-load-') as directory:
 journal=pathlib.Path(directory)/'events.jsonl'
 env=dict(os.environ,WORKBENCH_LOG=str(journal),WORKBENCH_ADDR='127.0.0.1:0')
 proc=subprocess.Popen([str(pathlib.Path(args.binary).resolve())],env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
 try:
  line=proc.stdout.readline().strip()
  if not line.startswith('LISTEN_ADDR=127.0.0.1:'): raise RuntimeError('server did not announce isolated loopback address: '+line)
  base='http://'+line.split('=',1)[1]
  def request(path,body=None):
   data=None if body is None else json.dumps(body).encode();headers={'Authorization':'Bearer demo-operator','Content-Type':'application/json'}
   start=time.perf_counter()
   with urllib.request.urlopen(urllib.request.Request(base+path,data=data,headers=headers),timeout=10) as response:
    payload=json.load(response)
   return payload,(time.perf_counter()-start)*1000
  request('/api/ticks',dict(event_id='baseline-tick',symbol='RUST',price_cents=100,sequence=1))
  begin=time.perf_counter();duplicate_attempts=0
  for n in range(args.orders):
   order=dict(order_id=f'baseline-{n:05d}',symbol='RUST',side='buy',quantity=1)
   result,ms=request('/api/orders',order);assert result['applied'] is True;latencies.append(ms)
   if n%10==0:
    result,_=request('/api/orders',order);assert result['applied'] is False;duplicate_attempts+=1
  elapsed=time.perf_counter()-begin;snapshot,_=request('/api/snapshot');assert snapshot['cash_cents']==1_000_000-100*args.orders;assert snapshot['positions'][0]['quantity']==args.orders
  ordered=sorted(latencies)
  result={'kind':'serial local HTTP+sync_data baseline','platform':platform.platform(),'machine':platform.machine(),'orders':args.orders,'duplicate_attempts':duplicate_attempts,'elapsed_seconds':elapsed,'orders_per_second':args.orders/elapsed,'latency_ms':{'min':ordered[0],'median':ordered[len(ordered)//2],'p95':ordered[min(len(ordered)-1,int(len(ordered)*.95))],'max':ordered[-1]},'journal_bytes':journal.stat().st_size,'verified_cash_cents':snapshot['cash_cents'],'verified_holdings':snapshot['positions'][0]['quantity'],'limits':['one process, one host, serial workload','no warm-up exclusion; debug binary unless caller records another profile','no claim of maximum capacity, production SLO, power-loss durability or realistic trading behavior']}
 finally:
  if proc.poll() is None:
   proc.send_signal(signal.SIGINT)
   try:proc.wait(timeout=5)
   except subprocess.TimeoutExpired:proc.kill();proc.wait();raise RuntimeError('server failed graceful shutdown')
  stderr=proc.stderr.read()
  if stderr:print(stderr)
pathlib.Path(args.output).write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
