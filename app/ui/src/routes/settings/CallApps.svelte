<script lang="ts">
  // Apps de visio surveillées (audio.call_apps) et proposition de démarrage (audio.auto_detect_calls).
  import type { Settings } from '../../lib/types';
  import { callAppId, isBuiltinCallApp, isValidBundle, parseBundles } from '../../lib/listening';
  import Icon from '../../lib/components/Icon.svelte';

  let { settings = $bindable() }: { settings: Settings } = $props();

  let adding = $state(false);
  let name = $state('');
  let bundleText = $state('');
  let error = $state('');
  let nameInput: HTMLInputElement | undefined = $state();

  const newId = $derived(callAppId(name, settings.audio.call_apps.map((a) => a.id)));

  function open() {
    adding = true;
    error = '';
    setTimeout(() => nameInput?.focus(), 0);
  }
  function cancel() {
    adding = false;
    name = '';
    bundleText = '';
    error = '';
  }
  function add(e: SubmitEvent) {
    e.preventDefault();
    const n = name.trim();
    const bundles = parseBundles(bundleText);
    if (!n) return void (error = 'Donnez un nom à l’app.');
    if (!bundles.length) return void (error = 'Indiquez l’identifiant de bundle de l’app (ex. com.example.app).');
    const bad = bundles.find((b) => !isValidBundle(b));
    if (bad) return void (error = `« ${bad} » n’est pas un identifiant de bundle (ex. com.example.app).`);
    settings.audio.call_apps.push({ id: newId, name: n, bundles, enabled: true, browser: false });
    cancel();
  }
  function remove(id: string) {
    settings.audio.call_apps = settings.audio.call_apps.filter((a) => a.id !== id);
  }
</script>

<h2 class="group-title">Apps de visio</h2>
<div class="apps" role="group" aria-label="Apps de visio">
  <label class="arow">
    <span class="grow">
      <span class="an">Proposer de démarrer l’écoute quand un appel commence</span>
      <span class="sub small faint">Une notification seulement : l’appel reste aussi signalé dans Sténo.</span>
    </span>
    <input class="switch" type="checkbox" role="switch" bind:checked={settings.audio.auto_detect_calls} />
  </label>

  {#each settings.audio.call_apps as app (app.id)}
    <div class="arow app" class:off={!app.enabled} data-app={app.id}>
      <span class="aic" aria-hidden="true"><Icon name={app.browser ? 'external' : 'phone'} size={15} /></span>
      <span class="grow">
        <span class="an"><span class="ellipsis">{app.name}</span>{#if !isBuiltinCallApp(app.id)}<span class="badge outline">Ajoutée</span>{/if}</span>
        <span class="bundles" title={app.bundles.join('\n')}>{app.bundles.join(' · ')}</span>
        {#if app.browser}
          <span class="awarn small"><Icon name="warn" size={12} />Attention : capte tout le son du navigateur, y compris les autres onglets.</span>
        {/if}
      </span>
      {#if !isBuiltinCallApp(app.id)}
        <button class="btn ghost sm icon" type="button" onclick={() => remove(app.id)} aria-label="Supprimer {app.name}" title="Supprimer"><Icon name="trash" size={13} /></button>
      {/if}
      <input class="switch" type="checkbox" role="switch" bind:checked={app.enabled} aria-label="Surveiller {app.name}" />
    </div>
  {/each}

  {#if adding}
    <form class="arow addform" onsubmit={add} aria-label="Ajouter une app">
      <span class="aic" aria-hidden="true"><Icon name="plus" size={15} /></span>
      <div class="grow col">
        <div class="inputs">
          <label class="col"><span class="small muted">Nom</span><input class="input" bind:this={nameInput} bind:value={name} placeholder="Google Chat" oninput={() => (error = '')} /></label>
          <label class="col"><span class="small muted">Identifiant de bundle</span><input class="input mono" bind:value={bundleText} placeholder="com.example.app" spellcheck="false" autocapitalize="off" oninput={() => (error = '')} /></label>
        </div>
        <p class="tiny faint">
          {#if name.trim()}Identifiant : <code>{newId}</code> · {/if}Plusieurs préfixes possibles, séparés par des virgules. Les processus auxiliaires dont le bundle commence ainsi sont inclus.
        </p>
        {#if error}<p class="aerr small" role="alert">{error}</p>{/if}
        <div class="row">
          <button class="btn primary sm" type="submit">Ajouter</button>
          <button class="btn sm" type="button" onclick={cancel}>Annuler</button>
        </div>
      </div>
    </form>
  {:else}
    <div class="arow foot">
      <button class="btn sm" type="button" onclick={open}><Icon name="plus" size={13} />Ajouter une app</button>
    </div>
  {/if}
</div>
<p class="reminder small faint"><Icon name="info" size={12} />L’écoute ne démarre jamais seule ; une fois lancée, le son capté est celui de l’app de l’appel en cours.</p>

<style>
  .group-title { margin: 24px 0 8px 4px; font-size: var(--fs-meta); font-weight: 600; color: var(--text-2); letter-spacing: 0.01em; }
  .apps {
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-sm);
    overflow: hidden;
  }
  .arow { display: flex; align-items: center; gap: 12px; padding: 12px 16px; position: relative; }
  .arow + .arow::before { content: ''; position: absolute; top: 0; left: 16px; right: 16px; border-top: 1px solid var(--line); }
  .arow.app + .arow::before, .arow.app::before { left: 56px; }
  .arow > .grow { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .an { display: inline-flex; align-items: center; gap: 8px; font-weight: 500; color: var(--text); }
  .an .badge { height: 18px; }
  .sub { line-height: 1.45; }
  .aic {
    width: 28px;
    height: 28px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    flex: none;
    background: var(--accent-soft);
    color: var(--accent-text);
  }
  .off .aic { background: var(--surface-2); color: var(--text-3); }
  .off .an { color: var(--text-2); }
  .bundles { font-family: var(--mono); font-size: 11px; color: var(--text-3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .awarn { display: inline-flex; align-items: center; gap: 6px; margin-top: 4px; color: var(--warn); font-weight: 500; }
  .foot { padding: 10px 16px; }
  .addform { align-items: flex-start; background: var(--surface-2); }
  .addform .col { gap: 6px; }
  .inputs { display: grid; grid-template-columns: 1fr 1.3fr; gap: 10px; }
  .inputs .input { height: 30px; }
  .addform code { font-family: var(--mono); color: var(--text-2); }
  .aerr { color: var(--err); }
  .reminder { display: flex; align-items: center; gap: 6px; margin: 8px 4px 0; }
</style>
