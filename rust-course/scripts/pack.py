#!/usr/bin/env python3
"""Package authored course content only, never generated targets or learner data."""
from pathlib import Path
import os,tempfile,zipfile
r=Path(__file__).resolve().parents[1];out=r.parent/'rust-course-review.zip'
excluded={'.local','target','pkg','dist','learner-work','.git','node_modules','__pycache__'}
files=[]
for p in sorted(r.rglob('*')):
 rel=p.relative_to(r)
 if any(x in excluded for x in rel.parts):continue
 if p.name=='.env' or p.name.startswith('.env.') or p.suffix in ('.db','.sqlite','.sqlite3','.pyc') or p.name.startswith('workbench-events'):continue
 if p.is_symlink():raise SystemExit(f'Refusing source symlink: {rel}')
 if p.is_file():files.append((p,rel))
# Complete the new archive before replacing the previous review draft.
fd,name=tempfile.mkstemp(prefix='.rust-course-package-',suffix='.zip',dir=out.parent)
os.close(fd);temporary=Path(name)
try:
 with zipfile.ZipFile(temporary,'w',zipfile.ZIP_DEFLATED) as z:
  for p,rel in files:z.write(p,Path('rust-course')/rel)
 temporary.replace(out)
finally:
 temporary.unlink(missing_ok=True)
print(f'{out} ({out.stat().st_size:,} bytes); caches, generated UI, databases and learner work excluded.')
