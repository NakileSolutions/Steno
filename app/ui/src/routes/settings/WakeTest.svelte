<script lang="ts">
  // Procédure de test du mot d'appel (PLAN §6.4) : 5 essais, texte piège, variantes proposées.
  import { api } from '../../lib/api';
  import type { Settings, VariantCheck } from '../../lib/types';
  import { toastError } from '../../lib/ui.svelte';
  import Icon from '../../lib/components/Icon.svelte';

  let {
    settings = $bindable(),
    onclose,
  }: { settings: Settings; onclose: () => void } = $props();

  const word = $derived(settings.voice.wake_word);
  const phrase = $derived(`${settings.voice.strict ? (settings.voice.prefixes[0] ?? 'ok') + ' ' : ''}${word}, cherche quelque chose`);
  const TRAP =
    "Le script de déploiement est décrit dans la description du ticket. Thomas s'est inscrit à la revue de jeudi. " +
    'Il faudra transcrire les échanges et prescrire un nouveau délai. Le scripteur a tout noté sur le tableau.';

  type Attempt = { text: string; detected: boolean; intent: string | null };
  let step = $state<'record' | 'trap' | 'result'>('record');
  let attempts = $state<Attempt[]>([]);
  let recording = $state(false);
  let trapText = $state<string | null>(null);
  let trapHits = $state<string[]>([]);
  let candidates = $state<{ variant: string; checked: boolean }[]>([]);
  let check = $state<VariantCheck | null>(null);
  let el: HTMLDialogElement | undefined = $state();

  $effect(() => {
    if (el && !el.open) el.showModal();
  });

  async function recordAttempt() {
    recording = true;
    try {
      const { text } = await api.wakeTestRecord(4);
      const d = await api.wakeDetect(text);
      attempts.push({ text, detected: d.detected && d.intent === 'search', intent: d.intent });
    } catch (e) {
      toastError(e);
    } finally {
      recording = false;
    }
  }

  async function recordTrap() {
    recording = true;
    try {
      const { text } = await api.wakeTestRecord(20);
      trapText = text;
      const sentences = text.split(/(?<=[.!?])\s+/).filter((s) => s.trim());
      const res = await Promise.all(sentences.map((s) => api.wakeDetect(s)));
      trapHits = sentences.filter((_, i) => res[i].detected);
      prepareCandidates();
      step = 'result';
    } catch (e) {
      toastError(e);
    } finally {
      recording = false;
    }
  }

  function prepareCandidates() {
    const verbs = Object.values(settings.voice.verbs).flat().map((v) => v.toLowerCase());
    const prefixes = settings.voice.prefixes.map((p) => p.toLowerCase());
    const out: string[] = [];
    for (const a of attempts.filter((x) => !x.detected)) {
      const words = a.text.toLowerCase().split(/[\s,.;:!?]+/).filter(Boolean);
      const vi = words.findIndex((w) => verbs.includes(w));
      const before = (vi > 0 ? words.slice(0, vi) : words.slice(0, 1)).filter((w) => !prefixes.includes(w));
      const v = before.join(' ').trim();
      if (v && !out.includes(v) && !settings.voice.variants.includes(v)) out.push(v);
    }
    candidates = out.map((variant) => ({ variant, checked: true }));
  }

  async function addVariants() {
    const chosen = candidates.filter((c) => c.checked).map((c) => c.variant);
    if (!chosen.length) return;
    try {
      check = await api.checkVariants(word, chosen);
      settings.voice.variants = [...new Set([...settings.voice.variants, ...check.accepted])];
      candidates = [];
    } catch (e) {
      toastError(e);
    }
  }

  const rate = $derived(attempts.filter((a) => a.detected).length);
  const done5 = $derived(attempts.length >= 5);
</script>

<dialog bind:this={el} oncancel={(e) => { e.preventDefault(); onclose(); }} aria-labelledby="wt-title">
  <header class="row">
    <h2 id="wt-title" class="grow">Tester le mot d’appel « {word} »</h2>
    <button class="btn ghost sm icon" onclick={onclose} aria-label="Fermer"><Icon name="x" size={14} /></button>
  </header>

  <ol class="steps" aria-label="Étapes">
    <li class:cur={step === 'record'} class:ok={step !== 'record'}>1. Cinq essais</li>
    <li class:cur={step === 'trap'} class:ok={step === 'result'}>2. Texte piège</li>
    <li class:cur={step === 'result'}>3. Résultat</li>
  </ol>

  {#if step === 'record'}
    <p>Dites à voix normale, comme en réunion :</p>
    <p class="say">« {phrase} »</p>
    <ul class="attempts">
      {#each Array(5) as _, i}
        {@const a = attempts[i]}
        <li class:ok={a?.detected} class:ko={a && !a.detected}>
          <span class="n">{i + 1}</span>
          {#if a}
            <span class="grow ellipsis">« {a.text} »</span>
            <span class="badge {a.detected ? 'ok' : 'err'}">{a.detected ? 'Détecté' : 'Raté'}</span>
          {:else if recording && i === attempts.length}
            <span class="grow rec"><span class="dot live"></span> Écoute… parlez maintenant</span>
          {:else}
            <span class="grow faint">—</span>
          {/if}
        </li>
      {/each}
    </ul>
    <div class="row foot">
      <span class="grow small muted">{attempts.length ? `Détection : ${rate}/${attempts.length}` : 'Objectif : 5/5'}</span>
      {#if done5}
        <button class="btn primary" onclick={() => (step = 'trap')}>Continuer</button>
      {:else}
        <!-- svelte-ignore a11y_autofocus -->
        <button class="btn primary" onclick={recordAttempt} disabled={recording} autofocus><Icon name="mic" size={14} />{recording ? 'Écoute…' : `Enregistrer l’essai ${attempts.length + 1}/5`}</button>
      {/if}
    </div>
  {:else if step === 'trap'}
    <p>Lisez ce texte à voix haute. Il contient des mots proches du mot d’appel : <strong>aucun déclenchement</strong> n’est attendu.</p>
    <blockquote class="trap">{TRAP}</blockquote>
    <div class="row foot">
      <span class="grow small muted">Environ 20 secondes.</span>
      <button class="btn primary" onclick={recordTrap} disabled={recording}><Icon name="mic" size={14} />{recording ? 'Écoute… lisez le texte' : 'Commencer la lecture'}</button>
    </div>
  {:else}
    <div class="results">
      <div class="score" class:good={rate === 5}>
        <span class="big">{rate}/5</span>
        <span class="small muted">commandes détectées</span>
      </div>
      <div class="score" class:good={trapHits.length === 0}>
        <span class="big">{trapHits.length}</span>
        <span class="small muted">faux déclenchement{trapHits.length > 1 ? 's' : ''}</span>
      </div>
    </div>
    {#if trapHits.length}
      <p class="small warn">Phrases qui ont déclenché à tort : {trapHits.map((h) => `« ${h} »`).join(', ')}. Essayez le mode strict ou un autre mot.</p>
    {/if}
    {#if rate === 5 && !trapHits.length}
      <p class="small okc">Parfait : le mot d’appel est fiable avec votre voix et votre micro.</p>
    {/if}
    {#if candidates.length}
      <div class="cands">
        <p class="small">Ajouter les transcriptions ratées comme variantes acceptées ?</p>
        {#each candidates as c (c.variant)}
          <label class="check small"><input type="checkbox" bind:checked={c.checked} /> <code>{c.variant}</code></label>
        {/each}
        <div><button class="btn sm" onclick={addVariants}><Icon name="plus" size={13} />Ajouter comme variantes</button></div>
      </div>
    {/if}
    {#if check}
      <div class="small">
        {#if check.accepted.length}<p class="okc">Ajoutées : {check.accepted.join(', ')} (pensez à enregistrer).</p>{/if}
        {#each check.rejected as r}<p class="warn">« {r.variant} » refusée : {r.reason}</p>{/each}
      </div>
    {/if}
    {#if trapText}<details class="small faint"><summary>Transcription du texte piège</summary><p>{trapText}</p></details>{/if}
    <div class="row foot">
      <button class="btn" onclick={() => { attempts = []; trapHits = []; check = null; step = 'record'; }}>Recommencer</button>
      <span class="grow"></span>
      <button class="btn primary" onclick={onclose}>Terminer</button>
    </div>
  {/if}
</dialog>

<style>
  dialog {
    width: min(520px, calc(100vw - 32px));
    padding: 18px 20px 20px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
  }
  dialog::backdrop { background: rgba(15, 17, 23, 0.35); }
  dialog > * + * { margin-top: 12px; }
  .steps { display: flex; gap: 16px; list-style: none; padding: 0; font-size: 12px; color: var(--text-3); }
  .steps .cur { color: var(--text); font-weight: 600; }
  .steps .ok { color: var(--ok); }
  .say { font-size: 17px; font-weight: 600; padding: 10px 14px; background: var(--surface-2); border-radius: 8px; }
  .attempts { list-style: none; padding: 0; border: 1px solid var(--line); border-radius: 8px; }
  .attempts li { display: flex; align-items: center; gap: 10px; padding: 7px 10px; border-top: 1px solid var(--line); min-height: 34px; }
  .attempts li:first-child { border-top: 0; }
  .n { width: 18px; color: var(--text-3); font-variant-numeric: tabular-nums; }
  .rec { display: flex; align-items: center; gap: 8px; color: var(--live); }
  .foot { gap: 8px; padding-top: 4px; }
  .trap { margin: 0; padding: 12px 14px; border-left: 3px solid var(--line-strong); background: var(--surface-2); border-radius: 0 8px 8px 0; font-size: 14px; line-height: 1.6; }
  .results { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .score { display: flex; flex-direction: column; align-items: center; padding: 14px; border: 1px solid var(--line); border-radius: 10px; }
  .score .big { font-size: 26px; font-weight: 600; color: var(--warn); font-variant-numeric: tabular-nums; }
  .score.good .big { color: var(--ok); }
  .warn { color: var(--warn); }
  .okc { color: var(--ok); }
  .cands { display: flex; flex-direction: column; gap: 6px; padding: 10px 12px; border: 1px solid var(--line); border-radius: 8px; }
</style>
