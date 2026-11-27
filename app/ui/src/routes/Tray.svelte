<script lang="ts">
  // Panneau de la barre de menu (~360×520).
  import { onMount } from 'svelte';
  import { api, onEvent, openMain } from '../lib/api';
  import { live, clock, STATE_LABEL } from '../lib/live.svelte';
  import { callOffer, isListening, meetingCallChip } from '../lib/listening';
  import { prefs, stopHint } from '../lib/prefs.svelte';
  import type { Request } from '../lib/types';
  import { duration, relDay, STATUS_CLASS, STATUS_LABEL } from '../lib/format';
  import { toastError } from '../lib/ui.svelte';
  import { plainText } from '../lib/markdown';
  import Logo from '../lib/Logo.svelte';
  import Icon from '../lib/components/Icon.svelte';
  import Meter from '../lib/components/Meter.svelte';
  import { t, tn } from '../lib/i18n.svelte';

  const s = $derived(live.s);
  const m = $derived(s.meeting);
  const active = $derived(isListening(s));
  const offer = $derived(callOffer(s, prefs.callApps));
  const themLabel = $derived(meetingCallChip(m?.call_app, prefs.callApps) ?? t('Visio'));

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
    let confidential = live.s.meeting?.confidential ?? false;
    const offs = [
      onEvent('steno://request', (r) => {
        const i = recent.findIndex((x) => x.id === r.id);
        if (i >= 0) recent[i] = r;
        else loadRecent();
      }),
      // Réunion passée en confidentielle ou supprimée : ses demandes quittent la liste.
      onEvent('steno://meetings', () => loadRecent()),
      onEvent('steno://live', (st) => {
        const c = st.meeting?.confidential ?? false;
        if (c !== confidential) {
          confidential = c;
          loadRecent();
        }
      }),
    ];
    return () => offs.forEach((off) => off());
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

  async function startRoom() {
    busy = true;
    try {
      await api.startMeeting(undefined, true);
    } catch (e) {
      toastError(e);
    } finally {
      busy = false;
    }
  }

  async function togglePause() {
    busy = true;
    try {
      if (s.state === 'paused') await api.resumeMeeting();
      else await api.pauseMeeting();
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
    openMain(r.meeting_id ? `/meetings/${r.meeting_id}?tab=requests&request=${r.id}` : `/search?request=${r.id}`);
  }
</script>

<div class="tray">
  <header class="th">
    <span class="tlogo"><Logo size={16} /></span>
    <span class="name">Sténo</span>
    <span class="grow"></span>
    <span class="state {s.state}">
      <span class="dot {s.state === 'listening' ? 'live' : s.state === 'idle' ? '' : s.state}"></span>
      {t(STATE_LABEL[s.state])}
    </span>
  </header>

  <section class="now">
    {#if m && s.state !== 'idle'}
      <div class="row cur">
        <span class="recdot {s.state}" aria-hidden="true"><span class="dot {s.state === 'listening' ? 'live' : s.state}"></span></span>
        <div class="grow">
          <p class="mt ellipsis">{m.title ?? t('Réunion sans titre')}</p>
          <p class="faint small">{#if s.state === 'processing'}{t('Compte rendu en préparation…')}{:else}<span class="tnum">{duration(m.started_at, null, clock.now)}</span>{/if}{#if s.pending_requests}{' · '}{tn(s.pending_requests, '{n} demande en cours', '{n} demandes en cours')}{/if}</p>
        </div>
      </div>

      {#if active}
        <div class="meters">
          <Meter db={s.mic_muted ? -90 : s.levels.me} label={s.mic_muted ? t('Micro coupé') : t('Micro')} icon={s.mic_muted ? 'micoff' : 'mic'} active />
          {#if m.call_app !== 'room'}<Meter db={s.levels.them} label={themLabel} icon="speaker" active />{/if}
        </div>
        <div class="row ctl">
          <button class="btn danger grow stop" onclick={toggle} disabled={busy}>
            <span class="sq" aria-hidden="true"></span>{t('Arrêter l’écoute')}
          </button>
          <button
            class="btn sm icon"
            onclick={togglePause}
            disabled={busy}
            title={s.state === 'paused' ? t('Reprendre l’écoute') : t('Mettre l’écoute en pause')}
            aria-label={s.state === 'paused' ? t('Reprendre l’écoute') : t('Mettre l’écoute en pause')}
          ><Icon name={s.state === 'paused' ? 'play' : 'pause'} size={15} /></button>
          <button
            class="btn sm icon mute"
            class:on={s.mic_muted}
            aria-pressed={s.mic_muted}
            onclick={() => api.setMicMuted(!s.mic_muted).catch(toastError)}
            title={s.mic_muted ? t('Rétablir votre micro dans Sténo') : t('Couper votre micro dans Sténo')}
            aria-label={s.mic_muted ? t('Rétablir votre micro dans Sténo') : t('Couper votre micro dans Sténo')}
          ><Icon name={s.mic_muted ? 'micoff' : 'mic'} size={15} /></button>
          <label class="check small conf" title={t('Aucun appel à l’IA pour cette réunion')}>
            <input class="switch" type="checkbox" role="switch" checked={!!m.confidential} onchange={(e) => api.setLiveConfidential(e.currentTarget.checked).catch(toastError)} />
            {t('Confidentielle')}
          </label>
        </div>
        <p class="stophint tiny"><Icon name="info" size={12} />{stopHint()}</p>
      {/if}
    {:else}
      {#if offer}
        <div class="call-offer" role="status">
          <span class="to-ic"><Icon name="phone" size={15} /></span>
          <div class="grow">
            <p class="to-title">{offer.heading}{t(' : ')}<strong>{offer.title}</strong></p>
            <p class="to-sub">{t('Sténo n’écoute pas encore.')}{#if offer.browser}{' '}{t('Tout le son du navigateur sera capté.')}{/if}</p>
          </div>
        </div>
      {:else}
        <div class="quiet">
          <p class="mt">{t('Sténo n’écoute pas')}</p>
          <p class="faint small">{t('L’écoute ne démarre que sur votre demande.')}</p>
        </div>
      {/if}
      <button class="btn primary go" onclick={toggle} disabled={busy || s.state === 'processing'}>
        <Icon name="play" size={14} stroke={2} />{t('Démarrer l’écoute')}
      </button>
      <button class="btn sm ghost room" onclick={startRoom} disabled={busy || s.state === 'processing'} title={t('Sans app de visio : seul le micro de l’ordinateur est écouté')}>
        <Icon name="user" size={14} />{t('Réunion en présentiel')}
      </button>
    {/if}
  </section>

  <form class="ask" onsubmit={ask}>
    <Icon name="ask" size={15} />
    <input data-search bind:value={query} placeholder={t('Demander à Sténo…')} aria-label={t('Demande rapide')} />
    <kbd>↩</kbd>
  </form>

  <section class="recent">
    <h2 class="label">{t('Dernières réponses')}</h2>
    <ul>
      {#each recent as r (r.id)}
        <li>
          <button class="ritem" onclick={() => openReq(r)}>
            <span class="row">
              <span class="q grow ellipsis">{r.query}</span>
              {#if r.status !== 'done'}<span class="badge status {STATUS_CLASS[r.status]}">{t(STATUS_LABEL[r.status])}</span>
              {:else}<span class="faint tiny tnum">{relDay(r.created_at)}</span>{/if}
            </span>
            {#if r.summary || r.error}<span class="sum">{plainText(r.summary) || r.error}</span>{/if}
          </button>
        </li>
      {:else}
        <li class="none">
          <span class="none-ic"><Icon name="ask" size={16} /></span>
          <span class="faint small">{t('Aucune demande récente.')}</span>
        </li>
      {/each}
    </ul>
  </section>

  <footer class="tf row">
    <button class="btn ghost sm" onclick={() => openMain('/settings')}><Icon name="settings" size={14} />{t('Réglages')}</button>
    <span class="grow"></span>
    <button class="btn sm" onclick={() => openMain()}>{t('Ouvrir Sténo')}<Icon name="external" size={12} /></button>
  </footer>
</div>

<style>
  /* Aspect « popover » macOS. Dans Tauri sur macOS, la fenêtre fournit la matière « popover » et les
     coins de 12 px : la racine est transparente et les surfaces légèrement translucides.
     Ailleurs (navigateur, maquette), le panneau est encadré d'un conteneur arrondi façon popover,
     posé sur un fond de bureau, avec un verre simulé. */
  .tray {
    --tray-surface: color-mix(in srgb, var(--surface) 86%, transparent);
    display: flex;
    flex-direction: column;
    height: calc(100vh - 16px);
    margin: 8px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--bg) 80%, transparent);
    -webkit-backdrop-filter: blur(30px) saturate(180%);
    backdrop-filter: blur(30px) saturate(180%);
    box-shadow:
      0 0 0 0.5px rgba(0, 0, 0, 0.22),
      inset 0 0 0 1px rgba(255, 255, 255, 0.45),
      0 2px 6px rgba(20, 22, 40, 0.1),
      0 18px 44px -8px rgba(20, 22, 40, 0.35);
    overflow: hidden;
  }
  @media (prefers-color-scheme: dark) {
    .tray {
      background: color-mix(in srgb, var(--bg) 78%, transparent);
      box-shadow:
        0 0 0 0.5px rgba(0, 0, 0, 0.7),
        inset 0 0 0 1px rgba(255, 255, 255, 0.09),
        0 2px 6px rgba(0, 0, 0, 0.3),
        0 18px 44px -8px rgba(0, 0, 0, 0.6);
    }
  }
  :global(html.native-vibrancy) .tray {
    --tray-surface: color-mix(in srgb, var(--surface) 62%, transparent);
    height: 100vh;
    margin: 0;
    border-radius: 0;
    background: transparent;
    -webkit-backdrop-filter: none;
    backdrop-filter: none;
    box-shadow: none;
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
    background: var(--tray-surface);
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
    background: var(--tray-surface);
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
    background: var(--tray-surface);
    border: 1px solid var(--line);
    transition: border-color var(--t-fast) var(--ease), box-shadow var(--t-fast) var(--ease);
    animation: appear var(--t) var(--ease) both;
  }
  .ritem:hover { border-color: var(--line-strong); box-shadow: var(--shadow-md); background: var(--surface); }
  .ritem:active { transform: scale(0.99); }
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
  .mute.on { background: var(--err-soft); color: var(--err); border-color: transparent; }
  .room { gap: 6px; align-self: center; margin-top: 4px; }
</style>
