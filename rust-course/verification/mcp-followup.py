#!/usr/bin/env python3
"""Bounded stdio MCP diagnostic; no configuration changes."""
import json, os, queue, subprocess, threading, time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
OUTER=ROOT.parent
MANIFEST=ROOT/'projects/Cargo.toml'
BIN='/Users/billyribeiro/.cargo/bin/rust-mcp-server'
CONFIGURED_SERVER_PATH='/Users/billyribeiro/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin'
records=[]
class Client:
 def __init__(self, command, cwd, label):
  self.record={'label':label,'command':command,'cwd':str(cwd),'requests':[],'stderr':[]}
  records.append(self.record)
  env=os.environ.copy();env['PATH']=CONFIGURED_SERVER_PATH
  self.process=subprocess.Popen(command,cwd=cwd,env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,bufsize=1)
  self.incoming=queue.Queue();self.counter=0
  threading.Thread(target=self.lines,args=(self.process.stdout,self.incoming),daemon=True).start()
  threading.Thread(target=self.errors,daemon=True).start()
 def lines(self,stream,q):
  for line in stream:q.put(line)
  q.put(None)
 def errors(self):
  for line in self.process.stderr:self.record['stderr'].append(line.rstrip())
 def send(self,obj):
  self.process.stdin.write(json.dumps(obj)+'\n');self.process.stdin.flush()
 def request(self,method,params,timeout=45):
  self.counter+=1;ident=self.counter
  request={'jsonrpc':'2.0','id':ident,'method':method,'params':params}
  self.send(request);start=time.monotonic()
  while True:
   line=self.incoming.get(timeout=max(.01,timeout-(time.monotonic()-start)))
   if line is None:raise RuntimeError('Server closed stdout')
   message=json.loads(line)
   if message.get('method')=='roots/list' and 'id' in message:
    self.send({'jsonrpc':'2.0','id':message['id'],'result':{'roots':[{'uri':OUTER.as_uri(),'name':'rust-course'}]}});continue
   if message.get('id')==ident:
    self.record['requests'].append({'method':method,'params':params,'response':message,'elapsed_seconds':round(time.monotonic()-start,3)})
    return message
 def close(self):
  self.process.stdin.close()
  try:self.process.wait(timeout=3)
  except subprocess.TimeoutExpired:self.process.terminate();self.process.wait(timeout=3)
  self.record['exit_code']=self.process.returncode

def run(command,cwd,label,do_check=False):
 client=Client(command,cwd,label)
 try:
  client.request('initialize',{'protocolVersion':'2025-03-26','capabilities':{'roots':{'listChanged':True}},'clientInfo':{'name':'course-mcp-diagnostic','version':'1.0'}})
  client.send({'jsonrpc':'2.0','method':'notifications/initialized'})
  listed=client.request('tools/list',{})
  tools=listed.get('result',{}).get('tools',[])
  client.record['tool_names']=[t['name']for t in tools]
  # Keep the schema summary, not thousands of lines of tool documentation.
  client.record['requests'][-1]['response']={'id':listed.get('id'),'tool_count':len(tools)}
  info=client.request('tools/call',{'name':'workspace-info','arguments':{'manifest_path':str(MANIFEST),'toolchain':'1.99.0','include_dependencies':False}})
  if info.get('result',{}).get('isError') is not False:raise RuntimeError('Workspace access not established; no build/check attempted')
  metadata=client.request('tools/call',{'name':'cargo-metadata','arguments':{'manifest_path':str(MANIFEST),'toolchain':'1.99.0','no_deps':True,'locking_mode':'frozen'}})
  if metadata.get('result',{}).get('isError') is not False:raise RuntimeError('Metadata unsuccessful; no build/check attempted')
  if do_check:
   checked=client.request('tools/call',{'name':'cargo-check','arguments':{'manifest_path':str(MANIFEST),'toolchain':'1.99.0','package':['core-rust-labs'],'lib':True,'locking_mode':'frozen','target_dir':str(OUTER/'.course-cache/native')}})
   if checked.get('result',{}).get('isError') is not False:raise RuntimeError('Targeted check unsuccessful')
 except Exception as e:
  client.record['diagnostic_error']=repr(e)
 finally:client.close()

run([BIN],OUTER,'configured stdio command and PATH; explicit tool manifest')
run([BIN,'--workspace',str(ROOT/'projects')],ROOT/'projects','explicit workspace; pinned toolchain; targeted check',True)
result={'date':'2026-10-09','kind':'Rust MCP direct JSON-RPC follow-up','server_binary':BIN,'manifest':str(MANIFEST),'direct_stdio_runs':records,'session_wrapper':{'workspace_info':'Unexpected response type','cargo_metadata':'Unexpected response type','workspace_access_established':False},'configuration_changed':False,'permissions_changed':False,'official_configuration_source':'https://learn.chatgpt.com/docs/extend/mcp?surface=cli'}
(ROOT/'verification/mcp-followup.json').write_text(json.dumps(result,indent=2)+'\n')
for record in records:
 print(record['label'],record.get('diagnostic_error','success'))
 for request in record['requests']:
  response=request['response'];r=response.get('result',{})
  print(' ',request['method'],request['params'].get('name',''), 'isError=',r.get('isError'),'keys=',list(response))
