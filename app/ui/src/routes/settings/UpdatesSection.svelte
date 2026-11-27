<script lang="ts">
  // Mises à jour : version installée, dernière version publiée et ses notes, installation
  // depuis les sources (quelques minutes), redémarrage.
  import { api } from '../../lib/api';
  import type { Settings } from '../../lib/types';
  import { updates, initUpdates } from '../../lib/updates.svelte';
  import { shortDate, time } from '../../lib/format';
  import { toastError } from '../../lib/ui.svelte';
  import Markdown from '../../lib/components/Markdown.svelte';
  import Icon from '../../lib/components/Icon.svelte';
  import { t } from '../../lib/i18n.svelte';

  let { settings = $bindable() }: { settings: Settings } = $props();
  initUpdates();
  if (!settings.updates) settings.updates = { check: true, auto_install: false };
  const s = $derived(updates.status);
  let busy = $state(false);

  async function run(f: () => Promise<unknown>) {
    busy = true;
    try {
      await f();
    } catch (e) {
      toastError(e);
    } finally {
      busy = false;
    }
  }
  const check = () => run(async () => (updates.status = await api.checkUpdate()));
  const install = () => run(() => api.installUpdate());
  const restart = () => run(() => api.restartApp());
</script>

<p class="intro muted">{t('Sténo vérifie sur GitHub si une nouvelle version est publiée (aucune donnée n’est envoyée). La mise à jour se fait depuis vos sources : récupération de la version, puis')} <code>make install</code>{t(', sans toucher à vos données.')}</p>

<div class="card upd" aria-live="polite">
  {#if !s}
    <p class="muted">{t('Chargement…')}</p>
  {:else}
    <div class="row head">
      <div class="grow">
        <p class="ver">{t('Version installée :')} <strong class="tnum">{s.current}</strong></p>
        <p class="small muted">
          {#if s.state === 'checking'}{t('Vérification…')}
          {:else if s.latest && s.available}{t('Nouvelle version disponible :')} <strong class="tnum">{s.latest}</strong>{#if s.published_at}{' · '}{t('publiée le {date}', { date: shortDate(s.published_at) })}{/if}
          {:else if s.latest}{t('À jour (dernière version publiée : {v})', { v: s.latest })}
          {:else if s.checked_at}{t('Aucune version publiée pour l’instant')}
          {:else}{t('Pas encore vérifié')}{/if}
          {#if s.checked_at && s.state !== 'checking'}{' · '}{t('vérifié à {time}', { time: time(s.checked_at) })}{/if}
        </p>
      </div>
      {#if s.state === 'installed'}
        <button class="btn primary" onclick={restart} disabled={busy}><Icon name="refresh" size={14} />{t('Redémarrer Sténo')}</button>
      {:else if s.state === 'installing'}
        <span class="badge"><span class="spin" aria-hidden="true"></span>{t('Installation…')}</span>
      {:else if s.available}
        <button class="btn primary" onclick={install} disabled={busy || !s.can_install} title={s.can_install ? t('Installer depuis les sources (quelques minutes)') : (s.message ?? '')}><Icon name="download" size={14} />{t('Installer {v}', { v: s.latest ?? '' })}</button>
      {/if}
      <button class="btn" onclick={check} disabled={busy || s.state === 'checking' || s.state === 'installing'}>{t('Vérifier')}</button>
    </div>
    {#if s.message}<p class="small msg" class:err={s.state === 'error'}><Icon name={s.state === 'error' ? 'warn' : 'info'} size={13} />{s.message}{#if s.state === 'error' && s.log_path}{' — '}{t('journal :')} <code>{s.log_path}</code>{/if}</p>{/if}
    {#if s.available && s.notes}
      <div class="notes"><Markdown source={s.notes} /></div>
    {/if}
    {#if s.url}<p class="small"><a href={s.url} target="_blank" rel="noreferrer">{t('Voir la version sur GitHub')}</a></p>{/if}
    {#if s.source_dir}<p class="small faint">{t('Sources :')} <code>{s.source_dir}</code></p>{/if}
  {/if}
</div>

<div class="field">
  <div class="field-label">{t('Vérification')}<span class="hint">{t('Au démarrage, puis toutes les 6 heures.')}</span></div>
  <div class="field-body">
    <label class="check toggle"><span>{t('Vérifier les nouvelles versions')}</span><input class="switch" type="checkbox" role="switch" bind:checked={settings.updates!.check} /></label>
  </div>
</div>
<div class="field">
  <div class="field-label">{t('Installation automatique')}<span class="hint">{t('Jamais pendant une réunion. Le redémarrage reste à votre initiative.')}</span></div>
  <div class="field-body">
    <label class="check toggle"><span>{t('Installer les nouvelles versions automatiquement')}</span><input class="switch" type="checkbox" role="switch" bind:checked={settings.updates!.auto_install} disabled={!settings.updates!.check} /></label>
  </div>
</div>

<style>
  .intro { margin: 20px 0 14px; line-height: 1.5; }
  .upd { padding: 14px 16px; display: flex; flex-direction: column; gap: 10px; margin-bottom: 8px; }
  .head { gap: 10px; align-items: center; }
  .ver { font-size: var(--fs-body); }
  .msg { display: flex; gap: 6px; align-items: flex-start; color: var(--text-2); }
  .msg.err { color: var(--err); }
  .notes { padding: 10px 12px; border-radius: 10px; background: var(--surface-2); max-height: 260px; overflow: auto; }
  .badge { display: inline-flex; gap: 6px; align-items: center; }
  .spin { width: 12px; height: 12px; border: 1.5px solid currentColor; border-right-color: transparent; border-radius: 50%; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  code { font-size: 12px; }
</style>
