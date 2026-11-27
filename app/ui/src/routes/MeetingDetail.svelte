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
        <span>{fullDate(m.started_at)}</span>
        <span class="sep">·</span>
        <span>{time(m.started_at)}{#if m.ended_at}{' – '}{time(m.ended_at)}{/if}</span>
        <span class="sep">·</span>
        <span>{duration(m.started_at, m.ended_at)}</span>
        {#if detail.speakers.length}
          <span class="sep">·</span>
          <span>{detail.speakers.length} intervenants</span>
        {/if}
        {#if m.status === 'live'}<span class="badge live">En direct</span>
        {:else if m.status !== 'done'}<span class="badge">{MEETING_STATUS[m.status]}</span>{/if}
        {#if m.confidential}<span class="badge"><Icon name="lock" size={11} /> Confidentielle</span>{/if}
        {#if detail.report?.correlated}<span class="badge"><Icon name="link2" size={11} /> Corrélée</span>{/if}
      </div>
      <div class="tabs" role="tablist" aria-label="Sections de la réunion" tabindex="-1" onkeydown={tabKey}>
        {#each TABS as t (t.id)}
          <button
            id="tab-{t.id}"
            role="tab"
            aria-selected={tab === t.id}
            aria-controls="panel"
            tabindex={tab === t.id ? 0 : -1}
            onclick={() => setQuery({ tab: t.id, request: null })}
          >
            {t.label}{#if counts[t.id]}<span class="cnt">{counts[t.id]}</span>{/if}
          </button>
        {/each}
      </div>
    </header>

    <div class="panel scroll" id="panel" role="tabpanel" aria-labelledby="tab-{tab}" bind:this={panel}>
      {#if tab === 'report'}
        <ReportTab {detail} reload={load} />
      {:else if tab === 'transcript'}
        <TranscriptTab {detail} />
      {:else if tab === 'requests'}
        <RequestsTab {detail} focusId={router.route.query.request} />
      {:else}
        <NotesTab {detail} />
      {/if}
    </div>
  </div>
{/if}

<style>
  .detail { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .dhead { padding: 20px 28px 0; border-bottom: 1px solid var(--line); flex: none; }
  .titlerow { align-items: flex-start; }
  .titlerow h1 { padding-top: 2px; }
  .tools { gap: 2px; color: var(--text-2); }
  .btn.on { color: var(--text); background: var(--selected); }
  .meta { gap: 6px; margin-top: 4px; color: var(--text-2); font-size: 12.5px; }
  .sep { color: var(--text-3); }
  .meta .badge { margin-left: 4px; }
  .tabs { display: flex; gap: 18px; margin-top: 16px; }
  .tabs button {
    position: relative;
    border: 0;
    background: none;
    padding: 0 0 10px;
    font: inherit;
    font-weight: 500;
    color: var(--text-2);
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .tabs button:hover { color: var(--text); }
  .tabs button[aria-selected='true'] { color: var(--text); }
  .tabs button[aria-selected='true']::after {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    bottom: -1px;
    height: 2px;
    border-radius: 2px;
    background: var(--text);
  }
  .cnt { font-size: 11px; color: var(--text-3); font-weight: 500; }
  .panel { flex: 1; min-height: 0; }
  .empty.full { height: 100%; }
</style>
