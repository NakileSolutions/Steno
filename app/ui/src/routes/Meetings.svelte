<script lang="ts">
  import { onMount } from 'svelte';
  import { api, onEvent } from '../lib/api';
  import { router, href } from '../lib/router.svelte';
  import type { MeetingFilter, MeetingSummary } from '../lib/types';
  import { cleanSnippet, debounce, duration, highlight, isoDate, time } from '../lib/format';
  import { groupByDay } from '../lib/group';
  import { meetingCallChip } from '../lib/listening';
  import { prefs } from '../lib/prefs.svelte';
  import { toastError } from '../lib/ui.svelte';
  import Icon from '../lib/components/Icon.svelte';
  import MeetingDetail from './MeetingDetail.svelte';

  let query = $state('');
  let period = $state<'all' | '7' | '30' | 'custom'>('all');
  let from = $state('');
  let to = $state('');
  let correlated = $state<'all' | 'yes' | 'no'>('all');
  let openActions = $state(false);
  let list = $state<MeetingSummary[]>([]);
  let loading = $state(true);
  let showFilters = $state(false);

  const selected = $derived(router.route.parts[1]);
  const activeFilters = $derived((period !== 'all' ? 1 : 0) + (correlated !== 'all' ? 1 : 0) + (openActions ? 1 : 0));

  function filter(): MeetingFilter {
    const f: MeetingFilter = { limit: 200 };
    if (query.trim()) f.query = query.trim();
    if (period === '7' || period === '30') {
      const d = new Date();
      d.setDate(d.getDate() - Number(period));
      f.from = isoDate(d);
    } else if (period === 'custom') {
      if (from) f.from = from;
      if (to) f.to = to;
    }
    if (correlated !== 'all') f.correlated = correlated === 'yes';
    if (openActions) f.openActions = true;
    return f;
  }

  let seq = 0;
  async function load() {
    const my = ++seq;
    try {
      const l = await api.listMeetings(filter());
      if (my === seq) list = l;
    } catch (e) {
      toastError(e);
    } finally {
      loading = false;
    }
  }
  const loadSoon = debounce(load, 180);

  $effect(() => {
    // Recharge à chaque changement de filtre (la requête texte avec anti-rebond).
    void [period, from, to, correlated, openActions];
    load();
  });

  onMount(() => onEvent('scribe://meetings', () => load()));

  const groups = $derived(groupByDay(list));

  function listKeydown(e: KeyboardEvent) {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    const links = [...document.querySelectorAll<HTMLAnchorElement>('.mlist a.mitem')];
    const i = links.indexOf(document.activeElement as HTMLAnchorElement);
    const next = links[e.key === 'ArrowDown' ? Math.min(links.length - 1, i + 1) : Math.max(0, i - 1)];
    if (next) {
      e.preventDefault();
      next.focus();
      next.click();
    }
  }
</script>

<div class="split">
  <section class="listpane" aria-label="Liste des réunions">
    <header class="lhead">
      <div class="row">
        <h1 class="grow">Réunions</h1>
        <button class="btn ghost sm" class:on={showFilters || activeFilters > 0} onclick={() => (showFilters = !showFilters)} aria-expanded={showFilters} aria-controls="mfilters">
          Filtres{#if activeFilters}{' · '}{activeFilters}{/if}
        </button>
      </div>
      <label class="searchbox">
        <Icon name="search" size={14} />
        <input
          data-search
          type="search"
          placeholder="Rechercher dans les réunions"
          title="Plein texte : transcriptions et comptes rendus"
          aria-label="Rechercher dans les réunions"
          bind:value={query}
          oninput={loadSoon}
        />
        <kbd>/</kbd>
      </label>
      {#if showFilters}
        <div class="filters" id="mfilters">
          <div class="frow">
            <span class="faint small">Période</span>
            <select class="input" bind:value={period} aria-label="Période">
              <option value="all">Tout</option>
              <option value="7">7 derniers jours</option>
              <option value="30">30 derniers jours</option>
              <option value="custom">Dates…</option>
            </select>
          </div>
          {#if period === 'custom'}
            <div class="frow">
              <input class="input" type="date" bind:value={from} aria-label="Du" />
              <span class="faint">→</span>
              <input class="input" type="date" bind:value={to} aria-label="Au" />
            </div>
          {/if}
          <div class="frow">
            <span class="faint small">Corrélation</span>
            <select class="input" bind:value={correlated} aria-label="Corrélation">
              <option value="all">Toutes</option>
              <option value="yes">Corrélées</option>
              <option value="no">Non corrélées</option>
            </select>
          </div>
          <label class="check small"><input type="checkbox" bind:checked={openActions} /> Avec des actions ouvertes</label>
        </div>
      {/if}
    </header>

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="mlist scroll" onkeydown={listKeydown}>
      {#if loading && !list.length}
        <div class="skel" aria-hidden="true">
          {#each [0, 1, 2, 3] as k (k)}
            <div class="skel-item"><span class="skeleton" style="width: {68 - k * 9}%"></span><span class="skeleton short"></span></div>
          {/each}
        </div>
      {:else if !list.length}
        <div class="empty">
          <Icon name="meetings" size={24} stroke={1.5} />
          <p>{query || activeFilters ? 'Aucune réunion ne correspond.' : 'Aucune réunion pour l’instant.'}</p>
        </div>
      {/if}
      {#each groups as g (g.day)}
        <h2 class="label day">{g.day}</h2>
        {#each g.items as m (m.id)}
          <a class="mitem" class:sel={selected === m.id} href={href(`/meetings/${m.id}`)} aria-current={selected === m.id ? 'true' : undefined}>
            <div class="row top">
              <span class="title grow ellipsis">{m.title ?? 'Réunion sans titre'}</span>
              {#if m.status === 'live'}<span class="badge live"><span class="dot live" style="width:6px;height:6px"></span>Direct</span>
              {:else}<span class="when">{time(m.started_at)}</span>{/if}
            </div>
            <div class="row meta">
              <span class="tnum">{duration(m.started_at, m.ended_at)}</span>
              {#if m.call_app}{@const app = meetingCallChip(m.call_app, prefs.callApps)}<span class="pill app" title="Appel {app}"><Icon name="phone" size={11} /> {app}</span>{/if}
              {#if m.confidential}<span class="pill" title="Confidentielle"><Icon name="lock" size={11} /> Confidentielle</span>{/if}
              {#if m.correlated}<span class="pill" title="Compte rendu corrélé"><Icon name="link2" size={11} /> Corrélée</span>{/if}
              {#if m.open_actions}<span class="pill acc" title="Actions ouvertes"><Icon name="actions" size={11} /> {m.open_actions} action{m.open_actions > 1 ? 's' : ''}</span>{/if}
              {#if m.status === 'processing'}<span class="pill">Traitement…</span>{/if}
            </div>
            {#if m.snippet}
              <p class="snippet">{@html highlight(cleanSnippet(m.snippet, query), query)}</p>
            {/if}
          </a>
        {/each}
      {/each}
    </div>
  </section>

  <section class="detailpane" aria-label="Détail de la réunion">
    {#if selected}
      {#key selected}
        <MeetingDetail id={selected} />
      {/key}
    {:else}
      <div class="empty full">
        <Icon name="meetings" size={26} stroke={1.5} />
        <h2>Sélectionnez une réunion</h2>
        <p class="small">Utilisez <kbd>/</kbd> pour chercher, <kbd>↑</kbd> <kbd>↓</kbd> pour parcourir la liste.</p>
      </div>
    {/if}
  </section>
</div>

<style>
  .split { display: grid; grid-template-columns: minmax(290px, 340px) 1fr; height: 100%; }
  .listpane { display: flex; flex-direction: column; min-height: 0; border-right: 1px solid var(--line); background: var(--bg); }
  .lhead { padding: 20px 16px 12px; display: flex; flex-direction: column; gap: 12px; }
  .lhead h1 { font-size: var(--fs-h2); }
  .btn.on { background: var(--accent-soft); color: var(--accent-text); }
  .searchbox {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 6px 0 10px;
    border-radius: 9px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    box-shadow: var(--shadow-xs);
    color: var(--text-3);
    transition: border-color var(--t-fast) var(--ease), box-shadow var(--t-fast) var(--ease);
  }
  .searchbox:focus-within { border-color: var(--focus); box-shadow: var(--ring); }
  .searchbox input { flex: 1; min-width: 0; border: 0; outline: 0; background: transparent; color: var(--text); font: inherit; }
  .searchbox input::-webkit-search-cancel-button { opacity: 0.5; }
  .filters { display: flex; flex-direction: column; gap: 8px; padding: 10px 12px; border-radius: 10px; background: var(--surface); border: 1px solid var(--line); animation: appear var(--t) var(--ease); }
  .frow { display: flex; align-items: center; gap: 8px; }
  .frow > span:first-child { width: 76px; flex: none; }
  .frow .input { height: 28px; font-size: 12.5px; }

  .mlist { flex: 1; min-height: 0; padding: 0 10px 16px; border-top: 1px solid var(--line); }
  .day { padding: 16px 8px 6px; }
  .mitem {
    display: block;
    padding: 10px 12px;
    border-radius: 10px;
    text-decoration: none;
    color: inherit;
    margin-bottom: 2px;
    position: relative;
    transition: background var(--t-fast) var(--ease), box-shadow var(--t-fast) var(--ease);
    animation: appear var(--t) var(--ease) both;
  }
  .mitem:hover { background: var(--hover); }
  .mitem.sel {
    background: var(--surface);
    box-shadow: 0 0 0 1px var(--accent-line), var(--shadow-md);
  }
  .mitem.sel::before {
    content: '';
    position: absolute;
    left: 0;
    top: 12px;
    bottom: 12px;
    width: 3px;
    border-radius: 0 3px 3px 0;
    background: var(--accent);
  }
  .mitem:focus-visible { outline-offset: -2px; }
  .title { font-weight: 600; font-size: 13.5px; letter-spacing: -0.005em; }
  .when { font-size: var(--fs-meta); color: var(--text-3); font-variant-numeric: tabular-nums; }
  .top { gap: 8px; }
  .meta { gap: 6px; color: var(--text-3); font-size: var(--fs-meta); margin-top: 4px; flex-wrap: wrap; }
  .meta > span:first-child { margin-right: 2px; }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 19px;
    padding: 0 7px;
    border-radius: var(--r-pill);
    background: var(--surface-2);
    box-shadow: inset 0 0 0 1px var(--line);
    font-size: var(--fs-micro);
    font-weight: 500;
    color: var(--text-2);
  }
  .mitem.sel .pill { background: var(--bg); }
  .pill.acc { background: var(--accent-soft); color: var(--accent-text); box-shadow: none; }
  .snippet {
    margin-top: 6px;
    color: var(--text-2);
    font-size: 12.5px;
    line-height: 1.45;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .skel { padding: 18px 4px; display: flex; flex-direction: column; gap: 18px; }
  .skel-item { padding: 0 8px; }
  .skeleton.short { width: 34%; height: 10px; }

  .detailpane { min-width: 0; min-height: 0; height: 100%; background: var(--surface); }
  .empty.full { height: 100%; }
</style>
