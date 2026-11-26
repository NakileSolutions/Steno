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
      <Icon name="ask" size={15} />
      <input class="grow" bind:value={query} placeholder="Poser une question sur cette réunion…" aria-label="Demander au Scribe à propos de cette réunion" />
      <button class="btn sm primary" disabled={!query.trim() || sending}>Demander</button>
    </form>
  {/if}

  {#if !list.length}
    <div class="empty">
      <Icon name="ask" size={28} stroke={1.2} />
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
  .wrap { padding: 16px 28px 40px; max-width: 820px; }
  .ask {
    gap: 8px;
    padding: 4px 4px 4px 11px;
    border: 1px solid var(--line-strong);
    border-radius: 9px;
    color: var(--text-3);
    margin-bottom: 16px;
    background: var(--surface);
  }
  .ask:focus-within { border-color: var(--focus); box-shadow: 0 0 0 3px color-mix(in srgb, var(--focus) 22%, transparent); }
  .ask input { border: 0; outline: 0; background: transparent; color: var(--text); font: inherit; height: 26px; }
  .list { gap: 12px; }
</style>
