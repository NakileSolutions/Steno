<script lang="ts">
  import type { Settings } from '../../lib/types';
  import TagInput from '../../lib/components/TagInput.svelte';
  import { t, tk } from '../../lib/i18n.svelte';

  let { settings = $bindable() }: { settings: Settings } = $props();
  const MODES = [
    { id: 'off', label: tk('Désactivée'), hint: tk('Jamais de corrélation.') },
    { id: 'on_demand', label: tk('À la demande'), hint: tk('Bouton « Corréler » sur le compte rendu. Recommandé.') },
    { id: 'auto', label: tk('Automatique'), hint: tk('Lancée juste après chaque compte rendu. Consomme plus de quota.') },
  ];
</script>

<div class="field">
  <div class="field-label">{t('Mode')}<span class="hint">{t('Croise le compte rendu avec le code et les connecteurs cochés.')}</span></div>
  <div class="field-body" role="radiogroup" aria-label={t('Mode de corrélation')}>
    {#each MODES as m (m.id)}
      <label class="check opt"><input type="radio" name="corr" value={m.id} bind:group={settings.correlation.mode} /><span><strong>{t(m.label)}</strong> <span class="faint small">— {t(m.hint)}</span></span></label>
    {/each}
  </div>
</div>
<div class="field">
  <label class="field-label" for="budget">{t('Budget')}<span class="hint">{t('Appels d’outils au maximum par corrélation.')}</span></label>
  <div class="field-body"><input id="budget" class="input num" type="number" min="5" max="200" bind:value={settings.correlation.budget} /></div>
</div>
<div class="field">
  <div class="field-label">{t('Exclusions')}<span class="hint">{t('Jamais de corrélation si le titre contient l’un de ces mots. Jamais pour une réunion confidentielle.')}</span></div>
  <div class="field-body"><TagInput bind:values={settings.correlation.exclude_words} label={t('Mots d’exclusion')} /></div>
</div>

<style>
  .opt { align-items: flex-start; padding: 10px 12px; border: 1px solid var(--line); border-radius: 10px; transition: border-color var(--t-fast) var(--ease), background var(--t-fast) var(--ease); }
  .opt:hover { border-color: var(--line-strong); }
  .opt:has(input:checked) { background: var(--accent-soft); border-color: var(--accent-line); }
  .opt input { margin-top: 2px; }
  [role='radiogroup'] { gap: 6px; }
  .num { width: 100px; }
</style>
