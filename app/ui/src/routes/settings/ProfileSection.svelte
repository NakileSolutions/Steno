<script lang="ts">
  import type { Settings } from '../../lib/types';
  import { t } from '../../lib/i18n.svelte';
  let { settings = $bindable() }: { settings: Settings } = $props();

  const patternError = $derived.by(() => {
    try {
      new RegExp(settings.profile.jira_key_pattern);
      return null;
    } catch {
      return t('Expression régulière invalide.');
    }
  });
</script>

<div class="field">
  <label class="field-label" for="p-lang">{t('Langue')}<span class="hint">{t('Interface et comptes rendus. Auto suit la langue de macOS.')}</span></label>
  <div class="field-body">
    <select id="p-lang" class="input w" value={settings.profile.language ?? 'auto'} onchange={(e) => (settings.profile.language = e.currentTarget.value as 'auto' | 'fr' | 'en')}>
      <option value="auto">{t('Auto')}</option>
      <option value="fr">Français</option>
      <option value="en">English</option>
    </select>
  </div>
</div>
<div class="field">
  <div class="field-label">{t('Apparence')}<span class="hint">{t('Auto suit le réglage de macOS.')}</span></div>
  <div class="field-body">
    <div class="seg-ctl" role="radiogroup" aria-label={t('Apparence')}>
      {#each [['auto', t('Auto')], ['light', t('Clair')], ['dark', t('Sombre')]] as [v, l] (v)}
        <button role="radio" aria-checked={(settings.profile.theme ?? 'auto') === v} onclick={() => (settings.profile.theme = v as 'auto' | 'light' | 'dark')}>{l}</button>
      {/each}
    </div>
  </div>
</div>

<p class="intro muted">{t('Aide l’agent à rédiger les comptes rendus (« Moi » = vous). Le nom n’est jamais exporté.')}</p>
<div class="field">
  <label class="field-label" for="p-name">{t('Nom')}</label>
  <div class="field-body"><input id="p-name" class="input w" bind:value={settings.profile.name} /></div>
</div>
<div class="field">
  <label class="field-label" for="p-role">{t('Rôle')}</label>
  <div class="field-body"><input id="p-role" class="input w" bind:value={settings.profile.role} placeholder={t('Ex. développeuse back-end')} /></div>
</div>
<div class="field">
  <label class="field-label" for="p-team">{t('Équipe')}</label>
  <div class="field-body"><input id="p-team" class="input w" bind:value={settings.profile.team} /></div>
</div>
<div class="field">
  <label class="field-label" for="p-jira">{t('Motif des clés Jira')}<span class="hint">{t('Expression régulière.')}</span></label>
  <div class="field-body">
    <input id="p-jira" class="input w mono" bind:value={settings.profile.jira_key_pattern} aria-invalid={!!patternError} />
    {#if patternError}<p class="small err">{patternError}</p>{/if}
  </div>
</div>

<style>
  .intro { margin: 20px 0 0; line-height: 1.5; }
  .w { max-width: 320px; }
  .err { color: var(--err); }
</style>
