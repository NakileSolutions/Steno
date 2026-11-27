<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { api, onEvent } from '../lib/api';
  import { router, href, setQuery } from '../lib/router.svelte';
  import type { Request, SearchHit } from '../lib/types';
  import { cleanSnippet, debounce, highlight, shortDate, time, dayLabel } from '../lib/format';
  import { toastError } from '../lib/ui.svelte';
  import Icon from '../lib/components/Icon.svelte';
  import RequestCard from '../lib/components/RequestCard.svelte';
  import { groupHits } from '../lib/group';
  import { t, tn } from '../lib/i18n.svelte';

  let query = $state(router.route.query.q ?? '');
  let hits = $state<SearchHit[]>([]);
  let searched = $state('');
  let aiReq = $state<Request | null>(null);
  let input: HTMLInputElement | undefined = $state();

  async function run() {
    // Recherche différée arrivée après avoir quitté la page : ne pas toucher à l'URL.
    if (router.route.path !== '/search') return;
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

  // `/search?request=<id>` (menu bar menu, quick panel): answer to a request made outside a
  // meeting. Followed even when the page is already open.
  async function showRequest(id: string) {
    try {
      const r = (await api.recentRequests(200)).find((x) => x.id === id);
      if (r && router.route.query.request === id) aiReq = r;
    } catch (e) {
      toastError(e);
    }
  }
  $effect(() => {
    const id = router.route.query.request;
    if (id) untrack(() => showRequest(id));
  });

  onMount(() => {
    if (query) run();
    input?.focus();
    return onEvent('steno://request', (r) => {
      if (aiReq && r.id === aiReq.id) aiReq = r;
    });
  });

  function closeAi() {
    aiReq = null;
    if (router.route.query.request) setQuery({ request: null });
  }

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
    <header class="head">
      <h1>{t('Recherche')}</h1>
      <p class="muted small">{t('Transcriptions et comptes rendus de toutes vos réunions.')}</p>
    </header>
    <form class="bar row" onsubmit={(e) => { e.preventDefault(); run(); }}>
      <label class="searchbox grow">
        <Icon name="search" size={17} />
        <input
          bind:this={input}
          data-search
          type="search"
          bind:value={query}
          oninput={runSoon}
          placeholder={t('Mot, ticket (APP-1234), service, personne…')}
          aria-label={t('Rechercher dans toutes les réunions')}
        />
        <kbd>/</kbd>
      </label>
      <button type="button" class="btn aibtn" onclick={askAi} disabled={!query.trim()} title={t('Poser la question à l’agent IA, qui lit l’historique via le serveur MCP Sténo')}>
        <Icon name="ask" size={15} />{t('Demander à l’IA')}
      </button>
    </form>

    {#if aiReq}
      <section class="ai">
        <div class="row"><h2 class="label grow">{t('Réponse de l’IA')}</h2><button class="btn ghost sm icon" onclick={closeAi} aria-label={t('Fermer la réponse')}><Icon name="x" size={13} /></button></div>
        <RequestCard req={aiReq} />
      </section>
    {/if}

    {#if searched}
      <p class="faint small count tnum">{tn(hits.length, '{n} résultat', '{n} résultats')} {tn(groups.length, 'dans {n} réunion', 'dans {n} réunions')}</p>
    {/if}

    {#each groups as g (g.id)}
      <section class="group">
        <a class="gtitle row" href={href(`/meetings/${g.id}`)}>
          <span class="gic" aria-hidden="true"><Icon name="meetings" size={14} /></span>
          <span class="grow ellipsis">{g.title ?? t('Réunion sans titre')}</span>
          <span class="faint small tnum">{dayLabel(g.started_at)} · {time(g.started_at)}</span>
          <Icon name="chevron" size={13} />
        </a>
        <ul>
          {#each g.hits as h, i (i)}
            <li>
              <a class="hit" href={href(`/meetings/${g.id}?tab=${h.kind === 'report' ? 'report' : 'transcript'}`)}>
                <span class="kind" class:rep={h.kind === 'report'}>{h.kind === 'report' ? t('Compte rendu') : h.ts ? time(h.ts) : t('Transcription')}</span>
                <span class="snip">{@html highlight(cleanSnippet(h.snippet, searched), searched)}</span>
              </a>
            </li>
          {/each}
        </ul>
      </section>
    {:else}
      {#if searched}
        <div class="empty">
          <Icon name="search" size={24} stroke={1.5} />
          <p>{t('Aucun résultat pour « {q} ».', { q: searched })}</p>
          <p class="small">{t('Essayez « Demander à l’IA » pour une question formulée.')}</p>
        </div>
      {:else}
        <div class="empty">
          <Icon name="search" size={24} stroke={1.5} />
          <p>{t('Recherche plein texte dans toutes les transcriptions et tous les comptes rendus.')}</p>
          <p class="small">{t('Les phrases privées ne sont jamais indexées dans les résultats.')}</p>
        </div>
      {/if}
    {/each}
    {#if groups.length}<p class="faint tiny tnum">{t('Dates : {from} → {to}', { from: shortDate(groups.at(-1)!.started_at), to: shortDate(groups[0].started_at) })}</p>{/if}
  </div>
</div>

<style>
  .page { height: 100%; }
  .inner { max-width: 860px; padding: 32px 40px 56px; }
  .head { display: flex; flex-direction: column; gap: 4px; }
  .bar { gap: 10px; margin: 18px 0 22px; }
  .searchbox {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 44px;
    padding: 0 10px 0 14px;
    border-radius: 12px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    box-shadow: var(--shadow-md);
    color: var(--text-3);
    transition: border-color var(--t-fast) var(--ease), box-shadow var(--t-fast) var(--ease), color var(--t-fast) var(--ease);
  }
  .searchbox:focus-within { border-color: var(--focus); box-shadow: var(--ring), var(--shadow-md); color: var(--accent-text); }
  .searchbox input { flex: 1; border: 0; outline: 0; background: transparent; font: inherit; font-size: 15px; color: var(--text); }
  .searchbox input::placeholder { color: var(--text-3); }
  .aibtn { height: 44px; padding: 0 16px; border-radius: 12px; background: var(--accent-soft); color: var(--accent-text); border-color: transparent; box-shadow: none; }
  .aibtn:hover:not(:disabled) { background: var(--accent-soft-2); border-color: transparent; }
  .ai { margin-bottom: 24px; animation: appear var(--t) var(--ease); }
  .ai .label { margin-bottom: 8px; }
  .count { margin-bottom: 10px; }
  .group {
    margin-bottom: 14px;
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    background: var(--surface);
    box-shadow: var(--shadow-sm);
    overflow: hidden;
    animation: appear var(--t) var(--ease) both;
  }
  .gtitle { padding: 11px 14px; font-weight: 600; text-decoration: none; color: inherit; border-bottom: 1px solid var(--line); gap: 10px; transition: background var(--t-fast) var(--ease); }
  .gtitle > :global(.icon:last-child) { color: var(--text-3); }
  .gtitle:hover { background: var(--hover); }
  .gic { width: 26px; height: 26px; border-radius: 8px; display: grid; place-items: center; background: var(--accent-soft); color: var(--accent-text); flex: none; }
  ul { list-style: none; margin: 0; padding: 6px 0; }
  .hit { display: grid; grid-template-columns: 104px 1fr; gap: 12px; padding: 7px 14px; text-decoration: none; color: inherit; transition: background var(--t-fast) var(--ease); }
  .hit:hover { background: var(--hover); }
  .kind {
    justify-self: start;
    height: 20px;
    padding: 0 8px;
    border-radius: var(--r-pill);
    display: inline-flex;
    align-items: center;
    font-size: var(--fs-micro);
    font-weight: 500;
    color: var(--text-2);
    background: var(--surface-2);
    font-variant-numeric: tabular-nums;
    margin-top: 1px;
  }
  .kind.rep { background: var(--accent-soft); color: var(--accent-text); }
  .snip { line-height: 1.55; color: var(--text-2); font-size: 13.5px; }
</style>
