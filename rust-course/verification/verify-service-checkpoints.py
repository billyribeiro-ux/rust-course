#!/usr/bin/env python3
"""Reconstruct all service milestones in temporary directories and test source copies."""
import argparse,datetime,hashlib,json,pathlib,shlex,subprocess,tempfile
ROOT=pathlib.Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--target-dir',type=pathlib.Path,required=True);a=p.parse_args()
projects=['p09-chat','p10-issues','p14-scheduler','p15-replication','p16-workbench'];results=[]
with (ROOT/'verification/services-checkpoints.log').open('w') as log:
 def run(command):
  log.write('$ '+shlex.join(map(str,command))+'\n');log.flush()
  result=subprocess.run(list(map(str,command)),cwd=ROOT,stdout=log,stderr=subprocess.STDOUT)
  log.write(f'exit={result.returncode}\n');log.flush();return result.returncode
 with tempfile.TemporaryDirectory(prefix='rust-course-checkpoints-') as tmp:
  for project in projects:
   manifest=json.loads((ROOT/'projects/checkpoints'/project/'checkpoint.json').read_text())
   for checkpoint in manifest['checkpoints']:
    destination=pathlib.Path(tmp)/project/checkpoint['id']
    reconstruct=['python3',ROOT/'scripts/start-checkpoint.py',project,checkpoint['id'],'--destination',destination]
    status=run(reconstruct);test=['mise','exec','--','cargo','test','--offline','--locked','--manifest-path',destination/'Cargo.toml','--target-dir',a.target_dir]
    if status==0:status=run(test)
    results.append({'project':project,'checkpoint':checkpoint['id'],'result':'passed'if status==0 else'failed','reconstruct':list(map(str,reconstruct)),'test':list(map(str,test))})
    if status:break
   if status:break
  # Refusing an existing destination is part of preserving learner work.
  marker=pathlib.Path(tmp)/'existing';marker.mkdir();(marker/'sentinel.txt').write_text('preserve me')
  rejected=run(['python3',ROOT/'scripts/start-checkpoint.py','p09','01-framing','--destination',marker])
  preserved=(marker/'sentinel.txt').read_text()=='preserve me' and sorted(x.name for x in marker.iterdir())==['sentinel.txt']
  results.append({'check':'existing destination refused and preserved','result':'passed'if rejected!=0 and preserved else'failed'})
result={'date':datetime.datetime.now(datetime.timezone.utc).isoformat(),'checks':results,'overall':'passed'if all(r['result']=='passed'for r in results)else'failed','author_review':'complete','independent_review':'pending','limits':['Final PostgreSQL integration tests are explicitly ignored by ordinary cargo test; previously recorded isolated database evidence remains separate.','Only complete source is copied; all builds use the caller-selected shared target directory.','Temporary reconstructed source copies are removed only after their cargo processes finish; immutable course sources and learner directories remain unchanged.']}
(ROOT/'verification/services-checkpoints.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({'overall':result['overall'],'checks':len(results)}));raise SystemExit(0 if result['overall']=='passed' else 1)
