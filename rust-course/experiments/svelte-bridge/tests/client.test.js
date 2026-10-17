import test from 'node:test';
import assert from 'node:assert/strict';
import { decodeSnapshot, money, createSnapshotClient, MAX_BYTES } from '../src/client.js';
const fixture = '{"simulated":true,"cash_cents":9223372036854775807,"quotes":[{"symbol":"RUST","price_cents":125,"sequence":18446744073709551615}],"positions":[{"symbol":"RUST","quantity":1}],"orders":[{"order_id":"buy-1","symbol":"RUST","side":"buy","quantity":1,"price_cents":125}],"watchlist":["RUST"],"events":3}';
const response = text => new Response(text, { headers: { 'Content-Type': 'application/json' } });
test('i64 and u64 are decoded exactly and money stays fixed point', () => {
  const value = decodeSnapshot(fixture);
  assert.equal(value.cash_cents, 9223372036854775807n);
  assert.equal(value.quotes[0].sequence, 18446744073709551615n);
  assert.equal(money(125n), '$1.25');
  assert.equal(money(-1n), '-$0.01');
  assert.equal(money(value.cash_cents), '$92233720368547758.07');
});
test('wrong envelopes, duplicate symbols, floats and oversized integers fail', () => {
  assert.throws(() => decodeSnapshot(fixture.replace('"simulated":true', '"simulated":false')));
  assert.throws(() => decodeSnapshot(fixture.replace('"watchlist":["RUST"]', '"watchlist":["RUST","RUST"]')));
  assert.throws(() => decodeSnapshot(fixture.replace('"price_cents":125', '"price_cents":1.25')));
  assert.throws(() => decodeSnapshot(fixture.replace('18446744073709551615', '18446744073709551616')));
});
test('GET uses exact local reader fixture and releases ownership after success', async () => {
  let calls = 0;
  const client = createSnapshotClient({ fetcher: async (url, options) => {
    calls += 1;
    assert.equal(url, '/api/snapshot');
    assert.equal(options.headers.Authorization, 'Bearer demo-reader');
    assert.equal(options.credentials, 'omit');
    assert.equal(options.cache, 'no-store');
    return response(fixture);
  }});
  assert.equal((await client.load()).events, 3n);
  await client.load(); assert.equal(calls, 2);
});
test('HTTP errors and oversized response bodies are explicit', async () => {
  await assert.rejects(createSnapshotClient({ fetcher: async () => new Response('do not display internals', { status: 503 }) }).load(), /HTTP 503/);
  await assert.rejects(createSnapshotClient({ fetcher: async () => response('x'.repeat(MAX_BYTES + 1)) }).load(), /byte budget/);
});
test('only one request is admitted; cancellation ends ownership', async () => {
  let called = 0;
  const client = createSnapshotClient({ fetcher: async (_url, { signal }) => {
    called += 1;
    return new Promise((_resolve, reject) => signal.addEventListener('abort', () => reject(signal.reason), { once: true }));
  }});
  const first = client.load();
  await assert.rejects(client.load(), /already active/);
  client.cancel(); await assert.rejects(first, /abort/i);
  assert.equal(called, 1);
});
test('deadline aborts a slow body as well as initial fetch', async () => {
  const client = createSnapshotClient({ deadlineMs: 5, fetcher: async (_url, { signal }) =>
    new Response(new ReadableStream({ start(controller) {
      signal.addEventListener('abort', () => controller.error(signal.reason), { once: true });
    }}), { headers: { 'Content-Type': 'application/json' } })
  });
  await assert.rejects(client.load(), /abort/i);
});
test('a response arriving after cancellation is never accepted', async () => {
  let resolve;
  const client = createSnapshotClient({ fetcher: () => new Promise(done => { resolve = done; }) });
  const pending = client.load(); client.cancel(); resolve(response(fixture));
  await assert.rejects(pending, /abort/i);
});
