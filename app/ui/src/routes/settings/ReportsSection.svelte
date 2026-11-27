<script lang="ts">
  // Comptes rendus : format par défaut, contexte proposé à l'arrêt, formats perso.
  import { onMount } from 'svelte';
  import { api } from '../../lib/api';
  import type { ReportTemplate, Settings } from '../../lib/types';
  import Icon from '../../lib/components/Icon.svelte';
  import { t } from '../../lib/i18n.svelte';
  import { toastError } from '../../lib/ui.svelte';

  let { settings = $bindable() }: { settings: Settings } = $props();
  let builtins = $state<ReportTemplate[]>([]);
  onMount(async () => {
    try {
      builtins = (await api.listReportTemplates()).filter((tp) => tp.builtin);
    } catch {
      builtins = [];
    }
  });

  function add() {
    settings.report.templates.push({ id: '', name: '', instructions: '' });
  }
  // Format déduit d'un compte rendu collé : l'agent propose nom et consignes, à relire.
  let fromExample = $state(false);
  let example = $state('');
  let deducing = $state(false);
  async function deduce() {
    deducing = true;
    try {
      const d = await api.templateFromExample(example);
      settings.report.templates.push({ id: '', name: d.name, instructions: d.instructions });
      fromExample = false;
      example = '';
    } catch (e) {
      toastError(e);
    } finally {
      deducing = false;
    }
  }
  function remove(i: number) {
    const [tp] = settings.report.templates.splice(i, 1);
    if (tp && settings.report.default_template === tp.id) settings.report.default_template = 'standard';
  }
</script>

<p class="intro muted">{t('Le format guide la rédaction : structure, ton, longueur. Il se choisit pour chaque réunion (à l’arrêt de l’écoute ou sur la page de la réunion) ; sinon le format par défaut s’applique.')}</p>

<div class="field">
  <label class="field-label" for="rep-default">{t('Format par défaut')}</label>
  <div class="field-body">
    <select id="rep-default" class="input w" bind:value={settings.report.default_template}>
      <optgroup label={t('Formats prédéfinis')}>
        {#each builtins as tp (tp.id)}<option value={tp.id}>{tp.name}</option>{/each}
      </optgroup>
      {#if settings.report.templates.some((tp) => tp.id)}
        <optgroup label={t('Mes formats')}>
          {#each settings.report.templates.filter((tp) => tp.id) as tp (tp.id)}<option value={tp.id}>{tp.name}</option>{/each}
        </optgroup>
      {/if}
    </select>
    {#if builtins.find((tp) => tp.id === settings.report.default_template)}
      <p class="faint small">{builtins.find((tp) => tp.id === settings.report.default_template)?.description}</p>
    {/if}
  </div>
</div>

<div class="field">
  <label class="field-label" for="rep-delay">{t('Délai avant la rédaction')}<span class="hint">{t('Temps laissé après la réunion pour ajouter contexte et documents. « Rédiger maintenant » le court-circuite.')}</span></label>
  <div class="field-body">
    <select id="rep-delay" class="input w" bind:value={settings.report.context_delay_s}>
      {#each [[0, t('Aucun (rédaction immédiate)')], [30, t('30 secondes')], [60, t('1 minute')], [120, t('2 minutes')], [300, t('5 minutes')]] as [v, l] (v)}<option value={v}>{l}</option>{/each}
    </select>
  </div>
</div>

<div class="field">
  <div class="field-label">{t('À l’arrêt de l’écoute')}<span class="hint">{t('Désactivé : l’arrêt se fait en un clic et le compte rendu part tout de suite. Le contexte peut s’ajouter pendant la réunion (bouton « Contexte ») ou sur la page de la réunion.')}</span></div>
  <div class="field-body">
    <label class="check toggle"><span>{t('Proposer contexte, documents et format à l’arrêt')}</span><input class="switch" type="checkbox" role="switch" bind:checked={settings.report.ask_context_on_stop} /></label>
  </div>
</div>

<div class="field">
  <div class="field-label">{t('Mes formats')}<span class="hint">{t('Enregistrés sur cet ordinateur, réutilisables pour toutes les réunions.')}</span></div>
  <div class="field-body">
    {#each settings.report.templates as tp, i (i)}
      <div class="tpl">
        <div class="row">
          <input class="input grow" bind:value={tp.name} placeholder={t('Nom (ex. Note COMEX)')} aria-label={t('Nom du format')} />
          <button class="btn ghost sm icon" onclick={() => remove(i)} title={t('Supprimer ce format')} aria-label={t('Supprimer le format {name}', { name: tp.name })}><Icon name="trash" size={14} /></button>
        </div>
        <textarea class="input" rows="3" bind:value={tp.instructions} placeholder={t('Consignes : structure, sections, ton, longueur… Ex. « Trois puces au plus, ton exécutif, chiffres en gras ».')} aria-label={t('Consignes du format {name}', { name: tp.name })}></textarea>
      </div>
    {/each}
    {#if fromExample}
      <div class="tpl">
        <textarea class="input" rows="6" bind:value={example} placeholder={t('Collez ici un compte rendu dont vous aimez la forme : l’IA en déduit la structure, le ton et la longueur, sans reprendre son contenu.')} aria-label={t('Compte rendu d’exemple')}></textarea>
        <div class="row">
          <button class="btn sm primary" onclick={deduce} disabled={deducing || example.trim().length < 40}>{deducing ? t('Analyse…') : t('Proposer le format')}</button>
          <button class="btn sm ghost" onclick={() => (fromExample = false)} disabled={deducing}>{t('Annuler')}</button>
        </div>
      </div>
    {/if}
    <div class="row">
      <button class="btn sm" onclick={add}><Icon name="plus" size={14} />{t('Nouveau format')}</button>
      {#if !fromExample}<button class="btn sm" onclick={() => (fromExample = true)}><Icon name="agent" size={14} />{t('À partir d’un exemple')}</button>{/if}
    </div>
    <p class="faint small">{t('Une réunion sans format choisi reprend celui de la dernière réunion du même titre (ex. chaque « Daily équipe »), sinon le format par défaut.')}</p>
    <p class="faint small">{t('Prédéfinis : {list}.', { list: builtins.map((tp) => tp.name).join(', ') })}</p>
  </div>
</div>

<style>
  .intro { margin: 20px 0 0; line-height: 1.5; }
  .w { max-width: 320px; }
  .tpl { display: flex; flex-direction: column; gap: 6px; padding: 10px; margin-bottom: 8px; border: 1px solid var(--line); border-radius: 10px; background: var(--surface); }
  .tpl textarea { resize: vertical; font: inherit; line-height: 1.5; }
  .row { gap: 6px; align-items: center; }
</style>
