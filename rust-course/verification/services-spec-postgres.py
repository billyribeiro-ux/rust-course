#!/usr/bin/env python3
"""Owned Unix-socket-only PostgreSQL fixture; never contacts an existing database."""
import argparse,json,os,pathlib,shutil,subprocess,tempfile,time
parser=argparse.ArgumentParser();parser.add_argument('--phase',choices=['before','after'],required=True);args=parser.parse_args()
course=pathlib.Path(__file__).resolve().parents[1];outer=course.parent
fixture=pathlib.Path(tempfile.mkdtemp(prefix='course-svc-',dir='/tmp'))
(fixture/'socket').mkdir()
log=course/'verification'/f'services-spec-postgres-{args.phase}.log'
report={'phase':args.phase,'fixture':str(fixture),'unix_socket_only':True,'existing_database_touched':False,'commands':[]}
started=False
with log.open('w') as out:
 def run(cmd,env=None,check=True,cwd=outer):
  out.write('$ '+subprocess.list2cmdline(list(map(str,cmd)))+'\n');out.flush()
  result=subprocess.run(cmd,stdout=out,stderr=subprocess.STDOUT,text=True,env=env,cwd=cwd)
  report['commands'].append({'argv':list(map(str,cmd)),'exit_code':result.returncode})
  if check and result.returncode:raise RuntimeError(f'command failed: {cmd[0]} ({result.returncode})')
  return result.returncode
 try:
  run(['initdb','-D',str(fixture/'data'),'--auth=trust','--no-locale','-E','UTF8'])
  run(['pg_ctl','-D',str(fixture/'data'),'-l',str(fixture/'server.log'),'-o',f'-c listen_addresses= -c unix_socket_directories={fixture}/socket -p 65432','-w','start']);started=True
  run(['createdb','-h',str(fixture/'socket'),'-p','65432','rust_course_test_scheduler'])
  env=os.environ.copy();env['COURSE_SCHEDULER_TEST_DATABASE_URL']=f'postgresql:///rust_course_test_scheduler?host={fixture}/socket&port=65432&user={os.environ["USER"]}'
  cmd=['mise','exec','--','cargo','test','--config','build.build-dir='+json.dumps(str(outer/'.course-cache/native')),'--offline','--locked','--manifest-path',str(course/'projects/Cargo.toml'),'--target-dir',str(outer/'.course-cache/native'),'-p','p14-scheduler','--test','postgres']
  if args.phase=='before':cmd += ['lease_fields_cannot_rewrite_durable_payload_or_retry_count']
  cmd += ['--','--ignored','--nocapture']
  report['test_exit_code']=run(cmd,env,check=False)
 finally:
  if started:report['stop_exit_code']=run(['pg_ctl','-D',str(fixture/'data'),'-m','fast','-w','stop'],check=False)
  if not started or report.get('stop_exit_code')==0:
   shutil.rmtree(fixture);report['owned_fixture_removed']=True
  else:report['owned_fixture_removed']=False
  (course/'verification'/f'services-spec-postgres-{args.phase}.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report))
raise SystemExit(report.get('test_exit_code',1))
