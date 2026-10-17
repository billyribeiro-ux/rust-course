#!/usr/bin/env python3
"""Static content/link/source checks. This is not a browser-rendering test."""
from pathlib import Path
from html.parser import HTMLParser
from urllib.parse import urlsplit,unquote
import json,hashlib,re,sys
r=Path(__file__).resolve().parents[1]
class Page(HTMLParser):
 def __init__(self):super().__init__(convert_charrefs=True);self.links=[];self.ids=set();self.sources=[];self.source=None;self.in_code=False;self.code=[];self.duplicates=[]
 def handle_starttag(self,tag,attrs):
  a=dict(attrs)
  if 'id' in a:
   if a['id'] in self.ids:self.duplicates.append(a['id'])
   self.ids.add(a['id'])
  if tag in ('a','link','script','img'):
   value=a.get('href') or a.get('src')
   if value:self.links.append(value)
  if 'data-source' in a:self.source=a['data-source']
  if tag=='code' and self.source:self.in_code=True;self.code=[]
 def handle_data(self,data):
  if self.in_code:self.code.append(data)
 def handle_endtag(self,tag):
  if tag=='code' and self.in_code:self.sources.append((self.source,''.join(self.code)));self.source=None;self.in_code=False
pages={}
for f in r.rglob('*.html'):
 if any(part in ('authoring','.local','learner-work','target','pkg','node_modules','dist') for part in f.relative_to(r).parts):continue
 p=Page();p.feed(f.read_text());pages[f.resolve()]=p
issues=[];checked=0
for f,p in pages.items():
 for duplicate in p.duplicates:issues.append({'file':str(f.relative_to(r)),'issue':'duplicate id','target':duplicate})
 for link in p.links:
  parts=urlsplit(link)
  if parts.scheme or parts.netloc:continue
  dest=(f.parent/unquote(parts.path)).resolve() if parts.path else f
  if not dest.is_relative_to(r):issues.append({'file':str(f.relative_to(r)),'issue':'link escapes portable folder','target':link});continue
  if dest.is_dir():dest=dest/'index.html'
  checked+=1
  if not dest.exists():issues.append({'file':str(f.relative_to(r)),'issue':'missing local target','target':link})
  elif parts.fragment and dest in pages and unquote(parts.fragment) not in pages[dest].ids:issues.append({'file':str(f.relative_to(r)),'issue':'missing anchor','target':link})
 for source,text in p.sources:
  expected=(r/source).read_text()
  if text!=expected:issues.append({'file':str(f.relative_to(r)),'issue':'displayed source differs','target':source})
lessons=[]
for f in (r/'authoring').glob('*.json'):
 data=json.loads(f.read_text())
 if isinstance(data,list):lessons.extend(x for x in data if isinstance(x,dict) and 'body'in x and 'stage'in x)
for x in lessons:
 text=re.sub('<[^>]*>',' ',x['body']);words=len(text.split())
 if words<650:issues.append({'file':x['id'],'issue':'lesson below depth review threshold','words':words})
 for term in ('playbook','stuck'):
  if term.lower() not in x['body'].lower():issues.append({'file':x['id'],'issue':'missing teaching element','element':term})
report={'kind':'static checks only; not rendered browser QA','pages':len(pages),'local_links_checked':checked,'source_inclusions_checked':sum(len(p.sources) for p in pages.values()),'lessons_checked':len(lessons),'issues':issues}
(r/'verification/site-static.json').write_text(json.dumps(report,indent=2));print(json.dumps({**report,'issues':issues[:25],'total_issues':len(issues)},indent=2));sys.exit(bool(issues))
