#!/usr/bin/env python3
"""Deterministically render authored lessons; source snippets come from real files."""
from pathlib import Path
import json,re,html,hashlib
ROOT=Path(__file__).resolve().parents[1]
E=html.escape
stages=['Tool foundations','Ownership and core Rust','Libraries and language design','Testing and durable storage','Concurrency and async','Networks and databases','Rust in the browser','Performance and unsafe Rust','Distributed behavior','The independent capstone']
projects=['Workshop Calculator','Local Task Journal','Log Detective','Typed Configuration Library','Expression Language','Durable Key–Value Store','Bounded File Indexer','Async Service Health Monitor','Protocol Chat Server','Team Issue Tracker API','Rust Operations Console','Deterministic Market Replay Engine','Safe API Around Unsafe Code','Durable Job Scheduler','Replicated State Lab','Research and Paper-Trading Workbench']
lessons=[]
for f in sorted((ROOT/'authoring').glob('*.json')):
    if f.name in ('assessments.json','project-status.json'):continue
    data=json.loads(f.read_text())
    if isinstance(data,list):lessons.extend(x for x in data if 'body' in x and 'stage' in x)
lessons.sort(key=lambda x:(x['stage'],x.get('order',999),x['id']))
# Logical sequencing, independent of lexicographic filenames.
sequence='00-terminal 00-workspace 00-programs 00-values 00-calculator 00-conversions 01-ownership 01-modeling 01-collections 01-journal 01-io 01-logs 02-traits 02-config 02-conversions 02-lifetimes 02-parser 02-evaluator 03-tests 03-fuzz 03-raii 03-interior 03-log-store 03-recovery 04-threads 04-atomics 04-indexer 04-parallel-tools 04-futures 04-pinning 04-monitor 05-protocols 05-chat 05-http 05-sql 05-tracker 05-web-boundaries 06-browser 06-state 06-console 06-boundaries 07-layout 07-allocation-dispatch 07-performance 07-replay 07-unsafe 07-auditing 07-traits 07-macros 07-ffi 08-delivery 08-scheduler 08-replication 08-operations 09-domain 09-workbench 09-review'.split()
lessons.sort(key=lambda x:sequence.index(x['id']) if x['id'] in sequence else 100+x['stage'])
byid={x['id']:x for x in lessons}; snippets=[]
glossary=json.loads((ROOT/'assets/glossary.json').read_text()) if (ROOT/'assets/glossary.json').exists() else {}
index=[{'title':x['title'],'summary':x['summary'],'href':f"lessons/stage-{x['stage']}/{x['id']}.html",'type':'Lesson','keywords':' '.join(x.get('objectives',[]))} for x in lessons]
def href(x):return f"lessons/stage-{x['stage']}/{x['id']}.html"
def slug(s):return re.sub(r'[^a-z0-9]+','-',re.sub('<[^>]+>','',s).lower()).strip('-')
def render_body(body,prefix):
    def source(m):
        rel=m.group(1);p=(ROOT/rel).resolve()
        if not p.is_relative_to(ROOT):raise ValueError(rel)
        content=p.read_text();digest=hashlib.sha256(content.encode()).hexdigest();snippets.append({'source':rel,'sha256':digest})
        return f'<div class="code-wrap" data-source="{E(rel)}" data-sha256="{digest}"><div class="code-label"><a href="{prefix}{E(rel)}">{E(rel)}</a></div><pre><code>{E(content)}</code></pre></div>'
    body=re.sub(r'<div data-code="([^"]+)"\s*>\s*</div>',source,body)
    body=body.replace('href="@root/','href="'+prefix)
    body=re.sub(r'<h2>(.*?)</h2>',lambda m:f'<h2 id="{slug(m.group(1))}">{m.group(1)}</h2>',body,flags=re.S)
    return body

def shell(title,body,rel='index.html',lesson=None):
    depth=len(Path(rel).parts)-1;pre='../'*depth or './'
    nav=''.join(f'<a class="nav-link" href="{pre}index.html#stage-{i}"><span class="stage-number">{i:02}</span>{name}</a>' for i,name in enumerate(stages))
    toc=''
    if lesson:
        toc='<aside class="toc" aria-label="On this page"><strong>In this lesson</strong>'+''.join(f'<a href="#{slug(t)}">{re.sub("<[^>]+>","",t)}</a>' for t in re.findall(r'<h2[^>]*>(.*?)</h2>',body,re.S))+'</aside>'
        body='<div class="lesson-grid"><article class="lesson">'+body+'</article>'+toc+'</div>'
    attrs=f'data-root="{pre}" data-page="{E(Path(rel).stem)}"'+(f' data-lesson="{lesson["id"]}"' if lesson else '')
    return f'''<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="description" content="A local, project-driven Rust workshop with runnable sources and evidence."><title>{E(title)} · Rust Workshop</title><link rel="stylesheet" href="{pre}assets/course.css"><script defer src="{pre}assets/search-index.js"></script><script defer src="{pre}assets/progress-data.js"></script><script defer src="{pre}assets/course.js"></script></head><body {attrs}><a class="skip" href="#main">Skip to content</a><aside class="sidebar" id="course-navigation"><a class="brand" href="{pre}index.html"><span class="brand-mark">Rs</span>Rust Workshop</a><p class="side-caption">Read. Predict. Build. Investigate.</p><a class="nav-link" href="{pre}index.html">Learning map</a><a class="nav-link" href="{pre}playbook.html">Problem-solving playbook</a><a class="nav-link" href="{pre}projects.html">Project workbench</a><div class="side-label">Your learning path</div>{nav}<div class="side-label">Reference & evidence</div><a class="nav-link" href="{pre}assessments/index.html">Independent assessments</a><a class="nav-link" href="{pre}glossary.html">Glossary</a><a class="nav-link" href="{pre}sources.html">Sources & versions</a><a class="nav-link" href="{pre}review.html">Review checklist</a><a class="nav-link" href="{pre}verification/index.html">Verification evidence</a><a class="nav-link" href="{pre}coverage.html">Coverage matrix</a><div class="side-label">Your progress</div><p class="side-caption" data-progress-text></p><div class="progress-track"><div class="progress-fill"></div></div><p class="side-caption">Self-report, not proof of mastery.</p></aside><header class="topbar"><button id="menu" class="menu" aria-label="Toggle navigation" aria-expanded="false" aria-controls="course-navigation">Menu</button><div class="search-wrap"><label class="visually-hidden" for="search">Search lessons and projects</label><input id="search" type="search" placeholder="Search concepts, lessons, projects…" autocomplete="off"><div id="search-results" class="search-results" aria-live="polite"></div></div><div class="topbar-actions"><button id="theme">Dark theme</button><button class="desktop-only" id="export">Export</button><button class="desktop-only" id="import-open" type="button">Import<span class="visually-hidden"> progress JSON</span></button><input hidden id="import" type="file" accept="application/json" aria-label="Choose progress JSON"></div></header><main class="layout" id="main"><div class="breadcrumbs"><a href="{pre}index.html">Workshop</a> / {E(title)}</div>{body}<p id="global-status" class="status" role="status"></p><footer>Rust Workshop · Local review draft · Independent review pending. <a href="{pre}playbook.html">Investigate with evidence</a> · <a href="{pre}review.html">Review this course</a>. Progress and notes stay in this browser unless you export them.</footer></main></body></html>'''

def write(rel,title,body,lesson=None):
    pre='../'*(len(Path(rel).parts)-1) or './';p=ROOT/rel;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(shell(title,render_body(body,pre),rel,lesson))

for i,x in enumerate(lessons):
    prereq=''.join(f'<li><a href="../../{href(byid[p])}">{E(byid[p]["title"])}</a></li>' if p in byid else f'<li>{E(p)}</li>' for p in x.get('prerequisites',[]))
    sources=''.join(f'<li><a href="{E(s["url"])}">{E(s["title"])}</a></li>' for s in x.get('sources',[]))
    body=f'<div class="eyebrow">Stage {x["stage"]:02} / {stages[x["stage"]]}</div><h1>{E(x["title"])}</h1><p class="lead">{E(x["summary"])}</p><p class="meta">Read with a terminal open. Predict before revealing. Verified source versions: <a href="../../sources.html">version ledger</a>.</p>'
    body+=f'<details><summary>Prerequisites and learning objectives</summary><ul>{prereq}</ul><ul>'+''.join(f'<li>{E(o)}</li>' for o in x.get('objectives',[]))+'</ul></details>'
    terms=[t for t in glossary if re.search(r'\b'+re.escape(t)+r'\b',x['body'],re.I)][:8]
    body+='<p class="meta">Key terms: '+' · '.join(f'<a href="../../glossary.html#{t.lower()}">{E(t)}</a>'for t in terms)+'</p>'
    body+='<p class="callout">Keep the <a href="../../playbook.html">problem-solving playbook</a> open. Record observations before explanations, and save the experiment that changed your mind.</p>'
    if x.get('project') in {f'p{n:02}' for n in range(1,17)}:
        body+=f'<p class="meta"><a href="../../projects/checkpoints/index.html#{x["project"]}">Reconstruct this project’s runnable checkpoints</a>. Each milestone states its own scope; learner scaffolds are separate assignments.</p>'
    body+=x['body']
    widgets={'01-ownership':('ownership','Ownership and borrow trace'),'01-logs':('iterator','A lazy iterator trace'),'04-futures':('tasks','Polling and task ownership'),'04-indexer':('queue','A queue with capacity three'),'05-sql':('transaction','Two competing transactions'),'08-replication':('partition','A delayed replica')}
    if x['id'] in widgets:
        kind,title=widgets[x['id']]
        buttons='<button data-action="produce">Produce one</button><button data-action="consume">Consume one</button>' if kind=='queue' else '<button data-action="next">Next state</button>'
        body+=f'<h2>Explore the model</h2><section class="widget" data-widget="{kind}" aria-label="{title}"><h3>{title}</h3><p>Conceptual teaching model: this trace does not show physical memory placement, prove a scheduler property, or implement consensus. Read the state explanation after each action.</p><div class="widget-visual" aria-hidden="true"></div><p data-output aria-live="polite"></p>{buttons}<button data-action="reset">Reset</button><noscript>JavaScript is disabled. The lesson above contains the equivalent explanation; this optional state trace needs JavaScript.</noscript></section>'
    body+='<h2>Primary references</h2><ul>'+sources+'</ul>' 
    body+='<section class="notes"><h2>Your evidence and notes</h2><label class="progress-line"><input type="checkbox" id="complete">I have completed this lesson’s independent work</label><p class="meta">This is a self-report. Record evidence; checking the box does not demonstrate competence.</p><label for="lesson-notes">Prediction, experiment, result, remaining uncertainty</label><textarea id="lesson-notes" placeholder="Expected… Observed… Hypothesis… Experiment… Result… Next question…"></textarea></section><nav class="pager" aria-label="Lesson navigation">'
    if i:body+=f'<a href="../../{href(lessons[i-1])}">Previous: {E(lessons[i-1]["title"])}</a>'
    if i+1<len(lessons):body+=f'<a href="../../{href(lessons[i+1])}">Next: {E(lessons[i+1]["title"])}</a>'
    body+='</nav>';write(href(x),x['title'],body,x)

first=href(lessons[0]) if lessons else 'playbook.html'
body=f'''<div class="intro-grid"><div><div class="eyebrow">Rust · A practical engineering workshop</div><h1>Build understanding.<br>Keep the evidence.</h1><p class="lead">A path from your first terminal command to systems that survive concurrency, persistence, and partial failure.</p><div class="stats"><div><strong>{len(lessons)}</strong><span>authored lessons</span></div><div><strong>16</strong><span>project path</span></div><div><strong>10</strong><span>learning stages</span></div></div><p class="meta">Review draft · Rust 1.99.1 via mise · Toolchain checked October 17, 2026</p></div><div class="panel continue"><div class="eyebrow">Begin with a small investigation</div><h2>Your next step</h2><p>Make a prediction, run a program, and explain the difference. Start at the beginning or use the diagnostic below.</p><a class="button primary" href="{first}">Open the first lesson</a> <a class="button" href="playbook.html">Open the playbook</a><p class="meta" data-progress-text></p><div class="progress-track"><div class="progress-fill"></div></div></div></div>
<h2 id="start">Set up once with mise</h2><div class="two-col"><section class="panel"><h3>Read locally</h3><p>Check the <a href="README.html">launch guide</a> for mise, Python 3.11+, Git and native development-tool prerequisites. From the portable course folder, inspect <code>mise.toml</code> before trusting its version selection, environment and local tasks:</p><pre><code>mise trust
mise install
mise exec -- rustc --version
mise run course</code></pre><p>Open <a href="http://127.0.0.1:8000/">localhost:8000</a>. Python 3 serves static files only. You can also open <code>index.html</code> directly; search uses a local script, not a network fetch.</p></section><section class="panel"><h3>Build deliberately</h3><p>Reference implementations and learner scaffolds are separate. From the course root, copy a scaffold with <code>python3 scripts/start-project.py p01</code>; existing learner work is never overwritten. Follow the project workbench to test that learner copy. Lesson reference commands specify <code>projects/</code> as their Cargo working directory.</p><p><a href="projects.html">Open project commands</a> · <a href="verification/index.html">Inspect actual evidence</a></p><p>Keep one shared target directory. The optional small profile trades debug detail and incremental speed for less disk use.</p></section></div>
<h2 id="diagnostic">Choose your starting point</h2><p>This diagnostic checks a few prerequisites. A correct guess is not enough: explain it aloud and complete the practical task for any stage you skip.</p><section class="diagnostic">'''
qs=[('You run a command in the wrong folder. What should you inspect first?', ['Your current directory and the files present','Reinstall Rust','Delete Cargo.lock'],0,'Use pwd and list files. Installation changes do not correct a path. Practical task: create a folder, move into it, and explain a relative path.'),('What is the value of the expression 2 + 3 * 4?', ['20','14','24'],1,'Multiplication has higher precedence, so 3 * 4 is evaluated before adding 2. Practical task: write a function and test its boundary input.'),('After let b = a for a String, which binding owns the value?', ['Both independently','b; a cannot be used as an owner','Neither'],1,'The String moves. It is not a Copy type; cloning requires an explicit allocation/copy choice. Demonstrate a borrow that avoids needing a clone.'),('A passing unit test establishes…', ['All inputs are correct','The tested observation under those conditions','Production safety'],1,'Tests support specific claims. They can miss relevant inputs, system boundaries and flawed assertions. Write a counterexample to an overly broad claim.'),('A server completed an operation but its response was lost. Retrying may…', ['Execute the operation again','Always return exactly once','Roll back the first operation automatically'],0,'The client has an uncertain outcome. Stable operation identity and deduplication can help, but external effects require explicit reasoning. Draw the failure timeline.')]
for n,(q,opts,a,why) in enumerate(qs):
    body+=f'<fieldset data-answer="{a}"><legend>{n+1}. {q}</legend>'+''.join(f'<label><input type="radio" name="q{n}" value="{i}"> {o}</label>' for i,o in enumerate(opts))+f'<p class="answer callout" hidden>{why}</p></fieldset>'
body+='<button id="diagnostic-check">Reveal explanations</button><p id="diagnostic-result" role="status"></p></section><h2 id="map">The learning map</h2><p>Advance when you can explain, change, and debug the work without the reference. Repeated independent practice, real experience and external review are necessary for expertise; this course is not a credential or a guarantee.</p><div class="stage-grid">'
for i,name in enumerate(stages):
    subset=[x for x in lessons if x['stage']==i];links=''.join(f'<li><a href="{href(x)}">{E(x["title"])}</a></li>' for x in subset)
    body+=f'<section class="stage-card" id="stage-{i}"><span class="num">{i:02}</span><div><h3>{name}</h3><p>{len(subset)} authored lessons · <a href="assessments/index.html#gate-{i}">independent checkpoint</a></p><ul>{links}</ul></div></section>'
body+='</div><h2>Review the work, not the label</h2><p><a href="review.html">Review checklist</a>, <a href="coverage.html">coverage matrix</a>, and <a href="course-status.json">machine-readable status</a> distinguish generation, self-review, verification, independent acceptance and learner competence. Remaining gaps are explicitly listed in the evidence index.</p>'
write('index.html','Learning map',body)

# Assessment content is authored separately and wrapped without browser fetching.
a=ROOT/'authoring/assessments-fragment.html'
if a.exists():write('assessments/index.html','Independent assessments',a.read_text())
evidence=ROOT/'authoring/evidence-fragment.html'
if evidence.exists():write('verification/index.html','Verification evidence',evidence.read_text())
# Pages authored as HTML fragments; no client-side content fetching.
for f in (ROOT/'authoring').glob('*.html'):
    if f.name.endswith('-fragment.html'): continue
    title=re.search(r'<h1>(.*?)</h1>',f.read_text(),re.S);write(f.name,title.group(1) if title else f.stem,f.read_text())

body='''<div class="eyebrow">Source workbench</div><h1>Sixteen projects, one connected path</h1><p>Reference code is complete only to its documented contract. Scaffolds intentionally leave work for you. Make a learner copy before editing; the helper refuses existing destinations. Start both blocks below in the course root, the folder containing <code>index.html</code>, <code>mise.toml</code> and <code>scripts</code>.</p><h2>Copy and test your learner project</h2><pre><code>python3 scripts/start-project.py p01
mise exec -- cargo test --manifest-path learner-work/p01-calculator/Cargo.toml</code></pre><p>Open the new copy's README.html and edit its source. The first calculator scaffold deliberately has four failing acceptance tests until you implement its contract. A passing reference test does not establish that your copy works.</p><h2>Check the supplied references</h2><pre><code>mise exec -- cargo test --manifest-path projects/Cargo.toml --workspace --locked</code></pre><p>The native reference workspace shares one Cargo.lock. UI builds use a separate WASM target and lockfile. A default <code>target/</code> is rebuildable cache, never part of the portable course archive.</p>'''
project_entries=[]
for n,title in enumerate(projects,1):
    pid=f'p{n:02}';dirs=list((ROOT/'projects/reference').glob(pid+'-*'))
    if n==11:dirs=[ROOT/'projects/ui-console'] if (ROOT/'projects/ui-console').exists() else []
    entries=[x for x in lessons if x.get('project')==pid]
    path=dirs[0].relative_to(ROOT).as_posix() if dirs else ''
    source_files=sorted(dirs[0].rglob('*.rs')) if dirs else []
    body+=f'<section class="panel" id="{pid}"><div class="eyebrow">Project {n:02}</div><h2>{E(title)}</h2><ul>'+''.join(f'<li><a href="{href(x)}">{E(x["title"])}</a></li>' for x in entries)+'</ul>'
    if path:body+=f'<p><a href="{path}/Cargo.toml">Cargo manifest</a> · <a href="assessments/index.html#{pid}">Acceptance assignment</a></p><details><summary>Source files</summary><ul>'+''.join(f'<li><a href="{f.relative_to(ROOT).as_posix()}">{E(f.relative_to(dirs[0]).as_posix())}</a></li>' for f in source_files)+'</ul></details>'
    if path:
        for name,label in [('README.html','Run instructions and contract'),('decisions.html','Engineering decisions and evidence')]:
            if (ROOT/path/name).exists():body+=f'<p><a href="{path}/{name}">{label}</a></p>'
    checkpoint_dirs=list((ROOT/'projects/checkpoints').glob(pid+'-*'))
    if checkpoint_dirs and (checkpoint_dirs[0]/'checkpoint.json').exists():
        body+=f'<p><a href="projects/checkpoints/index.html#{pid}">Reconstruct runnable checkpoints</a> · intermediate scope, exact source and final reference</p>'
    body+='</section>'
    index.append({'title':title,'summary':f'Project {n:02} · runnable sources and assessment','type':'Project','href':f'projects.html#{pid}'})
    project_entries.append({'id':pid,'title':title,'source':path,'authored':bool(source_files),'author_self_review_complete':False,'code_verified':False,'independent_review':'pending'})
write('projects.html','Project workbench',body)

body='<div class="eyebrow">Small source snapshots · shared build cache</div><h1>Reconstruct a runnable checkpoint</h1><p>Scaffolds are independent assignments and may contain explicitly labeled unfinished work. These checkpoints are complete to the contract shown below. The helper creates a new learner directory and refuses to overwrite existing work. Earlier milestones have smaller requirements; final checkpoints use the immutable reference source and materialize their Cargo manifests and local helper dependencies.</p><pre><code>python3 scripts/start-checkpoint.py p01 01-checked-add\n# The helper prints the new directory and its exact commands.</code></pre><p>Run the helper from the course folder. Read each checkpoint’s supported inputs and engineering decisions before expanding it. All native learner builds inherit the course’s mise target directory unless you intentionally select another shared cache. UI builds use scripts/build-ui.py --project with a separate WASM cache. Generated build output never belongs in a source checkpoint.</p>'
for manifest in sorted((ROOT/'projects/checkpoints').glob('p*/checkpoint.json')):
    record=json.loads(manifest.read_text());pid=record['project'][:3]
    body+=f'<section id="{pid}"><h2>{E(record["project"])}</h2><p><a href="{manifest.relative_to(ROOT).as_posix().replace("projects/checkpoints/", "")}">Machine-readable checkpoint manifest</a></p>'
    for checkpoint in record['checkpoints']:
        source=ROOT/checkpoint['source']
        body+=f'<h3>{E(checkpoint["id"])} · {E(checkpoint["title"])}</h3><p>{E(checkpoint["contract"])}</p><pre><code>python3 scripts/start-checkpoint.py {pid} {E(checkpoint["id"])}</code></pre>'
        for name in ('README.html','instructions.html','decisions.html','Cargo.toml'):
            if (source/name).exists():body+=f'<p><a href="../../{(source/name).relative_to(ROOT).as_posix()}">{E(name)}: source contract and implementation</a></p>'
        if checkpoint.get('commands'):body+='<details><summary>Checkpoint commands (use the working directory named in its instructions)</summary><pre><code>'+E('\n'.join(checkpoint['commands']))+'</code></pre></details>'
    body+='</section>'
write('projects/checkpoints/index.html','Runnable checkpoints',body)

sources={'https://mise.jdx.dev/lang/rust.html':'mise Rust tool management','https://doc.rust-lang.org/1.99.1/book/':'The Rust Book (1.99.1)','https://doc.rust-lang.org/1.99.1/std/':'Rust standard library (1.99.1)','https://docs.rs/tokio/1.53.3/tokio/':'Tokio 1.53.3','https://docs.rs/axum/0.8.9/axum/':'Axum 0.8.9','https://docs.rs/sqlx/0.9.0/sqlx/':'SQLx 0.9.0','https://docs.rs/leptos/0.8.22/leptos/':'Leptos 0.8.22','https://book.leptos.dev/':'Leptos Book (unversioned conceptual guide)','https://www.postgresql.org/docs/18/transaction-iso.html':'PostgreSQL 18 transaction isolation'}
for x in lessons:
    for s in x.get('sources',[]):sources[s['url']]=s['title']
optional_sources={}
optional_page=ROOT/'authoring/optional-svelte.html'
if optional_page.exists():
    for url,title in re.findall(r'<a href="(https?://[^"]+)">([^<]+)</a>',optional_page.read_text()):
        optional_sources[url]=title;sources[url]=title

body='<div class="eyebrow">Verified 2026-10-17</div><h1>Sources and version ledger</h1><p>The course uses original explanations and primary references. Versioned API documentation takes precedence over examples from a newer development branch. Unversioned conceptual guides may evolve; use the Cargo lockfiles and compiler diagnostics to resolve drift.</p><table><thead><tr><th>Tool / library</th><th>Pinned / observed</th><th>Compatibility evidence</th></tr></thead><tbody><tr><td>Rust via mise</td><td>1.99.1 · edition 2024</td><td>Installed compiler; stable point release October 15, 2026</td></tr><tr><td>mise</td><td>2026.10.12 observed</td><td>Rust 1.99.1 selection and checks passed; see the October 17 resumed audit</td></tr><tr><td>Tokio / Axum</td><td>1.53.3 / 0.8.9</td><td>Native workspace exact pins and lockfile</td></tr><tr><td>SQLx / PostgreSQL</td><td>0.9.0 / 18.6 observed</td><td>SQLx declared MSRV 1.94; isolated integration tests recorded separately</td></tr><tr><td>Leptos</td><td>0.8.22, csr feature</td><td>Declared MSRV 1.88; stable chosen over 0.9 beta</td></tr><tr><td>wasm-bindgen / gloo-net / gloo-timers</td><td>0.2.129 / 0.7.0 / 0.4.0</td><td>Separate UI lockfile and WASM target</td></tr><tr><td>Rust MCP</td><td>0.4.1 configured</td><td>Direct MCP workspace/metadata/check passed; live session wrapper still errors; see follow-up evidence</td></tr></tbody></table><p>The optional <a href="optional-svelte.html">Svelte bridge</a> uses Node 26.12.0, Svelte 5.57.2, Vite 8.3.4 and compatible pinned tooling. It has a separate lockfile and does not use SvelteKit. See the <a href="verification/oct17-audit.html">current version and verification audit</a>; earlier logs retain their original scope.</p><h2>Editions, channels and support claims</h2><p>An edition chooses language compatibility rules for a crate; it is not a compiler version. Crates of different editions can interoperate. Stable is the supported release channel; nightly is needed for experimental tools such as Miri, with its own dated toolchain. <code>rust-version</code> declares a minimum supported compiler (MSRV). This course declares and tests 1.99, and does not claim all dependency feature combinations work on earlier versions simply because individual crates list lower MSRVs.</p>'
body += (ROOT/'authoring/version-audit-fragment.html').read_text()
for url,title in sorted(sources.items(),key=lambda x:x[1]):
    mapped=[x for x in lessons if any(s['url']==url for s in x.get('sources',[]))]
    body+=f'<section><h3><a href="{E(url)}">{E(title)}</a></h3><p>'+(' · '.join(f'<a href="{href(x)}">{E(x["title"])}</a>' for x in mapped) or ('<a href="optional-svelte.html">Optional Svelte bridge</a>' if url in optional_sources else 'Toolchain and course-wide reference'))+'</p></section>'
write('sources.html','Sources and versions',body)

body='<div class="eyebrow">Explanation → practice → evidence</div><h1>Coverage matrix</h1><p>Each row maps a requirement to authored instruction, its prediction/practice/investigation, project application, independent assessment and recorded evidence. These are coverage claims, not claims of learner mastery. Specializations have deliberate limits; unavailable checks remain visible.</p><h2>Original curriculum requirements</h2><p>This map starts from the requested objectives rather than only the headings we happened to author. See also the <a href="projects/checkpoints/index.html">reconstructible checkpoints</a> and <a href="authoring/requirements-map.json">machine-readable map</a>.</p><div class="table-scroll"><table><thead><tr><th>Required objective</th><th>Explanation and practice</th><th>Application / assessment</th><th>Evidence and scope limits</th></tr></thead><tbody>'
for row in json.loads((ROOT/'authoring/requirements-map.json').read_text())['rows']:
    explanations=' · '.join(f'<a href="{href(byid[l])}">{E(byid[l]["title"])}</a>' for l in row['lessons'])
    application=' · '.join(f'<a href="projects.html#{p}">{p}</a>' for p in row['projects'])
    evidence=' · '.join(f'<a href="{E(e)}">{E(Path(e).name)}</a>' for e in row['verification'])
    body+=f'<tr><td>{E(row["objective"])}</td><td>{explanations}</td><td>{application}<br><a href="{row["assessment"]}">Independent assessment</a></td><td>{evidence}<p>{E(row["scope"])}</p></td></tr>'
body+='</tbody></table></div><h2>Detailed lesson objectives</h2><div class="table-scroll"><table><thead><tr><th>Objective</th><th>Explanation and practice</th><th>Application / assessment</th><th>Verification</th></tr></thead><tbody>'
for x in lessons:
    for o in x.get('objectives',[]):body+=f'<tr><td>{E(o)}</td><td><a href="{href(x)}">{E(x["title"])}</a></td><td><a href="projects.html#{x.get("project","")}">{E(x.get("project","specialization"))}</a> / <a href="assessments/index.html#gate-{x["stage"]}">Stage {x["stage"]}</a></td><td><a href="verification/index.html">Exact commands and limitations</a></td></tr>'
body+='</tbody></table></div>';write('coverage.html','Coverage matrix',body)

for optional in [('optional-svelte.html','Optional Svelte 5 bridge','A separate read-only client for the same Rust API'),('playbook.html','Problem-solving playbook','Investigations, hypotheses, experiments and design templates'),('projects/checkpoints/index.html','Runnable project checkpoints','Reconstruct complete early milestones and final references'),('intentional-failures.html','Intentional failure inventory','Compiler errors and deliberately incorrect runtime oracles')]:
    if (ROOT/optional[0]).exists():index.append({'href':optional[0],'title':optional[1],'summary':optional[2],'type':'Reference'})

(ROOT/'assets/search-index.js').write_text('window.COURSE_INDEX = '+json.dumps(index,ensure_ascii=False)+';\n')
(ROOT/'verification/snippet-map.json').write_text(json.dumps(snippets,indent=2))
old=json.loads((ROOT/'course-status.json').read_text()) if (ROOT/'course-status.json').exists() else {}
oldless={x['id']:x for x in old.get('lessons',[])};oldproj={x['id']:x for x in old.get('projects',[])}
status={**old,'schema_version':2,'verification_date':'2026-10-17','generation_finished':old.get('generation_finished',False),'independent_review':'pending','learner_competence':'not_assessed','toolchain':'1.99.1','tool_manager':'mise','lessons':[],'projects':[],'remaining_work':old.get('remaining_work',['Finish authorship, self-review, compiler checks, boundary tests, and browser inspection before marking generation finished.'])}
for x in lessons:status['lessons'].append({**oldless.get(x['id'],{}),'id':x['id'],'title':x['title'],'path':href(x),'stage':x['stage'],'authored':True,'author_self_review_complete':oldless.get(x['id'],{}).get('author_self_review_complete',False),'code_verified':oldless.get(x['id'],{}).get('code_verified',False),'independent_review':'pending','playbook_application':bool(re.search('playbook|investigation',x['body'],re.I))})
for p in project_entries:status['projects'].append({**p,**oldproj.get(p['id'],{}),'source':p['source'],'authored':p['authored'],'independent_review':'pending'})
# Changing reviewed prose or source must not silently preserve a verification claim.
# These fingerprints identify reviewed input, not proof of application correctness.
def fingerprint(paths,extra=''):
    digest=hashlib.sha256(extra.encode())
    for path in sorted(set(paths)):
        digest.update(path.relative_to(ROOT).as_posix().encode())
        digest.update(path.read_bytes())
    return digest.hexdigest()
shared=[ROOT/'projects/Cargo.toml',ROOT/'projects/Cargo.lock',ROOT/'rust-toolchain.toml']
for item in status['lessons']:
    authored=byid[item['id']]
    paths=[ROOT/p for p in re.findall(r'data-code="([^"]+)"',authored['body'])]
    digest=fingerprint(paths+shared,json.dumps(authored,sort_keys=True))
    previous=item.get('reviewed_input_sha256')
    if previous and previous!=digest:
        item['author_self_review_complete']=False
        item['code_verified']=False
        item['verification_note']='Reviewed input changed; repeat the applicable checks and author review.'
    item['reviewed_input_sha256']=digest
for item in status['projects']:
    folder=ROOT/item['source']
    paths=[p for p in folder.rglob('*') if p.is_file() and p.suffix in ('.rs','.toml','.lock','.sql','.json','.html') and not any(x in ('target','pkg','.local') for x in p.relative_to(folder).parts)]
    if item['id']=='p11':paths+=list((ROOT/'projects/reference/p11-ui-model').rglob('*.rs'))
    digest=fingerprint(paths+shared)
    previous=item.get('reviewed_input_sha256')
    if previous and previous!=digest:
        item['author_self_review_complete']=False
        item['code_verified']=False
        item['verification_note']='Reviewed input changed; repeat the applicable checks and author review.'
    item['reviewed_input_sha256']=digest
(ROOT/'course-status.json').write_text(json.dumps(status,indent=2))
print(f'Rendered {len(lessons)} lessons, {len(project_entries)} project entries, {len(snippets)} source inclusions.')
