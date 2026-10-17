#!/usr/bin/env python3
"""Check materializer preservation and actual course path-dependency shapes."""
import argparse,importlib.util,json,pathlib,subprocess,tempfile,tomllib,hashlib,shlex
ROOT=pathlib.Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--target-dir',type=pathlib.Path,required=True);args=p.parse_args()
spec=importlib.util.spec_from_file_location('checkpoint_helper',ROOT/'scripts/start-checkpoint.py');helper=importlib.util.module_from_spec(spec);spec.loader.exec_module(helper)
checks=[]
with (ROOT/'verification/checkpoint-helper-audit.log').open('w')as log, tempfile.TemporaryDirectory(prefix='course-helper-audit-')as tmp:
 def run(command,expect=0,metadata=False):
  log.write('$ '+shlex.join(map(str,command))+'\n');log.flush()
  r=subprocess.run(list(map(str,command)),cwd=ROOT,capture_output=True,text=True)
  if metadata and r.returncode==0:
   data=json.loads(r.stdout);log.write('metadata summary '+json.dumps({'package_count':len(data['packages']),'workspace_members':data['workspace_members'],'stdout_sha256':hashlib.sha256(r.stdout.encode()).hexdigest()})+'\n')
  else:log.write(r.stdout)
  log.write(r.stderr+'\nexit='+str(r.returncode)+'\n');log.flush();assert r.returncode==expect,r.stderr
  return r
 def passed(name):checks.append({'check':name,'result':'passed'})
 # No dependency source or learner content is mutated by these copies.
 for project in ['p17-specializations','p19-concurrency-labs']:
  dest=pathlib.Path(tmp)/project;helper.materialize(ROOT/'projects/reference'/project,dest,dest,{})
  (dest/'Cargo.lock').write_bytes((ROOT/'projects/Cargo.lock').read_bytes())
  m=tomllib.loads((dest/'Cargo.toml').read_text());assert m['package']['edition']=='2024';assert m['package']['rust-version']=='1.99'
  if project=='p17-specializations':
   child=dest/'_deps/course-kind-derive/Cargo.toml';cm=tomllib.loads(child.read_text());assert 'workspace' not in cm;assert cm['package']['edition']=='2024';assert m['dependencies']['course-kind-derive']['path']=='_deps/course-kind-derive'
  else:
   assert 'test-util'in m['dev-dependencies']['tokio']['features'];assert 'net'in m['dev-dependencies']['tokio']['features'];assert m['dev-dependencies']['tokio']['version']=='=1.53.2'
  run(['mise','exec','--','cargo','metadata','--offline','--format-version','1','--manifest-path',dest/'Cargo.toml'],metadata=True)
  run(['mise','exec','--','cargo','metadata','--offline','--locked','--format-version','1','--manifest-path',dest/'Cargo.toml'],metadata=True)
  passed(project+' path dependency / inherited fields and merged features resolve')
 for project,id in [('p09','01-framing'),('p10','01-http-validation'),('p14','01-lease-model'),('p15','01-append-transport'),('p16','01-order-domain')]:
  dest=pathlib.Path(tmp)/project;run(['python3',ROOT/'scripts/start-checkpoint.py',project,id,'--destination',dest]);run(['mise','exec','--','cargo','clippy','--offline','--locked','--all-targets','--manifest-path',dest/'Cargo.toml','--target-dir',args.target_dir,'--','-D','warnings']);passed(project+' reconstructed early milestone strict Clippy')
 existing=pathlib.Path(tmp)/'existing';existing.mkdir();(existing/'mine.txt').write_text('keep')
 run(['python3',ROOT/'scripts/start-checkpoint.py','p09','01-framing','--destination',existing],expect=1);assert (existing/'mine.txt').read_text()=='keep';assert len(list(existing.iterdir()))==1;passed('existing destination refused without changes')
 nested=ROOT/'projects/checkpoints/p09-chat/01-framing/.audit-destination';assert not nested.exists();run(['python3',ROOT/'scripts/start-checkpoint.py','p09','01-framing','--destination',nested],expect=1);assert not nested.exists();passed('destination nested in source rejected before creating it')
 try:helper.inside(pathlib.Path(tmp));raise AssertionError('outside source accepted')
 except ValueError:passed('source outside course rejected')
 result={'overall':'passed','checks':checks,'helper_sha256':hashlib.sha256((ROOT/'scripts/start-checkpoint.py').read_bytes()).hexdigest(),'author_review':'complete','independent_review':'pending','limits':['p11 actual WASM build is verified separately by root.','Path dependency tests resolve existing course shapes, not arbitrary Cargo workspace layouts.','No browser interaction is part of these checks.']}
 (ROOT/'verification/checkpoint-helper-audit.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({'overall':'passed','checks':len(checks)}))
