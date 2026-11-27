<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../lib/api';
  import { setPrefs } from '../lib/prefs.svelte';
  import { router, setQuery } from '../lib/router.svelte';
  import { SETTINGS_SECTIONS } from '../lib/sections';
  import type { Settings } from '../lib/types';
  import { toast, toastError } from '../lib/ui.svelte';
  import Icon from '../lib/components/Icon.svelte';
  import AgentSection from './settings/AgentSection.svelte';
  import ReposSection from './settings/ReposSection.svelte';
  import ConnectorsSection from './settings/ConnectorsSection.svelte';
  import CorrelationSection from './settings/CorrelationSection.svelte';
  import VoiceSection from './settings/VoiceSection.svelte';
  import AudioSection from './settings/AudioSection.svelte';
  import PrivacySection from './settings/PrivacySection.svelte';
  import ProfileSection from './settings/ProfileSection.svelte';
  import ImportSection from './settings/ImportSection.svelte';

  let settings = $state<Settings | null>(null);
  let saved = $state('');
  let saving = $state(false);

  const section = $derived(SETTINGS_SECTIONS.find((s) => s.id === router.route.query.section) ?? SETTINGS_SECTIONS[0]);
  const dirty = $derived(!!settings && JSON.stringify(settings) !== saved);
  /** Sections enregistrées immédiatement (tables à part), sans bouton Enregistrer. */
  const immediate = $derived(section.id === 'repos' || section.id === 'connectors' || section.id === 'import');

  onMount(async () => {
    try {
      const s = await api.getSettings();
      settings = s;
      saved = JSON.stringify(s);
    } catch (e) {
      toastError(e);
    }
  });

  async function save() {
    if (!settings) return;
    saving = true;
    try {
      const s = await api.saveSettings($state.snapshot(settings) as Settings);
      setPrefs(s);
      settings = s;
      saved = JSON.stringify(s);
      toast('Réglages enregistrés.');
    } catch (e) {
      toastError(e);
    } finally {
      saving = false;
    }
  }
  function revert() {
    if (saved) settings = JSON.parse(saved);
  }
  function replaced(s: Settings) {
    setPrefs(s);
    settings = s;
    saved = JSON.stringify(s);
  }

  function onkeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 's') {
      e.preventDefault();
      if (dirty) save();
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="settings">
  <header class="shead row">
    <span class="sic"><Icon name={section.icon} size={18} /></span>
    <h1 class="grow">{section.label}</h1>
    <select class="input secsel" value={section.id} onchange={(e) => setQuery({ section: e.currentTarget.value })} aria-label="Section des réglages">
      {#each SETTINGS_SECTIONS as s (s.id)}<option value={s.id}>{s.label}</option>{/each}
    </select>
    {#if dirty}
      <span class="faint small">Modifications non enregistrées</span>
      <button class="btn" onclick={revert}>Annuler</button>
    {/if}
    {#if !immediate || dirty}
      <button class="btn primary" onclick={save} disabled={!dirty || saving} title="Enregistrer (⌘S)">{saving ? 'Enregistrement…' : 'Enregistrer'}</button>
    {/if}
  </header>

  <div class="sbody scroll">
    <div class="inner">
      {#if settings}
        {#if section.id === 'agent'}<AgentSection bind:settings={settings} />
        {:else if section.id === 'repos'}<ReposSection />
        {:else if section.id === 'connectors'}<ConnectorsSection provider={settings.agent.provider} />
        {:else if section.id === 'correlation'}<CorrelationSection bind:settings={settings} />
        {:else if section.id === 'voice'}<VoiceSection bind:settings={settings} ensureSaved={async () => { if (dirty) await save(); }} />
        {:else if section.id === 'audio'}<AudioSection bind:settings={settings} />
        {:else if section.id === 'privacy'}<PrivacySection bind:settings={settings} />
        {:else if section.id === 'profile'}<ProfileSection bind:settings={settings} />
        {:else}<ImportSection onimported={replaced} {dirty} />{/if}
      {:else}
        <p class="faint">Chargement…</p>
      {/if}
    </div>
  </div>
</div>

<style>
  .settings { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .shead { gap: 10px; padding: 16px 32px 14px; border-bottom: 1px solid var(--line); background: var(--bg); flex: none; min-height: 62px; }
  .sic { color: var(--text-2); display: flex; }
  .secsel { width: auto; height: 26px; font-size: 12.5px; display: none; }
  :global(.shell.collapsed) .secsel { display: block; }
  .sbody { flex: 1; min-height: 0; }
  .inner { max-width: 860px; padding: 8px 32px 56px; }
</style>
