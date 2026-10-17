#!/usr/bin/env python3
"""Reproducible baseline checks with command, environment, status and full logs."""
import argparse,json,os,subprocess,time,datetime,sys
from pathlib import Path
r=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--target-dir',default=str(r/'.local/native-target'));p.add_argument('--skip-build',action='store_true');a=p.parse_args()
v=r/'verification';v.mkdir(exist_ok=True)
# Cargo 1.99 permits a separate global intermediate build directory. Keep this
# helper's explicitly selected verification cache self-contained without changing
# the learner's global Cargo configuration or debug/incremental profile.
check_env={**os.environ,'CARGO_BUILD_BUILD_DIR':str(Path(a.target_dir).resolve())}
commands=[('toolchain',['mise','exec','--','rustc','-Vv']),('mise',['mise','--version']),('format',['mise','exec','--','cargo','fmt','--manifest-path','projects/Cargo.toml','--all','--check'])]
if not a.skip_build:
 for name,args in [('tests',['test','--workspace']),('check',['check','--workspace','--all-targets']),('clippy',['clippy','--workspace','--all-targets']),('docs',['doc','--workspace','--no-deps']),('tree',['tree','--workspace','--depth','2'])]:
  cmd=['mise','exec','--','cargo',*args,'--locked','--manifest-path','projects/Cargo.toml']
  if name!='tree':cmd+=['--target-dir',a.target_dir]
  if name=='clippy':cmd+=['--','-D','warnings']
  commands.append((name,cmd))
node=['mise','exec','node@26.12.0','--','node']
commands += [('node',node+['--version']),('javascript',node+['--check','assets/course.js']),('progress-data-syntax',node+['--check','assets/progress-data.js']),('progress-data-tests',node+['--test','verification/progress-data.test.cjs']),('site',['python3','scripts/verify-site.py'])]
records=[]
for name,cmd in commands:
 begin=time.monotonic();print('RUN',name,flush=True)
 try:result=subprocess.run(cmd,cwd=r,env=check_env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,timeout=600);code=result.returncode;output=result.stdout
 except subprocess.TimeoutExpired as e:code=124;output='Timed out after 600 seconds\n'+str(e.stdout)
 (v/f'check-{name}.log').write_text('$ '+' '.join(cmd)+'\nworking_directory='+str(r)+'\nCARGO_BUILD_BUILD_DIR='+check_env['CARGO_BUILD_BUILD_DIR']+'\n'+output)
 records.append({'name':name,'command':cmd,'working_directory':str(r),'exit_code':code,'seconds':round(time.monotonic()-begin,3),'log':f'check-{name}.log','passed':code==0})
 (v/'checks.json').write_text(json.dumps({'date':datetime.datetime.now(datetime.timezone.utc).isoformat(),'target_dir':a.target_dir,'build_dir':check_env['CARGO_BUILD_BUILD_DIR'],'checks':records},indent=2));print(name,code,flush=True)
print('Baseline finished; database, WASM, Miri and browser checks have separate evidence records.')
sys.exit(any(x['exit_code']for x in records))
