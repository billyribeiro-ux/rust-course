#!/usr/bin/env python3
"""Save dated primary-reference responses and hashes; reachability is not review."""
import concurrent.futures
import datetime
import gzip
import hashlib
import json
from pathlib import Path
import urllib.request
from urllib.parse import urldefrag

ROOT = Path(__file__).resolve().parents[1]
urls = {}
for path in (ROOT / 'authoring').glob('*.json'):
    data = json.loads(path.read_text())
    if isinstance(data, list):
        for lesson in data:
            if isinstance(lesson, dict) and 'body' in lesson:
                for source in lesson.get('sources', []):
                    url = urldefrag(source['url'])[0]
                    urls.setdefault(url, []).append(lesson['id'])
for url in ('https://rust-lang.github.io/api-guidelines/checklist.html',
            'https://doc.rust-lang.org/1.99.1/edition-guide/rust-2024/unsafe-op-in-unsafe-fn.html',
            'https://doc.rust-lang.org/1.99.1/cargo/reference/resolver.html',
            'https://www.postgresql.org/support/versioning/',
            'https://nodejs.org/en/blog/release/v26.12.0'):
    urls.setdefault(url, [])
output = ROOT / 'sources/2026-10-17'
output.mkdir(parents=True, exist_ok=True)


def fetch(item):
    url, lessons = item
    record = {'url': url, 'lessons': lessons}
    try:
        request = urllib.request.Request(url, headers={'User-Agent': 'rust-course-primary-source-audit/1.0'})
        with urllib.request.urlopen(request, timeout=45) as response:
            raw = response.read()
            record.update(status=response.status, final_url=response.url,
                          last_modified=response.headers.get('Last-Modified'))
        digest = hashlib.sha256(raw).hexdigest()
        snapshot = output / (hashlib.sha256(url.encode()).hexdigest()[:20] + '.gz')
        snapshot.write_bytes(gzip.compress(raw, mtime=0))
        record.update(sha256=digest, bytes=len(raw), snapshot=str(snapshot.relative_to(ROOT)))
    except Exception as error:
        record['error'] = str(error)
        print(url, error, flush=True)
    return record


with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
    records = list(pool.map(fetch, sorted(urls.items())))
report = {'as_of': '2026-10-17', 'retrieved_at': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'scope': 'HTTP retrieval and raw-response hashes for lesson references. Semantic review and executed checks are separately recorded.',
          'sources': records}
(ROOT / 'verification/primary-sources-2026-10-17.json').write_text(json.dumps(report, indent=2) + '\n')
print('Sources:', len(records), 'retrieved:', sum('sha256' in r for r in records),
      'unavailable:', sum('error' in r for r in records))
