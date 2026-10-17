import { parse } from 'lossless-json';
/** @typedef {{symbol:string,price_cents:bigint,sequence:bigint}} Quote */
/** @typedef {{symbol:string,quantity:bigint}} Position */
/** @typedef {{order_id:string,symbol:string,side:'buy'|'sell',quantity:bigint,price_cents:bigint}} Fill */
/** @typedef {{simulated:true,cash_cents:bigint,quotes:Quote[],positions:Position[],orders:Fill[],watchlist:string[],events:bigint}} Snapshot */
const I64_MAX = 9223372036854775807n;
const U64_MAX = 18446744073709551615n;
export const MAX_BYTES = 65_536;
/** @param {unknown} value @returns {value is Record<string,unknown>} */
function object(value) { return value !== null && typeof value === 'object' && !Array.isArray(value); }
/** @param {unknown} value @param {bigint} min @param {bigint} max */
function integer(value, min, max) { return typeof value === 'bigint' && value >= min && value <= max; }
/** @param {unknown} value @returns {value is string} */
function symbol(value) { return typeof value === 'string' && /^[A-Z]{1,8}$/.test(value); }
/** @param {unknown} value @returns {value is string} */
function identifier(value) { return typeof value === 'string' && /^[A-Za-z0-9_-]{1,80}$/.test(value); }
/** @param {unknown[]} rows @param {string} key */
function unique(rows, key) { return new Set(rows.map(row => object(row) ? row[key] : undefined)).size === rows.length; }
/** Decode the existing Rust JSON contract without rounding i64/u64 values.
 * @param {string} text @returns {Snapshot}
 */
export function decodeSnapshot(text) {
  const value = parse(text, null, { parseNumber: (number) => {
    if (!/^-?\d+$/.test(number) || number.length > 21) throw new Error('Expected bounded integer JSON');
    return BigInt(number);
  }});
  if (!object(value) || value.simulated !== true || !integer(value.cash_cents, 0n, I64_MAX)
      || !integer(value.events, 0n, 10_000n)) throw new Error('Invalid snapshot envelope');
  const { quotes, positions, orders, watchlist } = value;
  if (!Array.isArray(quotes) || quotes.length > 32 || !unique(quotes, 'symbol')
      || !quotes.every(q => object(q) && symbol(q.symbol) && integer(q.price_cents, 1n, 100_000_000n) && integer(q.sequence, 0n, U64_MAX))) throw new Error('Invalid quotes');
  if (!Array.isArray(positions) || positions.length > 32 || !unique(positions, 'symbol')
      || !positions.every(p => object(p) && symbol(p.symbol) && integer(p.quantity, 0n, I64_MAX))) throw new Error('Invalid positions');
  if (!Array.isArray(orders) || orders.length > 100 || !unique(orders, 'order_id')
      || !orders.every(o => object(o) && identifier(o.order_id) && symbol(o.symbol) && (o.side === 'buy' || o.side === 'sell') && integer(o.quantity, 1n, 10_000n) && integer(o.price_cents, 1n, 100_000_000n))) throw new Error('Invalid orders');
  if (!Array.isArray(watchlist) || watchlist.length > 32 || new Set(watchlist).size !== watchlist.length
      || !watchlist.every(symbol)) throw new Error('Invalid watchlist');
  // The preceding structural checks establish this read model at the boundary.
  return /** @type {Snapshot} */ (/** @type {unknown} */ (value));
}
/** Format exact cents without passing through floating point. @param {bigint} cents */
export function money(cents) {
  const magnitude = cents < 0n ? -cents : cents;
  return `${cents < 0n ? '-' : ''}$${magnitude / 100n}.${String(magnitude % 100n).padStart(2, '0')}`;
}
/** One outstanding fetch per client, explicit byte budget, deadline and cancellation.
 * @param {{fetcher?:typeof fetch,deadlineMs?:number}} [options]
 */
export function createSnapshotClient({ fetcher = globalThis.fetch, deadlineMs = 3000 } = {}) {
  if (!Number.isFinite(deadlineMs) || deadlineMs <= 0 || deadlineMs > 30_000) throw new Error('Invalid deadline');
  /** @type {AbortController | null} */
  let active = null;
  return {
    cancel() { active?.abort(); },
    async load() {
      if (active) throw new Error('A snapshot request is already active');
      const controller = new AbortController();
      active = controller;
      const timer = setTimeout(() => controller.abort(), deadlineMs);
      try {
        const response = await fetcher('/api/snapshot', {
          headers: { Authorization: 'Bearer demo-reader' },
          credentials: 'omit', cache: 'no-store', signal: controller.signal
        });
        controller.signal.throwIfAborted();
        if (!response.ok) throw new Error(`Snapshot request failed (HTTP ${response.status})`);
        if (!response.headers.get('content-type')?.startsWith('application/json') || !response.body) throw new Error('Expected JSON response');
        const reader = response.body.getReader();
        const decoder = new TextDecoder('utf-8', { fatal: true });
        let bytes = 0;
        let text = '';
        try {
          while (true) {
            const chunk = await reader.read();
            controller.signal.throwIfAborted();
            if (chunk.done) break;
            bytes += chunk.value.byteLength;
            if (bytes > MAX_BYTES) throw new Error('Snapshot response exceeds byte budget');
            text += decoder.decode(chunk.value, { stream: true });
          }
          text += decoder.decode();
        } finally { reader.releaseLock(); }
        controller.signal.throwIfAborted();
        return decodeSnapshot(text);
      } catch (error) {
        controller.abort();
        throw error;
      } finally {
        clearTimeout(timer);
        active = null;
      }
    }
  };
}
