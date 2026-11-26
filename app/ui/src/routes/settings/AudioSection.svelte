<script lang="ts">
  import type { Settings } from '../../lib/types';
  import TagInput from '../../lib/components/TagInput.svelte';

  let { settings = $bindable() }: { settings: Settings } = $props();
  const SHORTCUTS = [
    { id: 'toggle', label: 'Démarrer / arrêter' },
    { id: 'aside', label: 'Aparté (maintenir)' },
    { id: 'quick_note', label: 'Note rapide' },
  ] as const;
</script>

<div class="field">
  <div class="field-label">Teams</div>
  <div class="field-body">
    <label class="check"><input type="checkbox" bind:checked={settings.audio.auto_detect_teams} /> Démarrer automatiquement quand un appel Teams commence</label>
    <label class="row small muted">Fin de réunion après
      <input class="input num" type="number" min="10" max="600" step="10" bind:value={settings.audio.end_grace_s} /> s sans micro utilisé par Teams
    </label>
  </div>
</div>
<div class="field">
  <label class="field-label" for="silence">Fin de phrase<span class="hint">Silence qui termine une phrase.</span></label>
  <div class="field-body">
    <div class="row">
      <input id="silence" type="range" min="400" max="2500" step="100" bind:value={settings.audio.silence_ms} />
      <span class="small mono">{settings.audio.silence_ms} ms</span>
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
  .num { width: 76px; height: 24px; }
  .row.small { gap: 6px; }
  input[type='range'] { width: 260px; accent-color: var(--brand); }
  .sc { display: grid; grid-template-columns: 170px 200px; align-items: center; gap: 10px; }
</style>
