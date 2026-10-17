#!/usr/bin/env python3
"""Copy an immutable scaffold into a new learner directory; never overwrite."""
import argparse,shutil
from pathlib import Path
root=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('project');p.add_argument('--name');a=p.parse_args()
choices=[x for x in (root/'projects/scaffolds').iterdir() if x.is_dir() and x.name.startswith(a.project)]
if len(choices)!=1:raise SystemExit('Choose exactly one scaffold: '+', '.join(x.name for x in (root/'projects/scaffolds').iterdir()))
name=a.name or choices[0].name
if not name or Path(name).name!=name or name in ('.','..'):raise SystemExit('Name must be a simple directory name')
dest=root/'learner-work'/name
if dest.exists():raise SystemExit(f'Refusing to overwrite {dest}. Choose --name another-name.')
shutil.copytree(choices[0],dest,ignore=shutil.ignore_patterns('target','.git'))
print(f'Created {dest}. The reference and original scaffold remain unchanged.')
