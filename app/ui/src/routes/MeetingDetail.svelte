<script lang="ts">
  import { onMount } from 'svelte';
  import { api, onEvent } from '../lib/api';
  import { router, navigate, setQuery, href } from '../lib/router.svelte';
  import type { MeetingDetail } from '../lib/types';
  import { duration, fullDate, time, MEETING_STATUS } from '../lib/format';
  import { confirmDialog, promptDialog, toast, toastError } from '../lib/ui.svelte';
  import { meetingCallChip } from '../lib/listening';
  import { prefs } from '../lib/prefs.svelte';
  import Icon from '../lib/components/Icon.svelte';
  import ReportTab from './meeting/ReportTab.svelte';
  import TranscriptTab from './meeting/TranscriptTab.svelte';
  import RequestsTab from './meeting/RequestsTab.svelte';
  import NotesTab from './meeting/NotesTab.svelte';
  import PlannedPanel from './meeting/PlannedPanel.svelte';
  import { i18n, t, tk, tn } from '../lib/i18n.svelte';

  let { id }: { id: string } = $props();

  let detail = $state<MeetingDetail | null>(null);
  let error = $state<string | null>(null);

  const TABS = [
    { id: 'report', label: tk('Compte rendu') },
    { id: 'transcript', label: tk('Transcription') },
    { id: 'requests', label: tk('Demandes') },
    { id: 'notes', label: tk('Notes') },
  ] as const;
  const tab = $derived(TABS.some((x) => x.id === router.route.query.tab) ? router.route.query.tab : 'report');

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
      onEvent('steno://report', (p) => p.meeting_id === id && load()),
      onEvent('steno://meetings', () => load()),
      onEvent('steno://request', (r) => {
        if (!detail || r.meeting_id !== id) return;
        const i = detail.requests.findIndex((x) => x.id === r.id);
        if (i >= 0) detail.requests[i] = r;
        else detail.requests.push(r);
      }),
    ];
    return () => uns.forEach((u) => u());
  });

  const m = $derived(detail?.meeting);
  const callApp = $derived(meetingCallChip(m?.call_app, prefs.callApps));

  // Changement d'onglet : on repart du haut (sauf pour une demande ciblée, qui se place seule).
  let panel: HTMLElement | undefined = $state();
  $effect(() => {
    void tab;
    if (panel && !router.route.query.request) panel.scrollTop = 0;
  });

  async function rename() {
    if (!m) return;
    const title = await promptDialog({ title: t('Renommer la réunion'), value: m.title ?? '', placeholder: t('Titre de la réunion') });
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
      title: t('Marquer comme confidentielle ?'),
      message: t('Aucun appel à l’IA ne sera plus fait pour cette réunion : ni compte rendu, ni corrélation, ni demande.'),
      confirm: t('Marquer confidentielle'),
    }))) return;
    try {
      await api.setConfidential(id, on);
      await load();
      toast(on ? t('Réunion marquée confidentielle.') : t('La réunion n’est plus confidentielle.'));
    } catch (e) {
      toastError(e);
    }
  }

  async function remove() {
    if (!m) return;
    if (!(await confirmDialog({
      title: t('Supprimer cette réunion ?'),
      message: t('« {title} » : transcription, compte rendu, demandes et notes seront supprimés définitivement.', { title: m.title ?? t('Sans titre') }),
      confirm: t('Supprimer'),
      danger: true,
    }))) return;
    try {
      await api.deleteMeeting(id);
      toast(t('Réunion supprimée.'));
      navigate('/meetings');
    } catch (e) {
      toastError(e);
    }
  }

  function tabKey(e: KeyboardEvent) {
    const i = TABS.findIndex((x) => x.id === tab);
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
    void i18n.lang;
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
    <a class="btn" href={href('/meetings')}>{t('Retour à la liste')}</a>
  </div>
{:else if detail && m}
  <div class="detail">
    <header class="dhead">
      <div class="row titlerow">
        <h1 class="grow">{m.title ?? t('Réunion sans titre')}</h1>
        <div class="row tools">
          <button class="btn ghost icon" onclick={rename} title={t('Renommer')} aria-label={t('Renommer la réunion')}><Icon name="pencil" /></button>
          <button class="btn ghost icon" class:on={m.confidential} onclick={toggleConfidential} title={m.confidential ? t('Confidentielle (cliquer pour lever)') : t('Marquer confidentielle')} aria-label={t('Réunion confidentielle')} aria-pressed={m.confidential}><Icon name="lock" /></button>
          <button class="btn ghost icon" onclick={remove} title={t('Supprimer')} aria-label={t('Supprimer la réunion')}><Icon name="trash" /></button>
        </div>
      </div>
      <div class="row meta wrap">
        <span class="mi"><Icon name="calendar" size={13} />{fullDate(m.started_at)}</span>
        <span class="mi tnum"><Icon name="clock" size={13} />{time(m.started_at)}{#if m.ended_at}{' – '}{time(m.ended_at)}{/if}{#if m.status !== 'planned'}<span class="sep">·</span>{duration(m.started_at, m.ended_at)}{/if}</span>
        {#if callApp}<span class="mi app" title={t('Appel {app}', { app: callApp })}><Icon name="phone" size={13} />{callApp}</span>{/if}
        {#if detail.speakers.length}
          <span class="mi"><Icon name="users" size={13} />{tn(detail.speakers.length, '{n} intervenant', '{n} intervenants')}</span>
        {/if}
        {#if m.status === 'live'}<span class="badge live"><span class="dot live" style="width:6px;height:6px"></span>{t('En direct')}</span>
        {:else if m.status !== 'done'}<span class="badge">{t(MEETING_STATUS[m.status])}</span>{/if}
        {#if m.confidential}<span class="badge warn"><Icon name="lock" size={11} /> {t('Confidentielle')}</span>{/if}
        {#if detail.report?.correlated}<span class="badge accent"><Icon name="link2" size={11} /> {t('Corrélée')}</span>{/if}
      </div>
      {#if m.status !== 'planned'}
      <div class="tabs" role="tablist" aria-label={t('Sections de la réunion')} tabindex="-1" onkeydown={tabKey}>
        {#each TABS as tb (tb.id)}
          <button
            id="tab-{tb.id}"
            role="tab"
            aria-selected={tab === tb.id}
            aria-controls="panel"
            tabindex={tab === tb.id ? 0 : -1}
            bind:this={tabEls[tb.id]}
            onclick={() => setQuery({ tab: tb.id, request: null })}
          >
            {t(tb.label)}{#if counts[tb.id]}<span class="cnt">{counts[tb.id]}</span>{/if}
          </button>
        {/each}
        <span class="ind" class:ready={ind.ready} style="transform: translateX({ind.left}px); width: {ind.width}px" aria-hidden="true"></span>
      </div>
      {/if}
    </header>

    {#if m.status === 'planned'}
      <div class="panel scroll"><PlannedPanel {detail} /></div>
    {:else}
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
    {/if}
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
