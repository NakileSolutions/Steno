<script lang="ts">
  import { onMount } from 'svelte';
  import { api, onEvent } from '../lib/api';
  import { router, navigate, setQuery, href } from '../lib/router.svelte';
  import type { MeetingDetail } from '../lib/types';
  import { duration, fullDate, time, MEETING_STATUS } from '../lib/format';
  import { confirmDialog, promptDialog, toast, toastError } from '../lib/ui.svelte';
  import Icon from '../lib/components/Icon.svelte';
  import ReportTab from './meeting/ReportTab.svelte';
  import TranscriptTab from './meeting/TranscriptTab.svelte';
  import RequestsTab from './meeting/RequestsTab.svelte';
  import NotesTab from './meeting/NotesTab.svelte';

  let { id }: { id: string } = $props();

  let detail = $state<MeetingDetail | null>(null);
  let error = $state<string | null>(null);

  const TABS = [
    { id: 'report', label: 'Compte rendu' },
    { id: 'transcript', label: 'Transcription' },
    { id: 'requests', label: 'Demandes' },
    { id: 'notes', label: 'Notes' },
  ] as const;
  const tab = $derived(TABS.some((t) => t.id === router.route.query.tab) ? router.route.query.tab : 'report');

  async function load() {
    try {
      detail = await api.getMeeting(id);
      error = null;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  onMount(() => {
    load();
    const uns = [
      onEvent('scribe://report', (p) => p.meeting_id === id && load()),
      onEvent('scribe://meetings', () => load()),
      onEvent('scribe://request', (r) => {
        if (!detail || r.meeting_id !== id) return;
        const i = detail.requests.findIndex((x) => x.id === r.id);
        if (i >= 0) detail.requests[i] = r;
        else detail.requests.push(r);
      }),
    ];
    return () => uns.forEach((u) => u());
  });

  const m = $derived(detail?.meeting);

  // Changement d'onglet : on repart du haut (sauf pour une demande ciblée, qui se place seule).
  let panel: HTMLElement | undefined = $state();
  $effect(() => {
    void tab;
    if (panel && !router.route.query.request) panel.scrollTop = 0;
  });

  async function rename() {
    if (!m) return;
    const title = await promptDialog({ title: 'Renommer la réunion', value: m.title ?? '', placeholder: 'Titre de la réunion' });
    if (title === null || !title.trim()) return;
    try {
      await api.renameMeeting(id, title.trim());
      await load();
    } catch (e) {
      toastError(e);
    }
  }

  async function toggleConfidential() {
    if (!m) return;
    const on = !m.confidential;
    if (on && !(await confirmDialog({
      title: 'Marquer comme confidentielle ?',
      message: 'Aucun appel à l’IA ne sera plus fait pour cette réunion : ni compte rendu, ni corrélation, ni demande.',
      confirm: 'Marquer confidentielle',
    }))) return;
    try {
      await api.setConfidential(id, on);
      await load();
      toast(on ? 'Réunion marquée confidentielle.' : 'La réunion n’est plus confidentielle.');
    } catch (e) {
      toastError(e);
    }
  }

  async function remove() {
    if (!m) return;
    if (!(await confirmDialog({
      title: 'Supprimer cette réunion ?',
      message: `« ${m.title ?? 'Sans titre'} » : transcription, compte rendu, demandes et notes seront supprimés définitivement.`,
      confirm: 'Supprimer',
      danger: true,
    }))) return;
    try {
      await api.deleteMeeting(id);
      toast('Réunion supprimée.');
      navigate('/meetings');
    } catch (e) {
      toastError(e);
    }
  }

  function tabKey(e: KeyboardEvent) {
    const i = TABS.findIndex((t) => t.id === tab);
    let j = -1;
    if (e.key === 'ArrowRight') j = (i + 1) % TABS.length;
    else if (e.key === 'ArrowLeft') j = (i - 1 + TABS.length) % TABS.length;
    if (j < 0) return;
    e.preventDefault();
    setQuery({ tab: TABS[j].id, request: null });
    document.getElementById(`tab-${TABS[j].id}`)?.focus();
  }

  // Indicateur d'onglet glissant.
  let tabEls = $state<Record<string, HTMLButtonElement>>({});
  let ind = $state({ left: 0, width: 0, ready: false });
  $effect(() => {
    void counts[tab];
    const el = tabEls[tab];
    if (el) ind = { left: el.offsetLeft, width: el.offsetWidth, ready: true };
  });

  const counts = $derived({
    report: detail?.report_versions.length ? `v${detail.report?.version}` : '',
    transcript: '',
    requests: detail?.requests.length ? String(detail.requests.length) : '',
    notes: detail?.notes.length ? String(detail.notes.length) : '',
  } as Record<string, string>);
</script>

{#if error}
  <div class="empty full">
    <Icon name="warn" size={28} />
    <h2>{error}</h2>
    <a class="btn" href={href('/meetings')}>Retour à la liste</a>
  </div>
{:else if detail && m}
  <div class="detail">
    <header class="dhead">
      <div class="row titlerow">
        <h1 class="grow">{m.title ?? 'Réunion sans titre'}</h1>
        <div class="row tools">
          <button class="btn ghost icon" onclick={rename} title="Renommer" aria-label="Renommer la réunion"><Icon name="pencil" /></button>
          <button class="btn ghost icon" class:on={m.confidential} onclick={toggleConfidential} title={m.confidential ? 'Confidentielle (cliquer pour lever)' : 'Marquer confidentielle'} aria-label="Réunion confidentielle" aria-pressed={m.confidential}><Icon name="lock" /></button>
          <button class="btn ghost icon" onclick={remove} title="Supprimer" aria-label="Supprimer la réunion"><Icon name="trash" /></button>
        </div>
      </div>
      <div class="row meta wrap">
        <span class="mi"><Icon name="calendar" size={13} />{fullDate(m.started_at)}</span>
        <span class="mi tnum"><Icon name="clock" size={13} />{time(m.started_at)}{#if m.ended_at}{' – '}{time(m.ended_at)}{/if}<span class="sep">·</span>{duration(m.started_at, m.ended_at)}</span>
        {#if detail.speakers.length}
          <span class="mi"><Icon name="users" size={13} />{detail.speakers.length} intervenants</span>
        {/if}
        {#if m.status === 'live'}<span class="badge live"><span class="dot live" style="width:6px;height:6px"></span>En direct</span>
        {:else if m.status !== 'done'}<span class="badge">{MEETING_STATUS[m.status]}</span>{/if}
        {#if m.confidential}<span class="badge warn"><Icon name="lock" size={11} /> Confidentielle</span>{/if}
        {#if detail.report?.correlated}<span class="badge accent"><Icon name="link2" size={11} /> Corrélée</span>{/if}
      </div>
      <div class="tabs" role="tablist" aria-label="Sections de la réunion" tabindex="-1" onkeydown={tabKey}>
        {#each TABS as t (t.id)}
          <button
            id="tab-{t.id}"
            role="tab"
            aria-selected={tab === t.id}
            aria-controls="panel"
            tabindex={tab === t.id ? 0 : -1}
            bind:this={tabEls[t.id]}
            onclick={() => setQuery({ tab: t.id, request: null })}
          >
            {t.label}{#if counts[t.id]}<span class="cnt">{counts[t.id]}</span>{/if}
          </button>
        {/each}
        <span class="ind" class:ready={ind.ready} style="transform: translateX({ind.left}px); width: {ind.width}px" aria-hidden="true"></span>
      </div>
    </header>

    {#key tab}<div class="panel scroll" id="panel" role="tabpanel" aria-labelledby="tab-{tab}" bind:this={panel}>
      {#if tab === 'report'}
        <ReportTab {detail} reload={load} />
      {:else if tab === 'transcript'}
        <TranscriptTab {detail} />
      {:else if tab === 'requests'}
        <RequestsTab {detail} focusId={router.route.query.request} />
      {:else}
        <NotesTab {detail} />
      {/if}
    </div>{/key}
  </div>
{/if}

<style>
  .detail { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .dhead { padding: 24px 32px 0; border-bottom: 1px solid var(--line); flex: none; }
  .titlerow { align-items: flex-start; }
  .titlerow h1 { padding-top: 1px; }
  .tools { gap: 2px; color: var(--text-2); }
  .btn.on { color: var(--warn); background: var(--warn-soft); }
  .meta { gap: 6px 14px; margin-top: 8px; color: var(--text-2); font-size: 12.5px; }
  .mi { display: inline-flex; align-items: center; gap: 6px; }
  .mi :global(.icon) { color: var(--text-3); }
  .sep { color: var(--text-3); margin: 0 6px; }
  .tabs { position: relative; display: flex; gap: 4px; margin-top: 18px; margin-left: -10px; }
  .tabs button {
    position: relative;
    border: 0;
    background: none;
    height: 36px;
    padding: 0 10px 2px;
    font: inherit;
    font-weight: 500;
    color: var(--text-2);
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border-radius: 7px 7px 0 0;
    transition: color var(--t-fast) var(--ease);
  }
  .tabs button:hover { color: var(--text); }
  .tabs button:focus-visible { outline-offset: -2px; border-radius: 7px; }
  .tabs button[aria-selected='true'] { color: var(--text); }
  .ind {
    position: absolute;
    left: 0;
    bottom: -1px;
    height: 2px;
    border-radius: 2px;
    background: var(--accent);
    opacity: 0;
    pointer-events: none;
  }
  .ind.ready { opacity: 1; transition: transform 180ms var(--ease), width 180ms var(--ease); }
  .cnt {
    min-width: 18px;
    height: 17px;
    padding: 0 5px;
    border-radius: var(--r-pill);
    background: var(--surface-2);
    font-size: var(--fs-micro);
    color: var(--text-3);
    font-weight: 600;
    display: inline-grid;
    place-items: center;
    font-variant-numeric: tabular-nums;
  }
  .tabs button[aria-selected='true'] .cnt { background: var(--accent-soft); color: var(--accent-text); }
  .panel { flex: 1; min-height: 0; animation: appear var(--t) var(--ease); }
  .empty.full { height: 100%; }
</style>
