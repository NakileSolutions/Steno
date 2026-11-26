<script lang="ts">
  import type { Settings } from '../../lib/types';
  import TagInput from '../../lib/components/TagInput.svelte';

  let { settings = $bindable() }: { settings: Settings } = $props();
  const MODES = [
    { id: 'off', label: 'Désactivée', hint: 'Jamais de corrélation.' },
    { id: 'on_demand', label: 'À la demande', hint: 'Bouton « Corréler » sur le compte rendu. Recommandé.' },
    { id: 'auto', label: 'Automatique', hint: 'Lancée juste après chaque compte rendu. Consomme plus de quota.' },
  ] as const;
</script>

<div class="field">
  <div class="field-label">Mode<span class="hint">Croise le compte rendu avec le code et les connecteurs cochés.</span></div>
  <div class="field-body" role="radiogroup" aria-label="Mode de corrélation">
    {#each MODES as m (m.id)}
      <label class="check opt"><input type="radio" name="corr" value={m.id} bind:group={settings.correlation.mode} /><span><strong>{m.label}</strong> <span class="faint small">— {m.hint}</span></span></label>
    {/each}
  </div>
</div>
<div class="field">
  <label class="field-label" for="budget">Budget<span class="hint">Appels d’outils au maximum par corrélation.</span></label>
  <div class="field-body"><input id="budget" class="input num" type="number" min="5" max="200" bind:value={settings.correlation.budget} /></div>
</div>
<div class="field">
  <div class="field-label">Exclusions<span class="hint">Jamais de corrélation si le titre contient l’un de ces mots. Jamais pour une réunion confidentielle.</span></div>
  <div class="field-body"><TagInput bind:values={settings.correlation.exclude_words} label="Mots d’exclusion" /></div>
</div>

<style>
  .opt { align-items: flex-start; padding: 3px 0; }
  .opt input { margin-top: 3px; }
  .num { width: 100px; }
</style>
