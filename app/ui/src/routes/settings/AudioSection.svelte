<script lang="ts">
  import type { Settings } from '../../lib/types';
  import TagInput from '../../lib/components/TagInput.svelte';
  import CallApps from './CallApps.svelte';
  import InputDevice from './InputDevice.svelte';

  let { settings = $bindable() }: { settings: Settings } = $props();
  const SHORTCUTS = [
    { id: 'toggle', label: 'Démarrer / arrêter l’écoute' },
    { id: 'aside', label: 'Aparté (maintenir)' },
    { id: 'quick_note', label: 'Note rapide' },
  ] as const;
</script>

<InputDevice bind:settings={settings} />

<CallApps bind:settings={settings} />

<h2 class="group-title">Transcription</h2>
<div class="field">
  <label class="field-label" for="silence">Fin de phrase<span class="hint">Silence qui termine une phrase.</span></label>
  <div class="field-body">
    <div class="row">
      <input id="silence" type="range" min="400" max="2500" step="100" bind:value={settings.audio.silence_ms} />
      <span class="val small tnum">{settings.audio.silence_ms} ms</span>
    </div>
  </div>
</div>
<div class="field">
  <div class="field-label">Vocabulaire<span class="hint">Noms de services, d’équipe, produits : aide la transcription.</span></div>
  <div class="field-body"><TagInput bind:values={settings.audio.vocabulary} label="Vocabulaire" placeholder="ms-booking, ORION…" /></div>
</div>
<div class="field">
  <div class="field-label">Raccourcis globaux<span class="hint">Ex. Ctrl+Alt+S, Cmd+Shift+Space. La fin d’un appel n’arrête pas l’écoute : bouton, raccourci, barre d’état ou voix.</span></div>
  <div class="field-body">
    {#each SHORTCUTS as sc (sc.id)}
      <label class="sc"><span>{sc.label}</span><input class="input mono" bind:value={settings.audio.shortcuts[sc.id]} /></label>
    {/each}
  </div>
</div>

<style>
  .group-title { margin: 28px 0 -12px 4px; font-size: var(--fs-meta); font-weight: 600; color: var(--text-2); letter-spacing: 0.01em; }
  input[type='range'] { width: 280px; accent-color: var(--accent); }
  .val { min-width: 64px; height: 24px; display: inline-grid; place-items: center; padding: 0 8px; border-radius: var(--r-pill); background: var(--surface-2); color: var(--text-2); font-weight: 500; }
  .sc { display: grid; grid-template-columns: 1fr 220px; align-items: center; gap: 12px; color: var(--text-2); }
</style>
