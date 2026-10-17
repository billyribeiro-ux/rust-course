<script>
  import { onMount } from 'svelte';
  import { createSnapshotClient, money } from './client.js';

  /** @type {import('./client.js').Snapshot | null} */
  let snapshot = $state.raw(null);
  let filter = $state('');
  let busy = $state(false);
  let status = $state('Waiting for the first snapshot.');
  let error = $state('');
  let received = $state('');
  const client = createSnapshotClient();
  let disposed = false;
  const visibleQuotes = $derived.by(() => (snapshot?.quotes ?? []).filter(quote => quote.symbol.includes(filter.trim().toUpperCase())));

  async function refresh() {
    if (busy || disposed) return;
    busy = true;
    error = '';
    status = snapshot ? 'Refreshing; previous snapshot remains visible.' : 'Loading the first snapshot…';
    try {
      const next = await client.load();
      if (disposed) return;
      snapshot = next;
      received = new Date().toLocaleTimeString();
      status = 'Snapshot received. Refresh to request another.';
    } catch (problem) {
      if (disposed) return;
      error = problem instanceof Error && problem.name !== 'AbortError' ? problem.message : 'Request cancelled or its three-second deadline expired.';
      status = snapshot ? 'Refresh failed; the displayed snapshot may be stale.' : 'No snapshot is available.';
    } finally {
      if (!disposed) busy = false;
    }
  }

  onMount(() => {
    void refresh();
    return () => { disposed = true; client.cancel(); };
  });
</script>

<svelte:head><title>Simulated Rust snapshot · optional Svelte bridge</title></svelte:head>
<main>
  <p class="eyebrow">Optional language boundary lab</p>
  <h1>Rust snapshot, Svelte interface</h1>
  <p class="notice"><strong>SIMULATED DATA ONLY.</strong> This read-only client uses public local fixture credentials. It does not place orders or connect to a broker.</p>
  <div class="actions">
    <button type="button" onclick={refresh} disabled={busy}>Refresh snapshot</button>
    <button type="button" onclick={() => client.cancel()} disabled={!busy}>Cancel request</button>
  </div>
  <p role="status">{status}</p>
  {#if error}<p class="error" role="alert">{error} If the service is unavailable, start the local Rust workbench on port 4116, then retry.</p>{/if}
  {#if snapshot}
    <p>Received locally at {received}. Committed events: {snapshot.events.toString()}. Simulated cash: <strong>{money(snapshot.cash_cents)}</strong>.</p>
    <label for="symbol-filter">Filter quote symbols</label>
    <input id="symbol-filter" bind:value={filter} maxlength="8" placeholder="For example, RUST" autocomplete="off" />
    <p>Watchlist: {snapshot.watchlist.length ? snapshot.watchlist.join(', ') : 'empty'}.</p>
    {#if snapshot.quotes.length === 0}
      <p>No quotes yet. Add a synthetic tick using the course terminal example.</p>
    {:else if visibleQuotes.length === 0}
      <p>No symbols match this filter. Clear it to show all quotes.</p>
    {:else}
      <div class="table-wrap"><table>
        <caption>Latest accepted synthetic quotes</caption>
        <thead><tr><th scope="col">Symbol</th><th scope="col">Price</th><th scope="col">Sequence</th></tr></thead>
        <tbody>{#each visibleQuotes as quote (quote.symbol)}<tr><th scope="row">{quote.symbol}</th><td>{money(quote.price_cents)}</td><td>{quote.sequence.toString()}</td></tr>{/each}</tbody>
      </table></div>
    {/if}
    <h2>Position ledger</h2>
    {#if snapshot.positions.length === 0}<p>No simulated positions.</p>
    {:else}<ul>{#each snapshot.positions as position (position.symbol)}<li>{position.symbol}: {position.quantity.toString()} whole shares</li>{/each}</ul>{/if}
  {/if}
  <p class="scope">One request at a time; a three-second deadline and a 64 KiB response budget. Filter and snapshot state reset on reload. The course's main interface remains Leptos.</p>
</main>

<style>
  :global(body) { margin: 0; background: #101b2a; color: #e6edf5; font: 1rem/1.65 system-ui, sans-serif; }
  main { max-width: 72ch; margin: 2rem auto; padding: 0 1.2rem 3rem; }
  h1 { font-size: clamp(1.8rem, 5vw, 2.7rem); line-height: 1.15; }
  .eyebrow { color: #99c9de; letter-spacing: .05em; }
  .notice { border-left: .3rem solid #e8ba70; padding: .8rem 1rem; background: #1d2b3f; }
  .actions { display: flex; gap: .7rem; flex-wrap: wrap; }
  button, input { font: inherit; border: 1px solid #9baabb; border-radius: .3rem; padding: .55rem .8rem; }
  button { background: #b4e4f0; color: #102538; cursor: pointer; }
  button:disabled { opacity: .55; cursor: default; }
  input { display: block; margin: .4rem 0 1rem; max-width: 100%; box-sizing: border-box; background: #182a3f; color: #fff; }
  :global(:focus-visible) { outline: 3px solid #f0c16d; outline-offset: 3px; }
  .error { border-left: .3rem solid #ffa9a9; padding: .6rem 1rem; color: #ffcbcb; }
  .table-wrap { overflow-x: auto; }
  table { border-collapse: collapse; width: 100%; }
  caption { text-align: left; font-weight: 700; padding: .5rem 0; }
  th, td { text-align: left; border-bottom: 1px solid #53687f; padding: .6rem; }
  .scope { border-top: 1px solid #53687f; padding-top: 1rem; color: #b7c9db; }
</style>
