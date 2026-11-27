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
  import Permissions from '../lib/components/Permissions.svelte';
  import ProfileSection from './settings/ProfileSection.svelte';
  import ReportsSection from './settings/ReportsSection.svelte';
  import UpdatesSection from './settings/UpdatesSection.svelte';
  import ImportSection from './settings/ImportSection.svelte';
  import { version } from '../../package.json';
  import { t } from '../lib/i18n.svelte';
  import { isLinux, modKey } from '../lib/platform';
  import DesktopShortcuts from '../lib/components/DesktopShortcuts.svelte';

  let settings = $state<Settings | null>(null);
  let saved = $state('');
  let saving = $state(false);

  const section = $derived(SETTINGS_SECTIONS.find((s) => s.id === router.route.query.section) ?? SETTINGS_SECTIONS[0]);
  const dirty = $derived(!!settings && JSON.stringify(settings) !== saved);
  /** Sections enregistrées immédiatement (tables à part), sans bouton Enregistrer. */
  const immediate = $derived(section.id === 'repos' || section.id === 'connectors' || section.id === 'import' || section.id === 'permissions');

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
      toast(t('Réglages enregistrés.'));
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
    <h1 class="grow">{t(section.label)}</h1>
    <select class="input secsel" value={section.id} onchange={(e) => setQuery({ section: e.currentTarget.value })} aria-label={t('Section des réglages')}>
      {#each SETTINGS_SECTIONS as s (s.id)}<option value={s.id}>{t(s.label)}</option>{/each}
    </select>
    {#if dirty}
      <span class="unsaved small"><span class="udot" aria-hidden="true"></span>{t('Modifications non enregistrées')}</span>
      <button class="btn" onclick={revert}>{t('Annuler')}</button>
    {/if}
    {#if !immediate || dirty}
      <button class="btn primary" onclick={save} disabled={!dirty || saving} title={t('Enregistrer ({k})', { k: `${modKey}S` })}>{saving ? t('Enregistrement…') : t('Enregistrer')}</button>
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
        {:else if section.id === 'permissions'}
          <section class="perm-sec">
            {#if isLinux}
              <p class="muted">{t('Serveur son et capture audio. Sous Linux, aucune autorisation n’est à accorder.')}</p>
              <Permissions />
              <DesktopShortcuts shortcuts={settings.audio.shortcuts} always />
            {:else}
              <p class="muted">{t('Droits accordés à Sténo par macOS. Un droit refusé ne peut être rétabli que dans les Réglages Système.')}</p>
              <Permissions />
            {/if}
          </section>
        {:else if section.id === 'reports'}<ReportsSection bind:settings={settings} />
        {:else if section.id === 'updates'}<UpdatesSection bind:settings={settings} />
        {:else if section.id === 'privacy'}<PrivacySection bind:settings={settings} />
        {:else if section.id === 'profile'}<ProfileSection bind:settings={settings} />
        {:else}<ImportSection onimported={replaced} {dirty} />{/if}
      {:else}
        <div class="skel" aria-hidden="true">
          {#each [0, 1, 2] as k (k)}<div class="sk-row"><span class="skeleton" style="width: 30%"></span><span class="skeleton" style="width: {60 - k * 12}%"></span></div>{/each}
        </div>
      {/if}
      <p class="credit">{t('Sténo {version} · Développé par Nakile Solutions', { version })}</p>
    </div>
  </div>
</div>

<style>
  .settings { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .shead { gap: 12px; padding: 18px 40px 16px; border-bottom: 1px solid var(--line); background: var(--bg); flex: none; min-height: 68px; }
  .shead h1 { font-size: var(--fs-h2); }
  .sic { width: 32px; height: 32px; border-radius: 9px; display: grid; place-items: center; background: var(--side-bg); color: #e6e9ff; box-shadow: 0 1px 2px rgba(0, 0, 0, 0.18), inset 0 0 0 1px rgba(255, 255, 255, 0.06); flex: none; }
  .secsel { width: auto; height: 28px; font-size: 12.5px; display: none; }
  :global(.shell.collapsed) .secsel { display: block; }
  .unsaved { display: inline-flex; align-items: center; gap: 7px; color: var(--warn); font-weight: 500; animation: appear var(--t) var(--ease); }
  .udot { width: 7px; height: 7px; border-radius: 50%; background: currentColor; }
  .sbody { flex: 1; min-height: 0; }
  .inner { max-width: 880px; padding: 4px 40px 64px; }
  .perm-sec { display: flex; flex-direction: column; gap: 14px; margin-top: 20px; }
  .credit { margin-top: 40px; text-align: center; font-size: 11.5px; color: var(--text-3); letter-spacing: 0.01em; }
  .skel { margin-top: 20px; padding: 20px; border-radius: var(--radius-lg); background: var(--surface); border: 1px solid var(--line); display: flex; flex-direction: column; gap: 22px; }
  .sk-row { display: grid; grid-template-columns: 200px 1fr; gap: 24px; }
  .sk-row .skeleton + .skeleton { margin-top: 0; }
</style>
