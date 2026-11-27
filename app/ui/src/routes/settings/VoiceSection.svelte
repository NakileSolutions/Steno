<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { api } from '../../lib/api';
  import { defaultOutputLabel, isDeviceMissing } from '../../lib/audio';
  import type { AudioDevice, Settings, VariantCheck, VoiceVerbs, WakeCheck } from '../../lib/types';
  import { debounce, stripLeadingIcon } from '../../lib/format';
  import { toastError } from '../../lib/ui.svelte';
  import Icon from '../../lib/components/Icon.svelte';
  import TagInput from '../../lib/components/TagInput.svelte';
  import WakeTest from './WakeTest.svelte';
  import { t, tk, tn } from '../../lib/i18n.svelte';

  let { settings = $bindable(), ensureSaved }: { settings: Settings; ensureSaved: () => Promise<void> } = $props();

  let check = $state<WakeCheck | null>(null);
  let checking = $state(false);
  let generated = $state<VariantCheck | null>(null);
  let rejectedNow = $state<{ variant: string; reason: string } | null>(null);
  let testing = $state(false);
  let outputs = $state<AudioDevice[]>([]);
  let outputsLoaded = $state(false);

  onMount(() => {
    api
      .listAudioDevices()
      .then((d) => (outputs = d.outputs))
      .catch(() => {})
      .finally(() => (outputsLoaded = true));
  });
  const outputGone = $derived(isDeviceMissing(settings.voice.feedback_device, outputs));

  const runCheck = debounce(async (word: string, strict: boolean) => {
    try {
      check = await api.checkWakeWord(word, strict);
    } catch (e) {
      toastError(e);
    } finally {
      checking = false;
    }
  }, 350);

  $effect(() => {
    const w = settings.voice.wake_word;
    const s = settings.voice.strict;
    untrack(() => (checking = true));
    runCheck(w, s);
  });

  async function generate() {
    try {
      generated = await api.generateVariants(settings.voice.wake_word);
      settings.voice.variants = [...new Set([...settings.voice.variants, ...generated.accepted])];
    } catch (e) {
      toastError(e);
    }
  }

  async function onAddVariant(v: string): Promise<boolean> {
    try {
      const r = await api.checkVariants(settings.voice.wake_word, [v]);
      rejectedNow = r.rejected[0] ?? null;
      return r.accepted.length > 0;
    } catch (e) {
      toastError(e);
      return false;
    }
  }

  async function openTest() {
    try {
      await ensureSaved();
      testing = true;
    } catch (e) {
      toastError(e);
    }
  }

  const INTENTS: { id: keyof VoiceVerbs; label: string; hint: string }[] = [
    { id: 'search', label: tk('Rechercher'), hint: tk('recherche croisée') },
    { id: 'check', label: tk('Vérifier'), hint: tk('vérification ciblée') },
    { id: 'note', label: tk('Noter'), hint: tk('aucune recherche') },
    { id: 'recap', label: tk('Récapituler'), hint: tk('résumé rapide') },
    { id: 'cancel', label: tk('Annuler'), hint: tk('dernière demande') },
    { id: 'stop', label: tk('Arrêter l’écoute'), hint: tk('termine la réunion') },
  ];
</script>

<div class="field">
  <label class="field-label" for="wake">{t('Mot d’appel')}<span class="hint">{t('Seul, en début de phrase ; précédé d’un préfixe (« OK Sténo »), n’importe où dans la phrase. Micro uniquement.')}</span></label>
  <div class="field-body">
    <div class="row">
      <input id="wake" class="input lg wake" bind:value={settings.voice.wake_word} autocomplete="off" spellcheck="false" />
      <button class="btn" onclick={openTest} disabled={!!check?.errors.length}><Icon name="mic" size={14} />{t('Tester…')}</button>
    </div>
    <div class="checks" aria-live="polite">
      {#if check}
        <p class="faint small">
          {tn(check.syllables, '{n} syllabe', '{n} syllabes')} · {tn(check.meetings_scanned, 'analysé sur {n} réunion', 'analysé sur {n} réunions')}
          {#if checking}{' · '}{t('vérification…')}{/if}
        </p>
        {#each check.errors as e}<p class="msg err"><Icon name="x" size={13} />{stripLeadingIcon(e)}</p>{/each}
        {#each check.warnings as w}<p class="msg warn"><Icon name="warn" size={13} />{stripLeadingIcon(w)}</p>{/each}
        {#if !check.errors.length && !check.warnings.length}<p class="msg ok"><Icon name="check" size={13} />{t('Aucun risque repéré.')}</p>{/if}
      {/if}
    </div>
  </div>
</div>

<div class="field">
  <div class="field-label">{t('Mode strict')}<span class="hint">{t('Exige un préfixe avant le mot d’appel.')}</span></div>
  <div class="field-body">
    <label class="check toggle"><span>{t('Exiger un préfixe (« OK {w} », « Dis {w} »)', { w: settings.voice.wake_word })}</span><input class="switch" type="checkbox" role="switch" bind:checked={settings.voice.strict} /></label>
    {#if settings.voice.strict}
      <TagInput bind:values={settings.voice.prefixes} label={t('Préfixes')} placeholder={t('Ajouter un préfixe…')} />
    {/if}
  </div>
</div>

<div class="field">
  <div class="field-label">{t('Variantes acceptées')}<span class="hint">{t('Erreurs de transcription courantes du mot d’appel. Les mots courants en réunion sont refusés.')}</span></div>
  <div class="field-body">
    <TagInput bind:values={settings.voice.variants} label={t('Variantes')} placeholder={t('Ajouter une variante…')} onadd={onAddVariant} />
    {#if rejectedNow}
      <p class="msg warn small"><Icon name="warn" size={13} />{t('« {variant} » refusée : {reason}', { variant: rejectedNow.variant, reason: rejectedNow.reason })}</p>
    {/if}
    <div class="row">
      <button class="btn sm" onclick={generate}><Icon name="refresh" size={13} />{t('Générer les variantes')}</button>
    </div>
    {#if generated?.rejected.length}
      <div class="rejected">
        <p class="label">{t('Variantes rejetées')}</p>
        <ul>
          {#each generated.rejected as r (r.variant)}
            <li><code>{r.variant}</code><span class="faint small">{r.reason}</span></li>
          {/each}
        </ul>
      </div>
    {/if}
  </div>
</div>

<div class="field">
  <div class="field-label">{t('Verbes par intention')}<span class="hint">{t('Le verbe qui suit le mot d’appel détermine l’action. Ex. « {w}, arrête l’écoute » termine la réunion.', { w: settings.voice.wake_word || 'Sténo' })}</span></div>
  <div class="field-body verbs">
    {#each INTENTS as it (it.id)}
      <div class="verb">
        <span class="vlabel"><strong>{t(it.label)}</strong><span class="faint small">{t(it.hint)}</span></span>
        <TagInput bind:values={() => settings.voice.verbs[it.id] ?? [], (v) => (settings.voice.verbs[it.id] = v)} label={t('Verbes : {label}', { label: t(it.label) })} />
      </div>
    {/each}
  </div>
</div>

<div class="field">
  <div class="field-label">{t('Retour sonore')}<span class="hint">{t('Quand une commande est détectée. Jamais dans l’audio de l’appel.')}</span></div>
  <div class="field-body">
    <div class="seg-ctl" role="radiogroup" aria-label={t('Retour sonore')}>
      {#each [['none', t('Aucun')], ['system', t('Son système')], ['headset', t('Casque uniquement')]] as [v, l]}
        <button role="radio" aria-checked={settings.voice.feedback === v} onclick={() => (settings.voice.feedback = v as Settings['voice']['feedback'])}>{l}</button>
      {/each}
    </div>
    {#if settings.voice.feedback !== 'none'}
      <div class="playon">
        <label class="small muted" for="feedback-device">{t('Jouer sur')}</label>
        <select
          id="feedback-device"
          class="input"
          value={settings.voice.feedback_device ?? ''}
          onchange={(e) => (settings.voice.feedback_device = e.currentTarget.value || null)}
          disabled={!outputs.length}
        >
          <option value="">{defaultOutputLabel(outputs)}</option>
          {#each outputs as d (d.id)}<option value={d.id}>{d.name}</option>{/each}
          {#if outputGone}<option value={settings.voice.feedback_device}>{t('Sortie choisie — débranchée, sortie par défaut utilisée')}</option>{/if}
        </select>
      </div>
      {#if !outputs.length && outputsLoaded}<p class="tiny faint">{t('Liste disponible dans l’app sur Mac.')}</p>{/if}
      {#if settings.voice.feedback === 'headset' && !settings.voice.feedback_device}<p class="tiny faint">{t('Joué seulement si la sortie par défaut est un casque.')}</p>{/if}
      {#if outputGone}<p class="tiny gonec">{t('Sortie choisie débranchée : le signal est joué sur la sortie par défaut.')}</p>{/if}
    {/if}
  </div>
</div>

<div class="field">
  <div class="field-label">{t('Aparté')}</div>
  <div class="field-body">
    <p class="muted small">{t('Coupez votre micro dans l’app de visio (Teams, Slack, Zoom…) et maintenez')} <kbd>{settings.audio.shortcuts.aside}</kbd> {t('en parlant : pas besoin de mot d’appel, et vos phrases restent privées. Raccourci modifiable dans « Audio & transcription ».')}</p>
  </div>
</div>

{#if testing}
  <WakeTest bind:settings onclose={() => (testing = false)} />
{/if}

<style>
  .playon { display: grid; grid-template-columns: auto minmax(0, 300px); align-items: center; gap: 10px; }
  .gonec { color: var(--warn); }
  .wake { width: 220px; font-weight: 600; }
  .checks { display: flex; flex-direction: column; gap: 3px; min-height: 20px; }
  .msg { display: flex; align-items: flex-start; gap: 6px; font-size: 12.5px; line-height: 1.4; }
  .msg :global(.icon) { margin-top: 2px; }
  .msg.err { color: var(--err); }
  .msg.warn { color: var(--warn); }
  .msg.ok { color: var(--ok); }
  .rejected { padding: 10px 12px; border-radius: 10px; background: var(--surface-2); border: 1px solid var(--line); }
  .rejected ul { list-style: none; margin: 6px 0 0; padding: 0; }
  .rejected li { display: flex; gap: 12px; align-items: baseline; padding: 3px 0; }
  .rejected code { min-width: 96px; color: var(--text); text-decoration: line-through; text-decoration-color: color-mix(in srgb, var(--err) 60%, transparent); }
  .verbs { gap: 10px; }
  .vlabel { display: flex; flex-direction: column; line-height: 1.35; }
  .vlabel strong { font-weight: 600; }
  .verb { display: grid; grid-template-columns: 170px 1fr; gap: 12px; align-items: center; }
</style>
