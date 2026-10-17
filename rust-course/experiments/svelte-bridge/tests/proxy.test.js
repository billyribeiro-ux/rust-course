import test from 'node:test';
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { snapshotAdmission } from '../vite.config.js';
class Reply extends EventEmitter {
  status = 0;
  writeHead(status) { this.status = status; return this; }
  end() { this.emit('finish'); }
}
function fixture() {
  let handler;
  snapshotAdmission().configureServer({ middlewares: { use(value) { handler = value; } } });
  return (overrides = {}) => {
    const reply = new Reply(); let forwarded = false;
    handler({ url: '/api/snapshot', method: 'GET', headers: {}, ...overrides }, reply, () => { forwarded = true; });
    return { reply, forwarded };
  };
}
test('proxy allows only the intended body-free read boundary', () => {
  const request = fixture();
  assert.equal(request({ url: '/api/orders' }).reply.status, 404);
  assert.equal(request({ method: 'POST' }).reply.status, 405);
  assert.equal(request({ headers: { origin: 'https://untrusted.invalid' } }).reply.status, 403);
  assert.equal(request({ headers: { 'content-length': '1' } }).reply.status, 400);
  assert.equal(request({ headers: { 'transfer-encoding': 'chunked' } }).reply.status, 400);
});
test('four admissions, rejection, and exactly-once permit release', () => {
  const request = fixture();
  const held = Array.from({ length: 4 }, () => request());
  assert(held.every(value => value.forwarded));
  assert.equal(request().reply.status, 429);
  held[0].reply.emit('close'); held[0].reply.emit('finish');
  assert.equal(request().forwarded, true);
  assert.equal(request().reply.status, 429);
});
