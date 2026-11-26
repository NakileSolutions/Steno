<script lang="ts">
  import { untrack } from 'svelte';
  import { api } from '../../lib/api';
  import type { Settings, VariantCheck, VoiceVerbs, WakeCheck } from '../../lib/types';
  import { debounce } from '../../lib/format';
  import { toastError } from '../../lib/ui.svelte';
  import Icon from '../../lib/components/Icon.svelte';
  import TagInput from '../../lib/components/TagInput.svelte';
  import WakeTest from './WakeTest.svelte';

  let { settings = $bindable(), ensureSaved }: { settings: Settings; ensureSaved: () => Promise<void> } = $props();

  let check = $state<WakeCheck | null>(null);
  let checking = $state(false);
  let generated = $state<VariantCheck | null>(null);
  let rejectedNow = $state<{ variant: string; reason: string } | null>(null);
  let testing = $state(false);

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
    { id: 'search', label: 'Rechercher', hint: 'recherche croisée' },
    { id: 'check', label: 'Vérifier', hint: 'vérification ciblée' },
    { id: 'note', label: 'Noter', hint: 'aucune recherche' },
    { id: 'recap', label: 'Récapituler', hint: 'résumé rapide' },
    { id: 'cancel', label: 'Annuler', hint: 'dernière demande' },
  ];
</script>

<div class="field">
  <label class="field-label" for="wake">Mot d’appel<span class="hint">Doit être dans les 2 premiers mots de la phrase, sur votre micro uniquement.</span></label>
  <div class="field-body">
    <div class="row">
      <input id="wake" class="input lg wake" bind:value={settings.voice.wake_word} autocomplete="off" spellcheck="false" />
      <button class="btn" onclick={openTest} disabled={!!check?.errors.length}><Icon name="mic" size={14} />Tester…</button>
    </div>
    <div class="checks" aria-live="polite">
      {#if check}
        <p class="faint small">
          {check.syllables} syllabe{check.syllables > 1 ? 's' : ''} · analysé sur {check.meetings_scanned} réunion{check.meetings_scanned > 1 ? 's' : ''}
          {#if checking}{' · vérification…'}{/if}
        </p>
        {#each check.errors as e}<p class="msg err"><Icon name="x" size={13} />{e}</p>{/each}
        {#each check.warnings as w}<p class="msg warn"><Icon name="warn" size={13} />{w}</p>{/each}
        {#if !check.errors.length && !check.warnings.length}<p class="msg ok"><Icon name="check" size={13} />Aucun risque repéré.</p>{/if}
      {/if}
    </div>
  </div>
</div>

<div class="field">
  <div class="field-label">Mode strict<span class="hint">Exige un préfixe avant le mot d’appel.</span></div>
  <div class="field-body">
    <label class="check"><input type="checkbox" bind:checked={settings.voice.strict} /> Exiger un préfixe (« OK {settings.voice.wake_word} », « Dis {settings.voice.wake_word} »)</label>
    {#if settings.voice.strict}
      <TagInput bind:values={settings.voice.prefixes} label="Préfixes" placeholder="Ajouter un préfixe…" />
    {/if}
  </div>
</div>

<div class="field">
  <div class="field-label">Variantes acceptées<span class="hint">Erreurs de transcription courantes du mot d’appel. Les mots courants en réunion sont refusés.</span></div>
  <div class="field-body">
    <TagInput bind:values={settings.voice.variants} label="Variantes" placeholder="Ajouter une variante…" onadd={onAddVariant} />
    {#if rejectedNow}
      <p class="msg warn small"><Icon name="warn" size={13} />« {rejectedNow.variant} » refusée : {rejectedNow.reason}</p>
    {/if}
    <div class="row">
      <button class="btn sm" onclick={generate}><Icon name="refresh" size={13} />Générer les variantes</button>
    </div>
    {#if generated?.rejected.length}
      <div class="rejected">
        <p class="label">Variantes rejetées</p>
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
  <div class="field-label">Verbes par intention<span class="hint">Le verbe qui suit le mot d’appel détermine l’action.</span></div>
  <div class="field-body verbs">
    {#each INTENTS as it (it.id)}
      <div class="verb">
        <span class="vlabel"><strong>{it.label}</strong><span class="faint small">{it.hint}</span></span>
        <TagInput bind:values={settings.voice.verbs[it.id]} label="Verbes : {it.label}" />
      </div>
    {/each}
  </div>
</div>

<div class="field">
  <div class="field-label">Retour sonore<span class="hint">Quand une commande est détectée. Jamais dans l’audio de Teams.</span></div>
  <div class="field-body">
    <div class="seg-ctl" role="radiogroup" aria-label="Retour sonore">
      {#each [['none', 'Aucun'], ['system', 'Son système'], ['headset', 'Casque uniquement']] as [v, l]}
        <button role="radio" aria-checked={settings.voice.feedback === v} onclick={() => (settings.voice.feedback = v as Settings['voice']['feedback'])}>{l}</button>
      {/each}
    </div>
  </div>
</div>

<div class="field">
  <div class="field-label">Aparté</div>
  <div class="field-body">
    <p class="muted small">Coupez votre micro dans Teams et maintenez <kbd>{settings.audio.shortcuts.aside}</kbd> en parlant : pas besoin de mot d’appel, et vos phrases restent privées. Raccourci modifiable dans « Audio & transcription ».</p>
  </div>
</div>

{#if testing}
  <WakeTest bind:settings onclose={() => (testing = false)} />
{/if}

<style>
  .wake { width: 220px; font-weight: 600; }
  .checks { display: flex; flex-direction: column; gap: 3px; min-height: 20px; }
  .msg { display: flex; align-items: flex-start; gap: 6px; font-size: 12.5px; line-height: 1.4; }
  .msg :global(.icon) { margin-top: 2px; }
  .msg.err { color: var(--err); }
  .msg.warn { color: var(--warn); }
  .msg.ok { color: var(--ok); }
  .rejected { padding: 8px 10px; border-radius: 8px; background: var(--surface-2); }
  .rejected ul { list-style: none; margin: 4px 0 0; padding: 0; }
  .rejected li { display: flex; gap: 10px; align-items: baseline; padding: 2px 0; }
  .rejected code { min-width: 90px; }
  .verbs { gap: 8px; }
  .vlabel { display: flex; flex-direction: column; line-height: 1.3; }
  .verb { display: grid; grid-template-columns: 190px 1fr; gap: 10px; align-items: center; }
</style>
