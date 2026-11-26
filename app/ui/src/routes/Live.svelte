<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { api, onEvent } from '../lib/api';
  import { live, clock, STATE_LABEL } from '../lib/live.svelte';
  import { href } from '../lib/router.svelte';
  import type { Request, Segment, Speaker } from '../lib/types';
  import { duration, timeS } from '../lib/format';
  import { toastError } from '../lib/ui.svelte';
  import { speakerColor, speakerName } from '../lib/components/speakers';
  import Icon from '../lib/components/Icon.svelte';
  import Meter from '../lib/components/Meter.svelte';
  import RequestCard from '../lib/components/RequestCard.svelte';

  const s = $derived(live.s);
  const meeting = $derived(s.meeting);
  const active = $derived(s.state === 'listening' || s.state === 'paused');

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
        if (seg.meeting_id !== meeting?.id || segments.some((x) => x.id === seg.id)) return;
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
        <span class="dot {s.state === 'listening' ? 'live' : s.state}"></span>
        <div class="grow">
          <h1 class="ellipsis">{meeting.title ?? 'Réunion sans titre'}</h1>
          <p class="muted small">
            {STATE_LABEL[s.state]} · {duration(meeting.started_at, meeting.ended_at, clock.now)}
            {#if s.pending_requests}{' · '}{s.pending_requests} demande{s.pending_requests > 1 ? 's' : ''} en cours{/if}
            {#if s.sidecar !== 'running'}{' · '}<span class="warn">Audio : {s.sidecar}</span>{/if}
          </p>
        </div>
        {#if s.state !== 'processing'}
          <label class="check small" title="Aucun appel à l’IA pour cette réunion">
            <input type="checkbox" checked={meeting.confidential} onchange={(e) => setConf(e.currentTarget.checked)} />
            <Icon name="lock" size={13} /> Confidentielle
          </label>
          <button class="btn danger" onclick={stop} disabled={busy}><Icon name="stop" size={13} />Arrêter</button>
        {/if}
      </div>
      {#if active}
        <div class="row ctl">
          <div class="meters">
            <Meter db={s.levels.me} label="Micro" icon="mic" />
            <Meter db={s.levels.them} label="Teams" icon="speaker" />
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
            <Icon name="hand" size={14} />
            {s.aside ? 'Aparté en cours…' : 'Maintenir pour un aparté'}
          </button>
        </div>
      {/if}
      {#if s.last_error}<p class="err small">{s.last_error}</p>{/if}
    </header>

    <div class="body">
      <section class="tx" aria-label="Transcription en direct">
        <div class="scroll txs" bind:this={scroller} {onscroll} aria-live="off">
          {#if !visible.length}
            <div class="empty"><p>En attente des premières phrases…</p></div>
          {/if}
          <ol class="segs">
            {#each visible as seg, i (seg.id)}
              {@const cont = i > 0 && visible[i - 1].speaker === seg.speaker}
              <li class="seg" class:cont class:private={seg.private} class:cmd={!!seg.request_id}>
                <time class="ts">{timeS(seg.ts)}</time>
                <span class="who" style="--c: {speakerColor(seg.speaker)}">{#if !cont}{speakerName(seg.speaker, speakers)}{/if}</span>
                <p class="text">
                  {#if seg.private}<span class="priv"><Icon name="lock" size={11} /> Aparté</span>{/if}
                  {seg.text}
                  {#if seg.request_id}<span class="badge outline"><Icon name="ask" size={10} /> Demande</span>{/if}
                </p>
              </li>
            {/each}
          </ol>
        </div>
        {#if !stick}
          <button class="jump" onclick={() => { stick = true; scrollBottom(true); }}>
            ↓ Revenir au direct{#if unseen}{' · '}{unseen} nouvelle{unseen > 1 ? 's' : ''}{/if}
          </button>
        {/if}
      </section>

      <aside class="side" aria-label="Demandes au Scribe">
        <form class="ask" onsubmit={ask}>
          <label class="label" for="live-ask">Demander au Scribe</label>
          <div class="row askrow">
            <input id="live-ask" class="input" bind:value={query} placeholder={meeting.confidential ? 'Confidentielle : notes seulement' : 'Ex. cherche les erreurs sur ms-booking'} autocomplete="off" />
            <button class="btn primary" disabled={!query.trim()}>Envoyer</button>
          </div>
          <p class="faint tiny">Ou à voix haute : « Scribe, cherche… », « Scribe, note… »</p>
        </form>
        <div class="feed scroll">
          {#each feed as r (r.id)}
            <RequestCard req={r} compact />
          {:else}
            <p class="faint small none">Aucune demande pour l’instant.</p>
          {/each}
        </div>
      </aside>
    </div>
  </div>
{:else}
  <div class="idle">
    <div class="empty">
      <Icon name="live" size={34} stroke={1.3} />
      {#if s.state === 'processing'}
        <h2>Compte rendu en préparation…</h2>
      {:else}
        <h2>Aucune réunion en cours</h2>
        <p class="small">Le Scribe démarre seul quand Teams utilise le micro. Vous pouvez aussi démarrer manuellement.</p>
        <form class="row start" onsubmit={(e) => { e.preventDefault(); start(); }}>
          <input class="input" bind:value={newTitle} placeholder="Titre (facultatif)" aria-label="Titre de la réunion" />
          <button class="btn primary" disabled={busy}><Icon name="play" size={13} />Démarrer</button>
        </form>
        <a class="small faint" href={href('/meetings')}>Voir les réunions passées</a>
      {/if}
    </div>
  </div>
{/if}

<style>
  .live { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .lbar { padding: 16px 24px 14px; border-bottom: 1px solid var(--line); background: var(--surface); flex: none; display: flex; flex-direction: column; gap: 12px; }
  .ttl { gap: 12px; }
  .ttl .dot { width: 10px; height: 10px; }
  .ttl h1 { font-size: 17px; }
  .warn { color: var(--warn); }
  .err { color: var(--err); }
  .ctl { gap: 20px; }
  .meters { display: grid; grid-template-columns: 1fr 1fr; gap: 24px; flex: 1; max-width: 520px; }
  .aside { touch-action: none; user-select: none; margin-left: auto; }
  .aside.on { background: var(--brand); color: var(--brand-ink); border-color: var(--brand); }

  .body { flex: 1; min-height: 0; display: grid; grid-template-columns: 1fr minmax(300px, 380px); }
  .tx { position: relative; min-height: 0; display: flex; flex-direction: column; background: var(--surface); }
  .txs { flex: 1; min-height: 0; padding: 12px 24px 24px; }
  .segs { list-style: none; margin: 0; padding: 0; }
  .seg { display: grid; grid-template-columns: 62px 96px 1fr; gap: 10px; padding: 7px 0 1px; align-items: baseline; animation: fadein 0.3s ease-out; }
  .seg.cont { padding-top: 0; }
  @keyframes fadein { from { opacity: 0; transform: translateY(3px); } }
  .ts { font-size: 11px; color: var(--text-3); font-variant-numeric: tabular-nums; }
  .seg.cont .ts { visibility: hidden; }
  .who { font-weight: 600; font-size: 12px; color: var(--c); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .text { font-size: 14px; line-height: 1.55; }
  .seg.private .text { color: var(--text-2); font-style: italic; }
  .seg.cmd .text { font-weight: 500; }
  .priv { display: inline-flex; gap: 3px; align-items: center; font-size: 11px; color: var(--text-3); font-style: normal; margin-right: 4px; }
  .text .badge { margin-left: 6px; vertical-align: 1px; height: 17px; }
  .jump {
    position: absolute;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    height: 28px;
    padding: 0 14px;
    border-radius: 14px;
    border: 1px solid var(--line-strong);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-weight: 500;
    box-shadow: var(--shadow);
  }

  .side { border-left: 1px solid var(--line); background: var(--bg); display: flex; flex-direction: column; min-height: 0; }
  .ask { padding: 14px 16px 12px; display: flex; flex-direction: column; gap: 7px; border-bottom: 1px solid var(--line); }
  .askrow { gap: 6px; }
  .feed { flex: 1; min-height: 0; padding: 12px 16px 20px; display: flex; flex-direction: column; gap: 10px; }
  .none { text-align: center; padding: 20px 0; }

  .idle { height: 100%; display: grid; place-items: center; }
  .start { margin-top: 8px; width: 360px; }
</style>
