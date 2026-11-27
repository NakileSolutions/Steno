<script lang="ts">
  import { onMount } from 'svelte';
  import { api, onEvent } from '../lib/api';
  import { router, href, setQuery } from '../lib/router.svelte';
  import type { Request, SearchHit } from '../lib/types';
  import { debounce, highlight, shortDate, time, dayLabel } from '../lib/format';
  import { toastError } from '../lib/ui.svelte';
  import Icon from '../lib/components/Icon.svelte';
  import RequestCard from '../lib/components/RequestCard.svelte';
  import { groupHits } from '../lib/group';

  let query = $state(router.route.query.q ?? '');
  let hits = $state<SearchHit[]>([]);
  let searched = $state('');
  let aiReq = $state<Request | null>(null);
  let input: HTMLInputElement | undefined = $state();

  async function run() {
    const q = query.trim();
    setQuery({ q: q || null });
    if (!q) {
      hits = [];
      searched = '';
      return;
    }
    try {
      hits = await api.search(q);
      searched = q;
    } catch (e) {
      toastError(e);
    }
  }
  const runSoon = debounce(run, 200);

  onMount(() => {
    if (query) run();
    input?.focus();
    return onEvent('scribe://request', (r) => {
      if (aiReq && r.id === aiReq.id) aiReq = r;
    });
  });

  async function askAi() {
    const q = query.trim();
    if (!q) return;
    try {
      aiReq = await api.askHistory(q);
    } catch (e) {
      toastError(e);
    }
  }

  const groups = $derived(groupHits(hits));
</script>

<div class="page scroll">
  <div class="inner">
    <h1>Recherche</h1>
    <form class="bar row" onsubmit={(e) => { e.preventDefault(); run(); }}>
      <label class="searchbox grow">
        <Icon name="search" size={16} />
        <input
          bind:this={input}
          data-search
          type="search"
          bind:value={query}
          oninput={runSoon}
          placeholder="Mot, ticket (APP-1234), service, personne…"
          aria-label="Rechercher dans toutes les réunions"
        />
      </label>
      <button type="button" class="btn" onclick={askAi} disabled={!query.trim()} title="Poser la question à l’agent IA, qui lit l’historique via le serveur MCP Scribe">
        <Icon name="ask" size={14} />Demander à l’IA
      </button>
    </form>

    {#if aiReq}
      <section class="ai">
        <div class="row"><h2 class="label grow">Réponse de l’IA</h2><button class="btn ghost sm icon" onclick={() => (aiReq = null)} aria-label="Fermer la réponse"><Icon name="x" size={13} /></button></div>
        <RequestCard req={aiReq} />
      </section>
    {/if}

    {#if searched}
      <p class="faint small count">{hits.length} résultat{hits.length > 1 ? 's' : ''} dans {groups.length} réunion{groups.length > 1 ? 's' : ''}</p>
    {/if}

    {#each groups as g (g.id)}
      <section class="group">
        <a class="gtitle row" href={href(`/meetings/${g.id}`)}>
          <span class="grow ellipsis">{g.title ?? 'Réunion sans titre'}</span>
          <span class="faint small">{dayLabel(g.started_at)} · {time(g.started_at)}</span>
        </a>
        <ul>
          {#each g.hits as h, i (i)}
            <li>
              <a class="hit" href={href(`/meetings/${g.id}?tab=${h.kind === 'report' ? 'report' : 'transcript'}`)}>
                <span class="kind">{h.kind === 'report' ? 'Compte rendu' : h.ts ? time(h.ts) : 'Transcription'}</span>
                <span class="snip">{@html highlight(h.snippet, searched)}</span>
              </a>
            </li>
          {/each}
        </ul>
      </section>
    {:else}
      {#if searched}
        <div class="empty">
          <p>Aucun résultat pour « {searched} ».</p>
          <p class="small">Essayez « Demander à l’IA » pour une question formulée.</p>
        </div>
      {:else}
        <div class="empty">
          <Icon name="search" size={30} stroke={1.2} />
          <p>Recherche plein texte dans toutes les transcriptions et tous les comptes rendus.</p>
          <p class="small">Les phrases privées ne sont jamais indexées dans les résultats.</p>
        </div>
      {/if}
    {/each}
    {#if groups.length}<p class="faint tiny">Dates : {shortDate(groups.at(-1)!.started_at)} → {shortDate(groups[0].started_at)}</p>{/if}
  </div>
</div>

<style>
  .page { height: 100%; }
  .inner { max-width: 820px; padding: 24px 32px 48px; }
  .bar { gap: 8px; margin: 14px 0 18px; }
  .searchbox {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 12px;
    border-radius: 9px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    color: var(--text-3);
  }
  .searchbox:focus-within { border-color: var(--focus); box-shadow: 0 0 0 3px color-mix(in srgb, var(--focus) 22%, transparent); }
  .searchbox input { flex: 1; border: 0; outline: 0; background: transparent; font: inherit; font-size: 14px; color: var(--text); }
  .bar .btn { height: 36px; }
  .ai { margin-bottom: 22px; }
  .ai .label { margin-bottom: 6px; }
  .count { margin-bottom: 8px; }
  .group { margin-bottom: 14px; border: 1px solid var(--line); border-radius: var(--radius-lg); background: var(--surface); overflow: hidden; }
  .gtitle { padding: 9px 14px; font-weight: 600; text-decoration: none; color: inherit; border-bottom: 1px solid var(--line); gap: 12px; }
  .gtitle:hover { background: var(--hover); }
  ul { list-style: none; margin: 0; padding: 4px 0; }
  .hit { display: grid; grid-template-columns: 96px 1fr; gap: 10px; padding: 6px 14px; text-decoration: none; color: inherit; }
  .hit:hover { background: var(--hover); }
  .kind { font-size: 11.5px; color: var(--text-3); padding-top: 1px; font-variant-numeric: tabular-nums; }
  .snip { line-height: 1.5; color: var(--text-2); }
</style>
