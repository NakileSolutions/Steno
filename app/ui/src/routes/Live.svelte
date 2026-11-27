<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { api, onEvent } from '../lib/api';
  import { live, clock, STATE_LABEL } from '../lib/live.svelte';
  import { callOffer, isListening, meetingCallChip } from '../lib/listening';
  import { prefs, stopHint } from '../lib/prefs.svelte';
  import { href } from '../lib/router.svelte';
  import type { Request, Segment, Speaker } from '../lib/types';
  import { duration, timeS } from '../lib/format';
  import { toastError } from '../lib/ui.svelte';
  import { initial, speakerColor, speakerName } from '../lib/components/speakers';
  import Logo from '../lib/Logo.svelte';
  import Icon from '../lib/components/Icon.svelte';
  import Meter from '../lib/components/Meter.svelte';
  import RequestCard from '../lib/components/RequestCard.svelte';

  const s = $derived(live.s);
  const meeting = $derived(s.meeting);
  const active = $derived(isListening(s));
  const offer = $derived(callOffer(s, prefs.callApps));
  /** Nom du canal « eux » : l'app de l'appel en cours. */
  const themLabel = $derived(meetingCallChip(meeting?.call_app, prefs.callApps) ?? 'Visio');

  let segments = $state<Segment[]>([]);
  let speakers = $state<Speaker[]>([]);
  let requests = $state<Request[]>([]);
  let loadedFor = $state<string | null>(null);
  let scroller: HTMLElement | undefined = $state();
  let stick = $state(true);
  let unseen = $state(0);
  let query = $state('');
  let newTitle = $state('');
  let busy = $state(false);

  async function loadMeeting(id: string) {
    loadedFor = id;
    try {
      const [segs, d] = await Promise.all([api.getTranscript(id, true), api.getMeeting(id)]);
      segments = segs;
      speakers = d.speakers;
      requests = d.requests;
      stick = true;
      await tick();
      scrollBottom(false);
    } catch (e) {
      toastError(e);
    }
  }

  $effect(() => {
    const id = meeting?.id ?? null;
    if (id && id !== loadedFor) loadMeeting(id);
  });

  onMount(() => {
    const uns = [
      onEvent('scribe://segment', async (seg) => {
        if (seg.meeting_id !== meeting?.id) return;
        // Une phrase déjà affichée peut revenir mise à jour (rattachée à une demande, écho…).
        const i = segments.findIndex((x) => x.id === seg.id);
        if (i >= 0) {
          segments[i] = seg;
          return;
        }
        segments.push(seg);
        if (stick) {
          await tick();
          scrollBottom(true);
        } else unseen++;
      }),
      onEvent('scribe://request', (r) => {
        if (r.meeting_id !== meeting?.id) return;
        const i = requests.findIndex((x) => x.id === r.id);
        if (i >= 0) requests[i] = r;
        else requests.push(r);
      }),
    ];
    return () => uns.forEach((u) => u());
  });

  function scrollBottom(smooth: boolean) {
    scroller?.scrollTo({ top: scroller.scrollHeight, behavior: smooth ? 'smooth' : 'auto' });
    unseen = 0;
  }
  function onscroll() {
    if (!scroller) return;
    stick = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 48;
    if (stick) unseen = 0;
  }

  const visible = $derived(segments.filter((x) => !x.is_echo));
  const feed = $derived([...requests].sort((a, b) => b.created_at.localeCompare(a.created_at)));

  async function ask(e: SubmitEvent) {
    e.preventDefault();
    const q = query.trim();
    if (!q) return;
    try {
      const r = await api.ask(q, meeting?.id);
      if (!requests.some((x) => x.id === r.id)) requests.push(r);
      query = '';
    } catch (err) {
      toastError(err);
    }
  }

  async function start() {
    busy = true;
    try {
      await api.startMeeting(newTitle.trim() || undefined);
      newTitle = '';
    } catch (e) {
      toastError(e);
    } finally {
      busy = false;
    }
  }
  async function stop() {
    busy = true;
    try {
      await api.stopMeeting();
    } catch (e) {
      toastError(e);
    } finally {
      busy = false;
    }
  }
  async function setConf(on: boolean) {
    try {
      await api.setLiveConfidential(on);
    } catch (e) {
      toastError(e);
    }
  }

  // Aparté : maintenir le bouton (souris ou Espace).
  let holding = false;
  function asideOn() {
    if (holding) return;
    holding = true;
    api.setAside(true).catch(toastError);
  }
  function asideOff() {
    if (!holding) return;
    holding = false;
    api.setAside(false).catch(toastError);
  }
</script>

{#if meeting && s.state !== 'idle'}
  <div class="live">
    <header class="lbar">
      <div class="row ttl">
        <span class="recdot {s.state}" aria-hidden="true"><span class="dot {s.state === 'listening' ? 'live' : s.state}"></span></span>
        <div class="grow">
          <h1 class="ellipsis">{meeting.title ?? 'Réunion sans titre'}</h1>
          <p class="sub">
            <span class="st {s.state}">{STATE_LABEL[s.state]}</span>
            <span class="tnum">{duration(meeting.started_at, meeting.ended_at, clock.now)}</span>
            {#if s.pending_requests}<span class="pend"><span class="spin" aria-hidden="true"></span>{s.pending_requests} demande{s.pending_requests > 1 ? 's' : ''} en cours</span>{/if}
            {#if s.sidecar !== 'running'}<span class="warn">Audio : {s.sidecar}</span>{/if}
          </p>
        </div>
        {#if s.state !== 'processing'}
          <label class="check conf" title="Aucun appel à l’IA pour cette réunion">
            <Icon name="lock" size={14} /> Confidentielle
            <input class="switch" type="checkbox" role="switch" checked={meeting.confidential} onchange={(e) => setConf(e.currentTarget.checked)} />
          </label>
          <button class="btn danger stopbtn" onclick={stop} disabled={busy}><span class="sq" aria-hidden="true"></span>Arrêter l’écoute</button>
        {/if}
      </div>
      {#if active}
        <div class="row ctl">
          <div class="meters">
            <Meter db={s.levels.me} label="Micro" icon="mic" />
            <Meter db={s.levels.them} label={themLabel} icon="speaker" />
          </div>
          <button
            class="btn aside"
            class:on={s.aside}
            aria-pressed={s.aside}
            title="Maintenir pour parler en aparté : vos phrases sont privées et exclues du compte rendu"
            onpointerdown={(e) => { e.currentTarget.setPointerCapture(e.pointerId); asideOn(); }}
            onpointerup={asideOff}
            onpointercancel={asideOff}
            onlostpointercapture={asideOff}
            onkeydown={(e) => { if ((e.key === ' ' || e.key === 'Enter') && !e.repeat) { e.preventDefault(); asideOn(); } }}
            onkeyup={(e) => { if (e.key === ' ' || e.key === 'Enter') { e.preventDefault(); asideOff(); } }}
            onblur={asideOff}
          >
            <Icon name="hand" size={15} />
            {s.aside ? 'Aparté en cours…' : 'Maintenir pour un aparté'}
          </button>
        </div>
      {/if}
      {#if active}<p class="stophint"><Icon name="info" size={13} />{stopHint()}</p>{/if}
      {#if s.last_error}<p class="err small"><Icon name="warn" size={13} />{s.last_error}</p>{/if}
    </header>

    <div class="body">
      <section class="tx" aria-label="Transcription en direct">
        <div class="scroll txs" bind:this={scroller} {onscroll} aria-live="off">
          {#if !visible.length}
            <div class="waiting">
              <span class="eq" aria-hidden="true"><i></i><i></i><i></i><i></i><i></i></span>
              <p>En attente des premières phrases…</p>
            </div>
          {/if}
          <ol class="segs">
            {#each visible as seg, i (seg.id)}
              {@const cont = i > 0 && visible[i - 1].speaker === seg.speaker}
              {@const name = speakerName(seg.speaker, speakers)}
              <li class="seg" class:cont class:private={seg.private} class:cmd={!!seg.request_id} style="--c: {speakerColor(seg.speaker)}">
                <time class="ts">{timeS(seg.ts)}</time>
                <span class="av">{#if !cont}<span class="avatar" aria-hidden="true">{initial(name)}</span>{/if}</span>
                <div class="body-c">
                  {#if !cont}<span class="who">{name}</span>{/if}
                  <p class="text">
                    {#if seg.private}<span class="priv"><Icon name="lock" size={11} /> Aparté</span>{/if}
                    {seg.text}
                    {#if seg.request_id}<span class="badge accent cmdtag"><Icon name="ask" size={11} /> Demande</span>{/if}
                  </p>
                </div>
              </li>
            {/each}
          </ol>
        </div>
        {#if !stick}
          <button class="jump" onclick={() => { stick = true; scrollBottom(true); }}>
            <Icon name="arrowdown" size={13} stroke={2} /> Revenir au direct{#if unseen}<span class="new">{unseen} nouvelle{unseen > 1 ? 's' : ''}</span>{/if}
          </button>
        {/if}
      </section>

      <aside class="side" aria-label="Demandes au Scribe">
        <form class="ask" onsubmit={ask}>
          <label class="label" for="live-ask">Demander au Scribe</label>
          <div class="askbox">
            <Icon name="ask" size={15} />
            <input id="live-ask" bind:value={query} placeholder={meeting.confidential ? 'Confidentielle : notes seulement' : 'Votre question…'} autocomplete="off" />
            <button class="btn primary sm" disabled={!query.trim()}>Envoyer</button>
          </div>
          <p class="faint tiny">Ou à voix haute : « Scribe, cherche… », « Scribe, note… »</p>
        </form>
        <div class="feed scroll">
          {#each feed as r (r.id)}
            <RequestCard req={r} compact />
          {:else}
            <div class="none">
              <span class="none-ic"><Icon name="ask" size={18} /></span>
              <p class="faint small">Aucune demande pour l’instant.</p>
            </div>
          {/each}
        </div>
      </aside>
    </div>
  </div>
{:else}
  <div class="idle">
    <div class="hero card">
      {#if s.state === 'processing'}
        <span class="hic proc"><Logo size={34} tile /></span>
        <h2>Compte rendu en préparation…</h2>
        <p class="lead">L’écoute est arrêtée. Le compte rendu apparaîtra dans « Réunions ».</p>
        <div class="progress" aria-hidden="true"><span></span></div>
      {:else if offer}
        <span class="hic"><Logo size={56} tile /></span>
        <div class="call-offer big" role="status">
          <span class="to-ic"><Icon name="phone" size={18} /></span>
          <div class="grow">
            <p class="to-title">{offer.heading} : <strong>{offer.title}</strong></p>
            <p class="to-sub">Le Scribe n’écoute pas encore : rien n’est transcrit tant que vous ne l’avez pas décidé.{#if offer.browser} Attention : tout le son du navigateur sera capté, y compris les autres onglets.{/if}</p>
          </div>
        </div>
        <button class="btn primary go" onclick={start} disabled={busy}><Icon name="play" size={15} stroke={2} />Démarrer l’écoute</button>
      {:else}
        <span class="hic"><Logo size={56} tile /></span>
        <h2>Le Scribe n’écoute pas</h2>
        <p class="lead">L’écoute démarre et s’arrête uniquement sur votre demande.</p>
        <form class="start" onsubmit={(e) => { e.preventDefault(); start(); }}>
          <input class="input lg" bind:value={newTitle} placeholder="Titre de la réunion (facultatif)" aria-label="Titre de la réunion" />
          <button class="btn primary go" disabled={busy}><Icon name="play" size={15} stroke={2} />Démarrer l’écoute</button>
        </form>
      {/if}
      {#if s.state !== 'processing'}
        <p class="tiny faint howto">Pour arrêter ensuite : le bouton « Arrêter l’écoute », le raccourci, le menu de la barre d’état, ou à la voix.</p>
      {/if}
    </div>
    <a class="past small" href={href('/meetings')}>Voir les réunions passées <Icon name="chevron" size={13} /></a>
  </div>
{/if}

<style>
  .live { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .lbar { padding: 18px 28px 14px; border-bottom: 1px solid var(--line); background: var(--surface); flex: none; display: flex; flex-direction: column; gap: 14px; }
  .ttl { gap: 14px; }
  .recdot { width: 34px; height: 34px; border-radius: 50%; display: grid; place-items: center; background: var(--live-soft); flex: none; }
  .recdot.paused { background: var(--warn-soft); }
  .recdot.processing { background: var(--accent-soft); }
  .recdot .dot { width: 10px; height: 10px; }
  .ttl h1 { font-size: var(--fs-h2); letter-spacing: -0.015em; }
  .sub { display: flex; align-items: center; gap: 8px; margin-top: 2px; font-size: var(--fs-meta); color: var(--text-2); }
  .sub > * + *::before { content: '·'; color: var(--text-3); margin-right: 8px; }
  .st { font-weight: 600; }
  .st.listening { color: var(--live); }
  .pend { display: inline-flex; align-items: center; gap: 6px; }
  .pend::before { margin-right: 2px !important; }
  .warn { color: var(--warn); }
  .err { color: var(--err); display: flex; align-items: center; gap: 6px; }
  .conf { gap: 8px; font-size: var(--fs-ui); color: var(--text-2); padding: 0 6px; height: 30px; }
  .conf :global(.icon) { color: var(--text-3); }
  .stopbtn { gap: 8px; }
  .sq { width: 9px; height: 9px; border-radius: 2px; background: currentColor; }
  .ctl { gap: 20px; }
  .meters { display: grid; grid-template-columns: 1fr 1fr; gap: 28px; flex: 1; max-width: 560px; }
  .aside { touch-action: none; user-select: none; margin-left: auto; }
  .aside.on { background: var(--accent); color: var(--accent-ink); border-color: transparent; box-shadow: 0 0 0 4px var(--accent-soft-2); }
  .stophint { display: flex; align-items: center; gap: 6px; color: var(--text-3); font-size: var(--fs-meta); margin-top: -4px; }

  .body { flex: 1; min-height: 0; display: grid; grid-template-columns: 1fr minmax(320px, 380px); }
  .tx { position: relative; min-height: 0; display: flex; flex-direction: column; background: var(--surface); }
  .txs { flex: 1; min-height: 0; padding: 14px 28px 32px; }
  .segs { list-style: none; margin: 0; padding: 0; max-width: 860px; }
  .seg {
    display: grid;
    grid-template-columns: 58px 32px 1fr;
    column-gap: 10px;
    padding-top: 14px;
    align-items: start;
    animation: appear 180ms var(--ease) both;
  }
  .seg.cont { padding-top: 0; }
  .ts { font-size: var(--fs-micro); color: var(--text-3); padding-top: 0; line-height: 24px; text-align: right; opacity: 0.9; transition: opacity var(--t-fast) var(--ease); }
  .seg.cont .ts { opacity: 0; }
  .seg:hover .ts { opacity: 1; }
  .av { padding-top: 0; }
  .who { display: block; font-weight: 600; font-size: var(--fs-meta); color: var(--c); line-height: 24px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .text { font-size: var(--fs-body); line-height: 1.55; color: var(--text); padding: 1px 0; }
  .seg.private .text { color: var(--text-2); font-style: italic; }
  .seg.cmd .text {
    margin: 3px 0;
    padding: 6px 12px;
    border-radius: 0 9px 9px 0;
    background: var(--accent-soft);
    box-shadow: inset 2px 0 0 var(--accent);
    font-weight: 500;
  }
  .priv { display: inline-flex; gap: 3px; align-items: center; font-size: var(--fs-micro); color: var(--text-3); font-style: normal; margin-right: 4px; }
  .cmdtag { margin-left: 8px; vertical-align: 1px; height: 18px; }
  .waiting { display: flex; flex-direction: column; align-items: center; gap: 12px; padding: 64px 0; color: var(--text-3); }
  .eq { display: flex; align-items: center; gap: 3px; height: 22px; }
  .eq i { width: 4px; height: 100%; border-radius: 2px; background: var(--accent); opacity: 0.7; animation: eq 1s ease-in-out infinite; }
  .eq i:nth-child(2) { animation-delay: -0.8s; }
  .eq i:nth-child(3) { animation-delay: -0.6s; }
  .eq i:nth-child(4) { animation-delay: -0.4s; }
  .eq i:nth-child(5) { animation-delay: -0.2s; }
  @keyframes eq { 0%, 100% { transform: scaleY(0.3); } 50% { transform: scaleY(1); } }
  .jump {
    position: absolute;
    bottom: 18px;
    left: 50%;
    transform: translateX(-50%);
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 32px;
    padding: 0 14px;
    border-radius: var(--r-pill);
    border: 1px solid var(--line-strong);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-weight: 500;
    box-shadow: var(--shadow);
    animation: appear var(--t) var(--ease);
  }
  .jump:hover { background: var(--surface-2); }
  .new { margin-left: 2px; padding: 1px 7px; border-radius: var(--r-pill); background: var(--accent); color: var(--accent-ink); font-size: var(--fs-micro); font-weight: 600; }

  .side { border-left: 1px solid var(--line); background: var(--bg); display: flex; flex-direction: column; min-height: 0; }
  .ask { padding: 16px 16px 14px; display: flex; flex-direction: column; gap: 8px; border-bottom: 1px solid var(--line); }
  .askbox {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 3px 3px 11px;
    border: 1px solid var(--line-strong);
    border-radius: 10px;
    background: var(--surface);
    color: var(--text-3);
    box-shadow: var(--shadow-xs);
    transition: border-color var(--t-fast) var(--ease), box-shadow var(--t-fast) var(--ease);
  }
  .askbox:focus-within { border-color: var(--focus); box-shadow: var(--ring); color: var(--accent-text); }
  .askbox input { flex: 1; min-width: 0; height: 28px; border: 0; outline: 0; background: transparent; color: var(--text); font: inherit; }
  .askbox input::placeholder { color: var(--text-3); }
  .feed { flex: 1; min-height: 0; padding: 14px 16px 24px; display: flex; flex-direction: column; gap: 10px; }
  .none { display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 36px 0; }
  .none-ic { width: 38px; height: 38px; border-radius: 11px; display: grid; place-items: center; background: var(--surface); border: 1px solid var(--line); color: var(--text-3); }
  .spin { width: 10px; height: 10px; border: 1.5px solid currentColor; border-right-color: transparent; border-radius: 50%; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .idle {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 18px;
    padding: 24px;
    background:
      radial-gradient(600px 320px at 50% 38%, var(--accent-soft), transparent 70%),
      var(--bg);
  }
  .hero {
    width: 460px;
    max-width: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 14px;
    padding: 36px 36px 26px;
    text-align: center;
    color: var(--text-2);
    border-radius: 18px;
    box-shadow: var(--shadow);
    animation: appear 220ms var(--ease);
  }
  .hero h2 { color: var(--text); font-size: 19px; letter-spacing: -0.015em; }
  .lead { font-size: var(--fs-ui); color: var(--text-2); margin-top: -6px; }
  .hic { display: grid; place-items: center; margin-bottom: 4px; border-radius: 16px; box-shadow: 0 6px 20px rgba(23, 27, 38, 0.28), 0 1px 2px rgba(23, 27, 38, 0.2); }
  .hic.proc { border-radius: 10px; }
  .call-offer.big { width: 100%; padding: 14px 16px; margin-top: 4px; }
  .call-offer.big .to-ic { width: 36px; height: 36px; border-radius: 10px; }
  .call-offer.big .to-title { font-size: 14.5px; }
  .call-offer.big .to-sub { font-size: 12.5px; margin-top: 3px; }
  .start { width: 100%; display: flex; flex-direction: column; gap: 10px; margin-top: 6px; }
  .go {
    height: 44px;
    width: 100%;
    font-size: 14.5px;
    font-weight: 600;
    border-radius: 11px;
    gap: 8px;
    box-shadow: 0 1px 2px rgba(40, 50, 160, 0.25), 0 6px 18px rgba(79, 94, 232, 0.3), inset 0 1px 0 rgba(255, 255, 255, 0.15);
  }
  .howto { max-width: 360px; line-height: 1.5; margin-top: 2px; }
  .progress { width: 180px; height: 4px; border-radius: 2px; background: var(--surface-2); overflow: hidden; margin-top: 4px; }
  .progress span { display: block; width: 40%; height: 100%; border-radius: 2px; background: var(--accent); animation: indet 1.4s var(--ease) infinite; }
  @keyframes indet { from { transform: translateX(-100%); } to { transform: translateX(250%); } }
  .past { display: inline-flex; align-items: center; gap: 4px; color: var(--text-2); text-decoration: none; padding: 6px 10px; border-radius: 8px; transition: background var(--t-fast) var(--ease), color var(--t-fast) var(--ease); }
  .past:hover { background: var(--hover); color: var(--text); }
</style>
