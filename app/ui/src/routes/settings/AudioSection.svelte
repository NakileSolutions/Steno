<script lang="ts">
  import type { Settings } from '../../lib/types';
  import TagInput from '../../lib/components/TagInput.svelte';
  import CallApps from './CallApps.svelte';
  import InputDevice from './InputDevice.svelte';
  import { t, tk } from '../../lib/i18n.svelte';
  import { isLinux } from '../../lib/platform';
  import DesktopShortcuts from '../../lib/components/DesktopShortcuts.svelte';

  let { settings = $bindable() }: { settings: Settings } = $props();
  const SHORTCUTS = [
    { id: 'toggle', label: tk('Démarrer / arrêter l’écoute') },
    { id: 'aside', label: tk('Aparté (maintenir)') },
    { id: 'quick_note', label: tk('Note rapide') },
  ] as const;
  const FINAL_LANGUAGES = [
    ['off', tk('Désactivée (garder le direct)')],
    ['fr', tk('Français')],
    ['en', tk('Anglais')],
    ['es', tk('Espagnol')],
    ['de', tk('Allemand')],
    ['it', tk('Italien')],
    ['pt', tk('Portugais')],
    ['nl', tk('Néerlandais')],
  ] as const;
</script>

<InputDevice bind:settings={settings} />

<CallApps bind:settings={settings} />

<h2 class="group-title">{t('Transcription')}</h2>
<div class="field">
  <label class="field-label" for="silence">{t('Fin de phrase')}<span class="hint">{t('Silence qui termine une phrase.')}</span></label>
  <div class="field-body">
    <div class="row">
      <input id="silence" type="range" min="400" max="2500" step="100" bind:value={settings.audio.silence_ms} />
      <span class="val small tnum">{settings.audio.silence_ms} ms</span>
    </div>
  </div>
</div>
<div class="field">
  <label class="field-label" for="final-tr">{t('Retranscription finale')}<span class="hint">{t('À la fin de la réunion, un modèle plus précis réécrit la transcription dans la langue choisie (évite le mélange français / anglais). Premier usage : téléchargement d’environ 1 Go.')}</span></label>
  <div class="field-body">
    <select id="final-tr" class="input" value={settings.audio.final_transcription ?? 'fr'} onchange={(e) => (settings.audio.final_transcription = e.currentTarget.value)}>
      {#each FINAL_LANGUAGES as [v, l] (v)}<option value={v}>{t(l)}</option>{/each}
    </select>
  </div>
</div>
<div class="field">
  <div class="field-label">{t('Vocabulaire')}<span class="hint">{t('Noms de services, d’équipe, produits : aide la transcription.')}</span></div>
  <div class="field-body"><TagInput bind:values={settings.audio.vocabulary} label={t('Vocabulaire')} placeholder="ms-booking, ORION…" /></div>
</div>
<div class="field">
  <div class="field-label">{t('Raccourcis globaux')}<span class="hint">{isLinux ? t('Ex. Ctrl+Alt+S, Super+Shift+Space. La fin d’un appel n’arrête pas l’écoute : bouton, raccourci, icône de la barre d’état ou voix.') : t('Ex. Ctrl+Alt+S, Cmd+Shift+Space. La fin d’un appel n’arrête pas l’écoute : bouton, raccourci, barre d’état ou voix.')}</span></div>
  <div class="field-body">
    {#each SHORTCUTS as sc (sc.id)}
      <label class="sc"><span>{t(sc.label)}</span><input class="input mono" bind:value={settings.audio.shortcuts[sc.id]} /></label>
    {/each}
    <DesktopShortcuts shortcuts={settings.audio.shortcuts} />
  </div>
</div>

<style>
  .group-title { margin: 28px 0 -12px 4px; font-size: var(--fs-meta); font-weight: 600; color: var(--text-2); letter-spacing: 0.01em; }
  input[type='range'] { width: 280px; accent-color: var(--accent); }
  .val { min-width: 64px; height: 24px; display: inline-grid; place-items: center; padding: 0 8px; border-radius: var(--r-pill); background: var(--surface-2); color: var(--text-2); font-weight: 500; }
  .sc { display: grid; grid-template-columns: 1fr 220px; align-items: center; gap: 12px; color: var(--text-2); }
</style>
