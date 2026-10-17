/* Pure backup validation: usable from file:// and testable without a browser. */
((root) => {
  'use strict';
  function record(value, type, label) {
    if (!value || typeof value !== 'object' || Array.isArray(value)) {
      throw new Error(`Invalid ${label} map`);
    }
    for (const [key, entry] of Object.entries(value)) {
      if (!/^[a-z0-9_-]+$/.test(key) || typeof entry !== type) {
        throw new Error(`Invalid ${label} entry: ${key}`);
      }
    }
    return Object.fromEntries(Object.entries(value));
  }
  function parse(text) {
    const value = JSON.parse(text);
    if (!value || value.version !== 1) throw new Error('Unsupported progress version');
    return {
      version: 1,
      completed: record(value.completed, 'boolean', 'completed'),
      notes: record(value.notes, 'string', 'notes'),
      theme: value.theme === 'dark' ? 'dark' : 'light',
    };
  }
  // Detect a completed intervening write. This is not an atomic compare-and-set:
  // localStorage does not provide a transaction across the later read and write.
  function prepareSave(state, expectedRaw, currentRaw) {
    if (currentRaw !== expectedRaw) return { conflict: true, currentRaw };
    return { conflict: false, text: JSON.stringify(state) };
  }
  const api = { parse, prepareSave };
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  else root.COURSE_PROGRESS = api;
})(globalThis);
