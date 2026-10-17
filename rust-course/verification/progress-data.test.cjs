// Node tests for backup data only; these do not exercise a DOM or browser policy.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { parse, prepareSave } = require('../assets/progress-data.js');
const backup = (notes = {}) => ({ version: 1, completed: { '00-terminal': true }, notes, theme: 'dark' });
test('exported notes round-trip, including long Unicode text', () => {
  const original = backup({ '00-terminal': '🦀'.repeat(100001) });
  assert.deepEqual(parse(JSON.stringify(original)), original);
});
test('invalid entries reject the entire import instead of dropping notes', () => {
  for (const notes of [{ valid: 'kept', invalid: 1 }, { 'wrong/key': 'text' }, [], null]) {
    assert.throws(() => parse(JSON.stringify(backup(notes))), /Invalid notes/);
  }
});
test('completed values must be booleans', () => {
  const value = backup(); value.completed['00-terminal'] = 'false';
  assert.throws(() => parse(JSON.stringify(value)), /Invalid completed/);
});
test('unknown versions and malformed JSON are rejected', () => {
  assert.throws(() => parse(JSON.stringify({ ...backup(), version: 2 })), /version/);
  assert.throws(() => parse('{'));
});
test('special object keys are own data, not a prototype change', () => {
  const value = parse('{"version":1,"completed":{},"notes":{"__proto__":"saved"}}');
  assert.equal(Object.getPrototypeOf(value.notes), Object.prototype);
  assert.equal(Object.hasOwn(value.notes, '__proto__'), true);
  assert.equal(value.notes.__proto__, 'saved');
});
test('a stale lesson tab cannot authorize overwriting a completed newer save', () => {
  const original = JSON.stringify(backup({ '00-terminal': 'first note' }));
  const tabA = parse(original);
  const tabB = parse(original);
  tabA.notes['01-ownership'] = 'ownership investigation from tab A';
  const savedA = prepareSave(tabA, original, original);
  assert.equal(savedA.conflict, false);
  tabB.notes['00-terminal'] = 'unsaved investigation from tab B';
  const decisionB = prepareSave(tabB, original, savedA.text);
  assert.deepEqual(decisionB, { conflict: true, currentRaw: savedA.text });
  assert.equal(Object.hasOwn(decisionB, 'text'), false);
  // Both pieces of evidence remain available for explicit recovery.
  assert.equal(parse(decisionB.currentRaw).notes['01-ownership'], tabA.notes['01-ownership']);
  assert.equal(tabB.notes['00-terminal'], 'unsaved investigation from tab B');
});
test('first save and repeated saves work when the expected snapshot still matches', () => {
  const draft = backup({ '00-terminal': 'before' });
  const first = prepareSave(draft, null, null);
  assert.equal(first.conflict, false);
  assert.deepEqual(parse(first.text), draft);
  draft.notes['00-terminal'] = 'after';
  const next = prepareSave(draft, first.text, first.text);
  assert.equal(next.conflict, false);
  assert.equal(parse(next.text).notes['00-terminal'], 'after');
});
test('deletion, malformed replacement and another first save also conflict', () => {
  const draft = backup();
  const original = JSON.stringify(draft);
  for (const [expected, current] of [[original, null], [original, '{broken'], [null, original]]) {
    assert.deepEqual(prepareSave(draft, expected, current), { conflict: true, currentRaw: current });
  }
});
