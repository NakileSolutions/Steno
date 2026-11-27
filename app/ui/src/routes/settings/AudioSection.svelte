<script lang="ts">
  import type { Settings } from '../../lib/types';
  import TagInput from '../../lib/components/TagInput.svelte';

  let { settings = $bindable() }: { settings: Settings } = $props();
  const SHORTCUTS = [
    { id: 'toggle', label: 'Démarrer / arrêter l’écoute' },
    { id: 'aside', label: 'Aparté (maintenir)' },
    { id: 'quick_note', label: 'Note rapide' },
  ] as const;
</script>

<div class="field">
  <div class="field-label">Teams<span class="hint">L’écoute ne démarre et ne s’arrête jamais toute seule.</span></div>
  <div class="field-body">
    <label class="check toggle"><span>Proposer de démarrer l’écoute quand un appel Teams commence (notification seulement)</span><input class="switch" type="checkbox" role="switch" bind:checked={settings.audio.auto_detect_teams} /></label>
    <p class="small faint">La fin de l’appel Teams ne termine pas la réunion : arrêtez l’écoute avec le bouton, le raccourci, le menu de la barre d’état ou à la voix.</p>
  </div>
</div>
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
  <div class="field-label">Raccourcis globaux<span class="hint">Ex. Ctrl+Alt+S, Cmd+Shift+Space.</span></div>
  <div class="field-body">
    {#each SHORTCUTS as sc (sc.id)}
      <label class="sc"><span>{sc.label}</span><input class="input mono" bind:value={settings.audio.shortcuts[sc.id]} /></label>
    {/each}
  </div>
</div>

<style>
  input[type='range'] { width: 280px; accent-color: var(--accent); }
  .val { min-width: 64px; height: 24px; display: inline-grid; place-items: center; padding: 0 8px; border-radius: var(--r-pill); background: var(--surface-2); color: var(--text-2); font-weight: 500; }
  .sc { display: grid; grid-template-columns: 1fr 220px; align-items: center; gap: 12px; color: var(--text-2); }
</style>
