<script lang="ts">
  // Panneau de la barre de menu (~360×520).
  import { onMount } from 'svelte';
  import { api, onEvent, openMain } from '../lib/api';
  import { live, clock, STATE_LABEL } from '../lib/live.svelte';
  import { isListening, teamsOffer } from '../lib/listening';
  import { stopHint } from '../lib/prefs.svelte';
  import type { Request } from '../lib/types';
  import { duration, relDay, STATUS_CLASS, STATUS_LABEL } from '../lib/format';
  import { toastError } from '../lib/ui.svelte';
  import { plainText } from '../lib/markdown';
  import Logo from '../lib/Logo.svelte';
  import Icon from '../lib/components/Icon.svelte';
  import Meter from '../lib/components/Meter.svelte';

  const s = $derived(live.s);
  const m = $derived(s.meeting);
  const active = $derived(isListening(s));
  const offer = $derived(teamsOffer(s));

  let recent = $state<Request[]>([]);
  let query = $state('');
  let busy = $state(false);

  async function loadRecent() {
    try {
      recent = await api.recentRequests(5);
    } catch {}
  }
  onMount(() => {
    loadRecent();
    return onEvent('scribe://request', (r) => {
      const i = recent.findIndex((x) => x.id === r.id);
      if (i >= 0) recent[i] = r;
      else loadRecent();
    });
  });

  async function toggle() {
    busy = true;
    try {
      if (active) await api.stopMeeting();
      else await api.startMeeting();
    } catch (e) {
      toastError(e);
    } finally {
      busy = false;
    }
  }

  async function ask(e: SubmitEvent) {
    e.preventDefault();
    const q = query.trim();
    if (!q) return;
    try {
      await api.ask(q, active ? m?.id : undefined);
      query = '';
      loadRecent();
    } catch (err) {
      toastError(err);
    }
  }

  function openReq(r: Request) {
    openMain(r.meeting_id ? `/meetings/${r.meeting_id}?tab=requests&request=${r.id}` : `/search?q=${encodeURIComponent(r.query)}`);
  }
</script>

<div class="tray">
  <header class="th">
    <span class="tlogo"><Logo size={16} /></span>
    <span class="name">Scribe</span>
    <span class="grow"></span>
    <span class="state {s.state}">
      <span class="dot {s.state === 'listening' ? 'live' : s.state === 'idle' ? '' : s.state}"></span>
      {STATE_LABEL[s.state]}
    </span>
  </header>

  <section class="now">
    {#if m && s.state !== 'idle'}
      <div class="row cur">
        <span class="recdot {s.state}" aria-hidden="true"><span class="dot {s.state === 'listening' ? 'live' : s.state}"></span></span>
        <div class="grow">
          <p class="mt ellipsis">{m.title ?? 'Réunion sans titre'}</p>
          <p class="faint small">{#if s.state === 'processing'}Compte rendu en préparation…{:else}<span class="tnum">{duration(m.started_at, null, clock.now)}</span>{/if}{#if s.pending_requests}{' · '}{s.pending_requests} demande{s.pending_requests > 1 ? 's' : ''} en cours{/if}</p>
        </div>
      </div>

      {#if active}
        <div class="meters">
          <Meter db={s.levels.me} label="Micro" icon="mic" active />
          <Meter db={s.levels.them} label="Teams" icon="speaker" active />
        </div>
        <div class="row ctl">
          <button class="btn danger grow stop" onclick={toggle} disabled={busy}>
            <span class="sq" aria-hidden="true"></span>Arrêter l’écoute
          </button>
          <label class="check small conf" title="Aucun appel à l’IA pour cette réunion">
            <input class="switch" type="checkbox" role="switch" checked={!!m.confidential} onchange={(e) => api.setLiveConfidential(e.currentTarget.checked).catch(toastError)} />
            Confidentielle
          </label>
        </div>
        <p class="stophint tiny"><Icon name="info" size={12} />{stopHint()}</p>
      {/if}
    {:else}
      {#if offer}
        <div class="teams-offer" role="status">
          <span class="to-ic"><Icon name="speaker" size={15} /></span>
          <div class="grow">
            <p class="to-title">Appel Teams détecté : <strong>{offer}</strong></p>
            <p class="to-sub">Le Scribe n’écoute pas encore.</p>
          </div>
        </div>
      {:else}
        <div class="quiet">
          <p class="mt">Le Scribe n’écoute pas</p>
          <p class="faint small">L’écoute ne démarre que sur votre demande.</p>
        </div>
      {/if}
      <button class="btn primary go" onclick={toggle} disabled={busy || s.state === 'processing'}>
        <Icon name="play" size={14} stroke={2} />Démarrer l’écoute
      </button>
    {/if}
  </section>

  <form class="ask" onsubmit={ask}>
    <Icon name="ask" size={15} />
    <input data-search bind:value={query} placeholder="Demander au Scribe…" aria-label="Demande rapide" />
    <kbd>↩</kbd>
  </form>

  <section class="recent">
    <h2 class="label">Dernières réponses</h2>
    <ul>
      {#each recent as r (r.id)}
        <li>
          <button class="ritem" onclick={() => openReq(r)}>
            <span class="row">
              <span class="q grow ellipsis">{r.query}</span>
              {#if r.status !== 'done'}<span class="badge status {STATUS_CLASS[r.status]}">{STATUS_LABEL[r.status]}</span>
              {:else}<span class="faint tiny tnum">{relDay(r.created_at)}</span>{/if}
            </span>
            {#if r.summary || r.error}<span class="sum">{plainText(r.summary) || r.error}</span>{/if}
          </button>
        </li>
      {:else}
        <li class="none">
          <span class="none-ic"><Icon name="ask" size={16} /></span>
          <span class="faint small">Aucune demande récente.</span>
        </li>
      {/each}
    </ul>
  </section>

  <footer class="tf row">
    <button class="btn ghost sm" onclick={() => openMain('/settings')}><Icon name="settings" size={14} />Réglages</button>
    <span class="grow"></span>
    <button class="btn sm" onclick={() => openMain()}>Ouvrir Scribe<Icon name="external" size={12} /></button>
  </footer>
</div>

<style>
  /* Aspect « popover » macOS : en-tête compact, cartes arrondies sur fond neutre.
     (La fenêtre n'est pas transparente : pas de coins arrondis sur le cadre lui-même.) */
  .tray {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--bg);
    overflow: hidden;
  }
  .th {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 42px;
    padding: 0 14px;
    flex: none;
  }
  .tlogo { width: 24px; height: 24px; border-radius: 7px; display: grid; place-items: center; background: var(--side-bg); color: #eef0f6; box-shadow: 0 1px 2px rgba(0, 0, 0, 0.2), inset 0 0 0 1px rgba(255, 255, 255, 0.08); }
  .name { font-weight: 600; font-size: 13.5px; letter-spacing: -0.005em; }
  .state {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 22px;
    padding: 0 9px;
    border-radius: var(--r-pill);
    font-size: var(--fs-micro);
    font-weight: 600;
    color: var(--text-2);
    background: var(--surface-2);
    box-shadow: inset 0 0 0 1px var(--line);
  }
  .state.listening { color: var(--live); background: var(--live-soft); box-shadow: none; }
  .state.processing { color: var(--accent-text); background: var(--accent-soft); box-shadow: none; }
  .state .dot { width: 6px; height: 6px; }
  .now {
    margin: 0 10px;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    border-radius: 12px;
    background: var(--surface);
    border: 1px solid var(--line);
    box-shadow: var(--shadow-md);
  }
  .cur { gap: 10px; }
  .recdot { width: 28px; height: 28px; border-radius: 50%; display: grid; place-items: center; background: var(--live-soft); flex: none; }
  .recdot.processing { background: var(--accent-soft); }
  .recdot.paused { background: var(--warn-soft); }
  .mt { font-weight: 600; font-size: 13.5px; }
  .quiet { display: flex; flex-direction: column; gap: 2px; }
  .meters { display: flex; flex-direction: column; gap: 6px; }
  .ctl { gap: 12px; }
  .ctl .btn { height: 32px; }
  .stop { gap: 8px; }
  .sq { width: 9px; height: 9px; border-radius: 2px; background: currentColor; }
  .conf { flex: none; gap: 7px; color: var(--text-2); }
  .go { height: 40px; font-size: 14px; font-weight: 600; border-radius: 10px; gap: 8px; box-shadow: 0 1px 2px rgba(40, 50, 160, 0.25), 0 4px 14px rgba(79, 94, 232, 0.28), inset 0 1px 0 rgba(255, 255, 255, 0.15); }
  .stophint { display: flex; gap: 6px; align-items: flex-start; color: var(--text-3); line-height: 1.45; margin-top: -2px; }
  .stophint :global(.icon) { margin-top: 1px; flex: none; }
  .ask {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 12px 10px 4px;
    height: 36px;
    padding: 0 8px 0 11px;
    border: 1px solid var(--line-strong);
    border-radius: 10px;
    background: var(--surface);
    color: var(--text-3);
    box-shadow: var(--shadow-xs);
    transition: border-color var(--t-fast) var(--ease), box-shadow var(--t-fast) var(--ease), color var(--t-fast) var(--ease);
  }
  .ask:focus-within { border-color: var(--focus); box-shadow: var(--ring); color: var(--accent-text); }
  .ask input { flex: 1; min-width: 0; border: 0; outline: 0; background: transparent; color: var(--text); font: inherit; }
  .ask input::placeholder { color: var(--text-3); }
  .recent { flex: 1; min-height: 0; overflow-y: auto; padding: 10px 10px 6px; }
  .recent .label { padding: 2px 4px 8px; }
  ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  .ritem {
    all: unset;
    display: flex;
    flex-direction: column;
    gap: 3px;
    width: 100%;
    box-sizing: border-box;
    padding: 9px 11px;
    border-radius: 10px;
    background: var(--surface);
    border: 1px solid var(--line);
    transition: border-color var(--t-fast) var(--ease), box-shadow var(--t-fast) var(--ease);
    animation: appear var(--t) var(--ease) both;
  }
  .ritem:hover { border-color: var(--line-strong); box-shadow: var(--shadow-md); }
  .ritem:focus-visible { outline: 2px solid var(--focus); outline-offset: 1px; }
  .q { font-weight: 600; font-size: 12.5px; }
  .sum {
    color: var(--text-2);
    font-size: var(--fs-meta);
    line-height: 1.45;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .none { display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 22px 8px; }
  .none-ic { width: 34px; height: 34px; border-radius: 10px; display: grid; place-items: center; background: var(--surface); border: 1px solid var(--line); color: var(--text-3); }
  .tf { padding: 8px 10px; border-top: 1px solid var(--line); flex: none; }
</style>
