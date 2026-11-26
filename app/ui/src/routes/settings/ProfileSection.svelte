<script lang="ts">
  import type { Settings } from '../../lib/types';
  let { settings = $bindable() }: { settings: Settings } = $props();

  const patternError = $derived.by(() => {
    try {
      new RegExp(settings.profile.jira_key_pattern);
      return null;
    } catch {
      return 'Expression régulière invalide.';
    }
  });
</script>

<p class="intro muted">Aide l’agent à rédiger les comptes rendus (« Moi » = vous). Le nom n’est jamais exporté.</p>
<div class="field">
  <label class="field-label" for="p-name">Nom</label>
  <div class="field-body"><input id="p-name" class="input w" bind:value={settings.profile.name} /></div>
</div>
<div class="field">
  <label class="field-label" for="p-role">Rôle</label>
  <div class="field-body"><input id="p-role" class="input w" bind:value={settings.profile.role} placeholder="Ex. développeuse back-end" /></div>
</div>
<div class="field">
  <label class="field-label" for="p-team">Équipe</label>
  <div class="field-body"><input id="p-team" class="input w" bind:value={settings.profile.team} /></div>
</div>
<div class="field">
  <label class="field-label" for="p-jira">Motif des clés Jira<span class="hint">Expression régulière.</span></label>
  <div class="field-body">
    <input id="p-jira" class="input w mono" bind:value={settings.profile.jira_key_pattern} aria-invalid={!!patternError} />
    {#if patternError}<p class="small err">{patternError}</p>{/if}
  </div>
</div>

<style>
  .intro { margin: 14px 0 6px; }
  .w { max-width: 320px; }
  .err { color: var(--err); }
</style>
