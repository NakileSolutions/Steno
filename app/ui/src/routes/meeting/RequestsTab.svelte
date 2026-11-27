<script lang="ts">
  import { api } from '../../lib/api';
  import type { MeetingDetail } from '../../lib/types';
  import { toastError } from '../../lib/ui.svelte';
  import RequestCard from '../../lib/components/RequestCard.svelte';
  import Icon from '../../lib/components/Icon.svelte';

  let { detail, focusId }: { detail: MeetingDetail; focusId?: string } = $props();
  const m = $derived(detail.meeting);
  const list = $derived([...detail.requests].sort((a, b) => b.created_at.localeCompare(a.created_at)));
  let query = $state('');
  let sending = $state(false);

  async function ask(e: SubmitEvent) {
    e.preventDefault();
    const q = query.trim();
    if (!q) return;
    sending = true;
    try {
      const r = await api.ask(q, m.id);
      if (!detail.requests.some((x) => x.id === r.id)) detail.requests.push(r);
      query = '';
    } catch (err) {
      toastError(err);
    } finally {
      sending = false;
    }
  }
</script>

<div class="wrap">
  {#if !m.confidential}
    <form class="ask row" onsubmit={ask}>
      <Icon name="ask" size={16} />
      <input class="grow" bind:value={query} placeholder="Poser une question sur cette réunion…" aria-label="Demander au Scribe à propos de cette réunion" />
      <button class="btn sm primary" disabled={!query.trim() || sending}>Demander</button>
    </form>
  {/if}

  {#if !list.length}
    <div class="empty">
      <Icon name="ask" size={24} stroke={1.5} />
      <h2>Aucune demande</h2>
      <p class="small">Pendant une réunion, dites « Scribe, cherche… » ou tapez votre question.</p>
    </div>
  {:else}
    <div class="col list">
      {#each list as r (r.id)}
        <RequestCard req={r} focused={focusId === r.id} />
      {/each}
    </div>
  {/if}
</div>

<style>
  .wrap { padding: 18px 32px 48px; max-width: calc(72ch + 64px); }
  .ask {
    gap: 10px;
    padding: 4px 4px 4px 12px;
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    color: var(--text-3);
    margin-bottom: 18px;
    background: var(--surface);
    box-shadow: var(--shadow-md);
    transition: border-color var(--t-fast) var(--ease), box-shadow var(--t-fast) var(--ease), color var(--t-fast) var(--ease);
  }
  .ask:focus-within { border-color: var(--focus); box-shadow: var(--ring); color: var(--accent-text); }
  .ask input { border: 0; outline: 0; background: transparent; color: var(--text); font: inherit; font-size: 14px; height: 32px; }
  .ask input::placeholder { color: var(--text-3); }
  .list { gap: 12px; }
</style>
