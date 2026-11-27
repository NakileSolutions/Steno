<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { api, onEvent } from '../lib/api';
  import { live, clock, STATE_LABEL } from '../lib/live.svelte';
  import { ROOM, callOffer, isListening, meetingCallChip } from '../lib/listening';
  import { isDeviceMissing, missingInputLabel } from '../lib/audio';
  import type { AudioDevice } from '../lib/types';
  import { prefs, stopHint } from '../lib/prefs.svelte';
  import { href, navigate } from '../lib/router.svelte';
  import type { Request, Segment, Speaker } from '../lib/types';
  import { clockDuration, timeS } from '../lib/format';
  import { insertChrono } from '../lib/group';
  import { copyText, promptDialog, toast, toastError } from '../lib/ui.svelte';
  import { initial, speakerColor, speakerName } from '../lib/components/speakers';
  import Logo from '../lib/Logo.svelte';
  import Icon from '../lib/components/Icon.svelte';
  import Meter from '../lib/components/Meter.svelte';
  import Waveform from '../lib/components/Waveform.svelte';
  import RequestCard from '../lib/components/RequestCard.svelte';
  import ContextDialog from '../lib/components/ContextDialog.svelte';
  import { t, tn } from '../lib/i18n.svelte';

  const s = $derived(live.s);
  const meeting = $derived(s.meeting);
  const active = $derived(isListening(s));
  const offer = $derived(callOffer(s, prefs.callApps));
  /** Nom du canal « eux » : l'app de l'appel en cours. */
  const themLabel = $derived(meetingCallChip(meeting?.call_app, prefs.callApps) ?? t('Visio'));
  /** Puce de l'app de l'appel (Teams, Slack…), si l'écoute a démarré sur un appel. */
  const appChip = $derived(meetingCallChip(meeting?.call_app, prefs.callApps));
  /** Réunion en présentiel : micro seul, pas de son d'app. */
  const inPerson = $derived(meeting?.call_app === ROOM);
  // Micro choisi débranché : le sidecar a repris le micro par défaut (liste relue à chaque réunion).
  let inputs = $state<AudioDevice[]>([]);
  $effect(() => {
    if (!meeting?.id || !prefs.inputDevice) return;
    api.listAudioDevices().then((d) => (inputs = d.inputs)).catch(() => {});
  });
  const micGone = $derived(isDeviceMissing(prefs.inputDevice, inputs));
  const elapsed = $derived.by(() => {
    if (!meeting) return '0:00';
    const end = meeting.ended_at ? new Date(meeting.ended_at).getTime() : clock.now;
    return clockDuration(Math.max(0, Math.round((end - new Date(meeting.started_at).getTime()) / 1000)));
  });

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

  /** Retire une phrase qui n'a pas été dite (erreur de transcription). */
  async function removeSegment(seg: Segment) {
    try {
      await api.deleteSegment(seg.id);
      segments = segments.filter((x) => x.id !== seg.id);
    } catch (e) {
      toastError(e);
    }
  }

  /** Donne un nom à un intervenant reconnu à la voix (« Intervenant 3 » → « Marie »). */
  async function renameSpeaker(label: string) {
    if (!meeting || label === 'Moi') return;
    const current = speakers.find((s) => s.label === label)?.display_name ?? '';
    const name = await promptDialog({
      title: t('Nommer l’intervenant'),
      message: t('Nom affiché pour « {name} » dans cette réunion.', { name: speakerName(label, speakers) }),
      value: current,
      placeholder: t('Prénom Nom'),
      confirm: t('Enregistrer'),
    });
    if (name === null) return;
    try {
      await api.renameSpeaker(meeting.id, label, name.trim());
      const sp = speakers.find((s) => s.label === label);
      if (sp) sp.display_name = name.trim() || null;
      else speakers.push({ label, display_name: name.trim() || null });
    } catch (e) {
      toastError(e);
    }
  }

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
      onEvent('steno://segment', async (seg) => {
        if (seg.meeting_id !== meeting?.id) return;
        // Une phrase déjà affichée peut revenir mise à jour (rattachée à une demande, écho…).
        const i = segments.findIndex((x) => x.id === seg.id);
        if (i >= 0) {
          segments[i] = seg;
          return;
        }
        const at = insertChrono(segments, seg);
        // Phrase rangée plus haut (arrivée en retard) : pas de « nouvelles lignes » à signaler.
        if (at < segments.length - 1) return;
        if (stick) {
          await tick();
          scrollBottom(true);
        } else unseen++;
      }),
      onEvent('steno://segment_deleted', ({ meeting_id, id }) => {
        if (meeting_id === meeting?.id) segments = segments.filter((x) => x.id !== id);
      }),
      onEvent('steno://request', (r) => {
        if (r.meeting_id !== meeting?.id) return;
        const i = requests.findIndex((x) => x.id === r.id);
        if (i >= 0) requests[i] = r;
        else requests.push(r);
      }),
    ];
    return () => uns.forEach((u) => u());
  });

  // Défilement automatique en cours : ses événements `scroll` ne détachent pas du direct.
  let autoUntil = 0;
  function scrollBottom(smooth: boolean) {
    autoUntil = performance.now() + (smooth ? 600 : 50);
    scroller?.scrollTo({ top: scroller.scrollHeight, behavior: smooth ? 'smooth' : 'auto' });
    unseen = 0;
  }
  function onscroll() {
    if (!scroller) return;
    if (stick && performance.now() < autoUntil) return;
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

  async function start(inPerson = false) {
    busy = true;
    try {
      await api.startMeeting(newTitle.trim() || undefined, inPerson);
      newTitle = '';
    } catch (e) {
      toastError(e);
    } finally {
      busy = false;
    }
  }
  let ctxOpen = $state(false);
  let ctxMode = $state<'stop' | 'edit'>('edit');
  /** Arrêt depuis la fenêtre : propose d'abord contexte, documents et format (sauf réunion confidentielle). */
  async function askStop() {
    if (!meeting?.confidential) {
      try {
        const st = await api.getSettings();
        if (st.report?.ask_context_on_stop) {
          ctxMode = 'stop';
          ctxOpen = true;
          return;
        }
      } catch {
        // Réglages illisibles : arrêt direct.
      }
    }
    await stop();
  }
  function openContext() {
    ctxMode = 'edit';
    ctxOpen = true;
  }
  async function stop() {
    busy = true;
    const id = meeting?.id;
    try {
      await api.stopMeeting();
      // Page de la réunion : compte à rebours de la rédaction, contexte et documents à portée.
      if (id) navigate(`/meetings/${id}`);
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

  // Rappel RGPD : prévenir les participants au début de chaque écoute (une fois par réunion).
  let consentDismissed = $state<string | null>(null);
  const showConsent = $derived(!!meeting && prefs.consentReminder && consentDismissed !== meeting.id && s.state !== 'processing');
  async function copyConsent() {
    if (await copyText(prefs.consentMessage)) {
      toast(t('Message copié : collez-le dans le chat de l’appel.'));
      consentDismissed = meeting?.id ?? null;
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
    <header class="lbar hero-live" class:rec={s.state === 'listening'}>
      <div class="row ttl">
        <span class="recdot {s.state}" aria-hidden="true"><span class="dot {s.state === 'listening' ? 'live' : s.state}"></span></span>
        <div class="grow">
          <p class="sub">
            <span class="st {s.state}">{t(STATE_LABEL[s.state])}</span>
            {#if appChip}<span class="chip"><Icon name={inPerson ? 'user' : 'phone'} size={11} />{appChip}</span>{/if}
            {#if micGone}<span class="chip micgone" title={missingInputLabel(prefs.inputDeviceName, prefs.inputDevice)}><Icon name="mic" size={11} />{t('micro par défaut')}</span>{/if}
            {#if s.pending_requests}<span class="pend"><span class="spin" aria-hidden="true"></span>{tn(s.pending_requests, '{n} demande en cours', '{n} demandes en cours')}</span>{/if}
            {#if s.sidecar !== 'running'}<span class="warn">{t('Audio : {state}', { state: s.sidecar })}</span>{/if}
          </p>
          <h1 class="ellipsis">{meeting.title ?? t('Réunion sans titre')}</h1>
        </div>
        {#if s.state !== 'processing'}
          <label class="check conf" title={t('Aucun appel à l’IA pour cette réunion')}>
            <Icon name="lock" size={14} /> {t('Confidentielle')}
            <input class="switch" type="checkbox" role="switch" checked={meeting.confidential} onchange={(e) => setConf(e.currentTarget.checked)} />
          </label>
          <button class="btn pausebtn" onclick={togglePause} disabled={busy} title={s.state === 'paused' ? t('Reprendre la transcription') : t('Mettre l’écoute en pause : rien n’est transcrit, la réunion continue')}>
            <Icon name={s.state === 'paused' ? 'play' : 'pause'} size={14} />{s.state === 'paused' ? t('Reprendre') : t('Pause')}
          </button>
          {#if !meeting.confidential}
            <button class="btn ctxbtn" onclick={openContext} title={t('Contexte, documents et format du compte rendu')}><Icon name="paperclip" size={14} />{t('Contexte')}</button>
          {/if}
          <button class="btn danger stopbtn" onclick={askStop} disabled={busy}><span class="sq" aria-hidden="true"></span>{t('Arrêter l’écoute')}</button>
        {/if}
      </div>
      {#if showConsent}
        <div class="consent" role="status">
          <Icon name="info" size={15} />
          <p class="grow">{t('Prévenez les participants que la réunion est transcrite.')}</p>
          <button class="btn sm primary" onclick={copyConsent}><Icon name="copy" size={13} />{t('Copier le message')}</button>
          <button class="btn sm ghost icon" onclick={() => (consentDismissed = meeting?.id ?? null)} title={t('Masquer pour cette réunion')} aria-label={t('Masquer le rappel')}><Icon name="x" size={13} /></button>
        </div>
      {/if}
      {#if s.state === 'paused'}<p class="paused-note small"><Icon name="pause" size={13} />{t('Écoute en pause : rien n’est transcrit jusqu’à la reprise.')}</p>{/if}

      <div class="row wavebar">
        <Waveform me={s.levels.me} them={s.levels.them} active={s.state === 'listening'} />
        <time class="clock tnum" aria-label={t('Durée {d}', { d: elapsed })}>{elapsed}</time>
      </div>

      {#if active}
        <div class="row ctl">
          <button
            class="btn mute"
            class:on={s.mic_muted}
            aria-pressed={s.mic_muted}
            onclick={() => api.setMicMuted(!s.mic_muted).catch(toastError)}
            title={s.mic_muted ? t('Rétablir votre micro dans Sténo') : t('Couper votre micro dans Sténo : vos paroles ne sont plus transcrites (le son de l’appel, si)')}
          >
            <Icon name={s.mic_muted ? 'micoff' : 'mic'} size={15} />
            {s.mic_muted ? t('Micro coupé') : t('Couper mon micro')}
          </button>
          <div class="meters">
            <Meter db={s.mic_muted ? -90 : s.levels.me} label={s.mic_muted ? t('Micro coupé') : t('Micro')} icon={s.mic_muted ? 'micoff' : 'mic'} />
            {#if !inPerson}<Meter db={s.levels.them} label={themLabel} icon="speaker" />{/if}
          </div>
          <button
            class="btn aside"
            class:on={s.aside}
            aria-pressed={s.aside}
            title={t('Maintenir pour parler en aparté : vos phrases sont privées et exclues du compte rendu')}
            onpointerdown={(e) => { e.currentTarget.setPointerCapture(e.pointerId); asideOn(); }}
            onpointerup={asideOff}
            onpointercancel={asideOff}
            onlostpointercapture={asideOff}
            onkeydown={(e) => { if ((e.key === ' ' || e.key === 'Enter') && !e.repeat) { e.preventDefault(); asideOn(); } }}
            onkeyup={(e) => { if (e.key === ' ' || e.key === 'Enter') { e.preventDefault(); asideOff(); } }}
            onblur={asideOff}
          >
            <Icon name="hand" size={15} />
            {s.aside ? t('Aparté en cours…') : t('Maintenir pour un aparté')}
          </button>
        </div>
      {/if}
      {#if active}<p class="stophint"><Icon name="info" size={13} />{stopHint()}</p>{/if}
      {#if s.last_error}<p class="err small"><Icon name="warn" size={13} />{s.last_error}</p>{/if}
    </header>
    <ContextDialog meetingId={meeting.id} mode={ctxMode} bind:open={ctxOpen} onstop={stop} />

    <div class="body">
      <section class="tx" aria-label={t('Transcription en direct')}>
        <div class="scroll txs" bind:this={scroller} {onscroll} aria-live="off">
          {#if !visible.length}
            <div class="waiting">
              <span class="eq" aria-hidden="true"><i></i><i></i><i></i><i></i><i></i></span>
              <p>{t('En attente des premières phrases…')}</p>
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
                  {#if !cont}
                    {#if !/^Eux-\d+$/.test(seg.speaker)}<span class="who">{name}</span>
                    {:else}<button class="who rename" onclick={() => renameSpeaker(seg.speaker)} title={t('Nommer cet intervenant')}>{name}</button>{/if}
                  {/if}
                  <p class="text">
                    {#if seg.private}<span class="priv"><Icon name="lock" size={11} /> {t('Aparté')}</span>{/if}
                    {seg.text}
                    {#if seg.request_id}<span class="badge accent cmdtag"><Icon name="ask" size={11} /> {t('Demande')}</span>{/if}
                  </p>
                </div>
                <button class="del" onclick={() => removeSegment(seg)} title={t('Supprimer cette phrase (jamais dite)')} aria-label={t('Supprimer cette phrase')}><Icon name="trash" size={13} /></button>
              </li>
            {/each}
          </ol>
        </div>
        {#if !stick}
          <button class="jump" onclick={() => { stick = true; scrollBottom(true); }}>
            <Icon name="arrowdown" size={13} stroke={2} /> {t('Revenir au direct')}{#if unseen}<span class="new">{tn(unseen, '{n} nouvelle', '{n} nouvelles')}</span>{/if}
          </button>
        {/if}
      </section>

      <aside class="side" aria-label={t('Demandes à Sténo')}>
        <form class="ask" onsubmit={ask}>
          <label class="label" for="live-ask">{t('Demander à Sténo')}</label>
          <div class="askbox">
            <Icon name="ask" size={15} />
            <input id="live-ask" bind:value={query} placeholder={meeting.confidential ? t('Confidentielle : notes seulement') : t('Votre question…')} autocomplete="off" />
            <button class="btn primary sm" disabled={!query.trim()}>{t('Envoyer')}</button>
          </div>
          <p class="faint tiny">{t('Ou à voix haute : « Sténo, cherche… », « Sténo, note… »')}</p>
        </form>
        <div class="feed scroll">
          {#each feed as r (r.id)}
            <RequestCard req={r} compact />
          {:else}
            <div class="none">
              <span class="none-ic"><Icon name="ask" size={18} /></span>
              <p class="faint small">{t('Aucune demande pour l’instant.')}</p>
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
        <h2>{t('Compte rendu en préparation…')}</h2>
        <p class="lead">{t('L’écoute est arrêtée. Le compte rendu apparaîtra dans « Réunions ».')}</p>
        <div class="progress" aria-hidden="true"><span></span></div>
      {:else if offer}
        <span class="hic"><Logo size={56} tile /></span>
        <div class="call-offer big" role="status">
          <span class="to-ic"><Icon name="phone" size={18} /></span>
          <div class="grow">
            <p class="to-title">{offer.heading}{t(' : ')}<strong>{offer.title}</strong></p>
            <p class="to-sub">{t('Sténo n’écoute pas encore : rien n’est transcrit tant que vous ne l’avez pas décidé.')}{#if offer.browser}{' '}{t('Attention : tout le son du navigateur sera capté, y compris les autres onglets.')}{/if}</p>
          </div>
        </div>
        <button class="btn primary go" onclick={() => start()} disabled={busy}><Icon name="play" size={15} stroke={2} />{t('Démarrer l’écoute')}</button>
        <button class="btn room" onclick={() => start(true)} disabled={busy} title={t('Sans app de visio : seul le micro de l’ordinateur est écouté')}><Icon name="user" size={15} />{t('Plutôt une réunion en présentiel')}</button>
      {:else}
        <span class="hic"><Logo size={56} tile /></span>
        <h2>{t('Sténo n’écoute pas')}</h2>
        <p class="lead">{t('L’écoute démarre et s’arrête uniquement sur votre demande.')}</p>
        <form class="start" onsubmit={(e) => { e.preventDefault(); start(); }}>
          <input class="input lg" bind:value={newTitle} placeholder={t('Titre de la réunion (facultatif)')} aria-label={t('Titre de la réunion')} />
          <button class="btn primary go" disabled={busy}><Icon name="play" size={15} stroke={2} />{t('Démarrer l’écoute')}</button>
        </form>
        <button class="btn room" onclick={() => start(true)} disabled={busy} title={t('Sans app de visio : seul le micro de l’ordinateur est écouté ; les intervenants sont identifiés à la fin')}>
          <Icon name="user" size={15} />{t('Réunion en présentiel')}
        </button>
      {/if}
      {#if s.state !== 'processing'}
        <p class="tiny faint howto">{t('Pour arrêter ensuite : le bouton « Arrêter l’écoute », le raccourci, le menu de la barre d’état, ou à la voix.')}</p>
      {/if}
    </div>
    <a class="past small" href={href('/meetings')}>{t('Voir les réunions passées')} <Icon name="chevron" size={13} /></a>
  </div>
{/if}

<style>
  .live { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .lbar {
    position: relative;
    padding: 20px 28px 14px;
    border-bottom: 1px solid var(--line);
    background:
      radial-gradient(70% 140% at 100% 0%, color-mix(in srgb, var(--accent) 7%, transparent), transparent 60%),
      radial-gradient(50% 120% at 0% 0%, color-mix(in srgb, var(--accent) 5%, transparent), transparent 70%),
      var(--surface);
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .lbar.rec {
    background:
      radial-gradient(70% 140% at 100% 0%, color-mix(in srgb, var(--accent) 8%, transparent), transparent 60%),
      radial-gradient(40% 120% at 0% 0%, color-mix(in srgb, var(--live) 6%, transparent), transparent 70%),
      var(--surface);
  }
  .ttl { gap: 14px; align-items: center; }
  .recdot { width: 38px; height: 38px; border-radius: 12px; display: grid; place-items: center; background: var(--live-soft); flex: none; box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--live) 18%, transparent); }
  .recdot.paused { background: var(--warn-soft); box-shadow: none; }
  .recdot.processing { background: var(--accent-soft); box-shadow: none; }
  .recdot .dot { width: 10px; height: 10px; }
  .ttl h1 { font: 650 21px/1.25 var(--font-display); letter-spacing: -0.02em; margin-top: 2px; }
  .sub { display: flex; align-items: center; gap: 8px; font-size: var(--fs-meta); color: var(--text-2); }
  .st { font-weight: 650; font-size: var(--fs-micro); letter-spacing: 0.06em; text-transform: uppercase; }
  .st.listening { color: var(--live); }
  .st.paused { color: var(--warn); }
  .st.processing { color: var(--accent-text); }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 20px;
    padding: 0 8px;
    border-radius: var(--r-pill);
    font-size: var(--fs-micro);
    font-weight: 500;
    color: var(--text-2);
    background: var(--surface-2);
    box-shadow: inset 0 0 0 1px var(--line);
  }
  .chip :global(.icon) { color: var(--text-3); }
  .chip.micgone { color: var(--warn); }
  .chip.micgone :global(.icon) { color: var(--warn); }
  .pend { display: inline-flex; align-items: center; gap: 6px; color: var(--accent-text); font-weight: 500; }
  .warn { color: var(--warn); }
  .err { color: var(--err); display: flex; align-items: center; gap: 6px; }
  .conf { gap: 8px; font-size: var(--fs-ui); color: var(--text-2); padding: 0 6px; height: 30px; }
  .conf :global(.icon) { color: var(--text-3); }
  .stopbtn { gap: 8px; }
  .pausebtn { gap: 6px; }
  .btn.room { gap: 7px; margin-top: 10px; }
  .consent { display: flex; align-items: center; gap: 10px; margin-top: 10px; padding: 8px 10px 8px 12px; border-radius: 10px; background: var(--accent-soft); color: var(--accent-text); }
  .consent p { color: var(--text); font-size: var(--fs-ui); }
  .paused-note { display: flex; align-items: center; gap: 6px; margin-top: 8px; color: var(--warn); }
  .sq { width: 9px; height: 9px; border-radius: 2px; background: currentColor; }
  .wavebar { gap: 22px; align-items: center; padding: 2px 0 0 52px; }
  .clock {
    font: 300 30px/1 var(--font-display);
    letter-spacing: -0.02em;
    color: var(--text);
    min-width: 92px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .ctl { gap: 20px; padding-left: 52px; }
  .meters { display: grid; grid-template-columns: 1fr 1fr; gap: 28px; flex: 1; max-width: 520px; }
  .aside { touch-action: none; user-select: none; margin-left: auto; }
  .mute.on { background: var(--err-soft); color: var(--err); border-color: transparent; }
  .aside.on { background: var(--accent); color: var(--accent-ink); border-color: transparent; box-shadow: 0 0 0 4px var(--accent-soft-2); }
  .stophint { display: flex; align-items: center; gap: 6px; color: var(--text-3); font-size: var(--fs-meta); margin-top: -2px; padding-left: 52px; }

  .body { flex: 1; min-height: 0; display: grid; grid-template-columns: 1fr minmax(320px, 380px); }
  .tx { position: relative; min-height: 0; display: flex; flex-direction: column; background: var(--surface); }
  .txs { flex: 1; min-height: 0; padding: 6px 28px 40px; }
  .segs { list-style: none; margin: 0; padding: 0; max-width: 860px; }
  .seg {
    display: grid;
    grid-template-columns: 58px 32px 1fr 24px;
    column-gap: 12px;
    padding-top: 22px;
    align-items: start;
    animation: seg-in 240ms var(--ease-out) both;
  }
  .seg.cont { padding-top: 4px; }
  @keyframes seg-in { from { opacity: 0; transform: translateY(6px); } }
  .ts { font-size: var(--fs-micro); color: var(--text-3); padding-top: 0; line-height: 24px; text-align: right; opacity: 0.9; transition: opacity var(--t-fast) var(--ease); }
  .seg.cont .ts { opacity: 0; }
  .seg:hover .ts { opacity: 1; }
  .av { padding-top: 0; }
  .who { display: block; font-weight: 600; font-size: var(--fs-meta); color: var(--c); line-height: 24px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .text { font-size: var(--fs-body); line-height: 1.62; color: var(--text); padding: 1px 0; }
  .who.rename { all: unset; display: block; font-weight: 600; font-size: var(--fs-meta); color: var(--c); line-height: 24px; cursor: pointer; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .who.rename:hover { text-decoration: underline; }
  .who.rename:focus-visible { outline: 2px solid var(--accent); border-radius: 4px; }
  .del { all: unset; display: grid; place-items: center; width: 24px; height: 24px; border-radius: 6px; color: var(--text-3); opacity: 0; cursor: pointer; transition: opacity var(--t-fast) var(--ease); }
  .seg:hover .del, .del:focus-visible { opacity: 1; }
  .del:hover { color: var(--err); background: var(--err-soft); }
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
  /* Les demandes arrivent en douceur ; celles en cours respirent à peine. */
  .feed :global(article.req) { animation: req-in 320ms var(--ease-out) both; }
  .feed :global(article.req.pending) { box-shadow: 0 0 0 1px var(--accent-line), 0 6px 20px -10px color-mix(in srgb, var(--accent) 60%, transparent); }
  @keyframes req-in { from { opacity: 0; transform: translateY(-8px) scale(0.98); } }
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
