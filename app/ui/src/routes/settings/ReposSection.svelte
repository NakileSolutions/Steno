<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../../lib/api';
  import type { Repo } from '../../lib/types';
  import { confirmDialog, toast, toastError } from '../../lib/ui.svelte';
  import Icon from '../../lib/components/Icon.svelte';
  import { t } from '../../lib/i18n.svelte';

  let repos = $state<Repo[]>([]);
  let loaded = $state(false);

  onMount(async () => {
    try {
      repos = await api.listRepos();
    } catch (e) {
      toastError(e);
    } finally {
      loaded = true;
    }
  });

  async function add() {
    try {
      const path = await api.pickFolder();
      if (!path) return;
      const r = await api.addRepo(path);
      repos.push(r);
      toast(t('Dépôt « {label} » ajouté.', { label: r.label }));
    } catch (e) {
      toastError(e);
    }
  }
  async function update(r: Repo) {
    try {
      await api.updateRepo($state.snapshot(r) as Repo);
    } catch (e) {
      toastError(e);
    }
  }
  async function remove(r: Repo) {
    if (!(await confirmDialog({ title: t('Retirer « {label} » ?', { label: r.label }), message: t('Le dossier ne sera plus lisible par l’agent. Rien n’est supprimé sur le disque.'), confirm: t('Retirer'), danger: true }))) return;
    try {
      await api.removeRepo(r.id);
      repos = repos.filter((x) => x.id !== r.id);
    } catch (e) {
      toastError(e);
    }
  }
</script>

<p class="intro muted">{t('Dossiers que l’agent peut')} <strong>{t('lire')}</strong> {t('(jamais modifier) pendant les demandes en direct et la corrélation. Enregistré automatiquement.')}</p>

<div class="list">
  {#each repos as r (r.id)}
    <div class="repo card">
      <div class="row">
        <Icon name="folder" size={16} />
        <input class="input label-in" bind:value={r.label} onchange={() => update(r)} aria-label={t('Libellé du dépôt')} />
        <span class="path faint small ellipsis grow" title={r.path}>{r.path}</span>
        <button class="btn ghost sm icon" onclick={() => remove(r)} aria-label={t('Retirer {label}', { label: r.label })} title={t('Retirer')}><Icon name="trash" size={13} /></button>
      </div>
      <div class="row opts">
        <label class="check small"><input class="switch" type="checkbox" role="switch" bind:checked={r.use_live} onchange={() => update(r)} /> {t('En direct')}</label>
        <label class="check small"><input class="switch" type="checkbox" role="switch" bind:checked={r.use_correlation} onchange={() => update(r)} /> {t('Corrélation')}</label>
      </div>
      <label class="globs">
        <span class="small muted">{t('Exclusions (un motif glob par ligne)')}</span>
        <textarea class="input mono" rows="2" bind:value={r.exclude_globs} onchange={() => update(r)} placeholder={'**/node_modules/**\n**/*.min.js'}></textarea>
      </label>
    </div>
  {:else}
    {#if loaded}<div class="empty"><Icon name="folder" size={28} /><p>{t('Aucun dépôt lisible.')}</p></div>{/if}
  {/each}
</div>
<button class="btn" onclick={add}><Icon name="plus" size={14} />{t('Ajouter un dossier…')}</button>

<style>
  .intro { margin: 20px 0 16px; line-height: 1.5; }
  .list { display: flex; flex-direction: column; gap: 12px; margin-bottom: 14px; }
  .repo { padding: 14px 16px; display: flex; flex-direction: column; gap: 10px; }
  .repo > .row > :global(.icon:first-child) { color: var(--accent-text); }
  .label-in { width: 180px; font-weight: 600; height: 28px; }
  .path { font-family: var(--mono); font-size: 11.5px; }
  .opts { gap: 22px; padding-left: 26px; color: var(--text-2); }
  .globs { display: flex; flex-direction: column; gap: 6px; padding-left: 26px; }
  .globs textarea { font-size: 12px; }
</style>
