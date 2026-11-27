<script lang="ts">
  // Comptes rendus : format par défaut, contexte proposé à l'arrêt, formats perso.
  import { onMount } from 'svelte';
  import { api } from '../../lib/api';
  import type { ReportTemplate, Settings } from '../../lib/types';
  import Icon from '../../lib/components/Icon.svelte';

  let { settings = $bindable() }: { settings: Settings } = $props();
  let builtins = $state<ReportTemplate[]>([]);
  onMount(async () => {
    try {
      builtins = (await api.listReportTemplates()).filter((t) => t.builtin);
    } catch {
      builtins = [];
    }
  });

  function add() {
    settings.report.templates.push({ id: '', name: '', instructions: '' });
  }
  function remove(i: number) {
    const [t] = settings.report.templates.splice(i, 1);
    if (t && settings.report.default_template === t.id) settings.report.default_template = 'standard';
  }
</script>

<p class="intro muted">Le format guide la rédaction : structure, ton, longueur. Il se choisit pour chaque réunion (à l’arrêt de l’écoute ou sur la page de la réunion) ; sinon le format par défaut s’applique.</p>

<div class="field">
  <label class="field-label" for="rep-default">Format par défaut</label>
  <div class="field-body">
    <select id="rep-default" class="input w" bind:value={settings.report.default_template}>
      <optgroup label="Formats prédéfinis">
        {#each builtins as t (t.id)}<option value={t.id}>{t.name}</option>{/each}
      </optgroup>
      {#if settings.report.templates.some((t) => t.id)}
        <optgroup label="Mes formats">
          {#each settings.report.templates.filter((t) => t.id) as t (t.id)}<option value={t.id}>{t.name}</option>{/each}
        </optgroup>
      {/if}
    </select>
    {#if builtins.find((t) => t.id === settings.report.default_template)}
      <p class="faint small">{builtins.find((t) => t.id === settings.report.default_template)?.description}</p>
    {/if}
  </div>
</div>

<div class="field">
  <div class="field-label">À l’arrêt de l’écoute<span class="hint">Désactivé : l’arrêt se fait en un clic et le compte rendu part tout de suite. Le contexte peut s’ajouter pendant la réunion (bouton « Contexte ») ou sur la page de la réunion.</span></div>
  <div class="field-body">
    <label class="check toggle"><span>Proposer contexte, documents et format à l’arrêt</span><input class="switch" type="checkbox" role="switch" bind:checked={settings.report.ask_context_on_stop} /></label>
  </div>
</div>

<div class="field">
  <div class="field-label">Mes formats<span class="hint">Enregistrés sur ce Mac, réutilisables pour toutes les réunions.</span></div>
  <div class="field-body">
    {#each settings.report.templates as t, i (i)}
      <div class="tpl">
        <div class="row">
          <input class="input grow" bind:value={t.name} placeholder="Nom (ex. Note COMEX)" aria-label="Nom du format" />
          <button class="btn ghost sm icon" onclick={() => remove(i)} title="Supprimer ce format" aria-label="Supprimer le format {t.name}"><Icon name="trash" size={14} /></button>
        </div>
        <textarea class="input" rows="3" bind:value={t.instructions} placeholder="Consignes : structure, sections, ton, longueur… Ex. « Trois puces au plus, ton exécutif, chiffres en gras »." aria-label="Consignes du format {t.name}"></textarea>
      </div>
    {/each}
    <button class="btn sm" onclick={add}><Icon name="plus" size={14} />Nouveau format</button>
    <p class="faint small">Prédéfinis : {builtins.map((t) => t.name).join(', ')}.</p>
  </div>
</div>

<style>
  .intro { margin: 20px 0 0; line-height: 1.5; }
  .w { max-width: 320px; }
  .tpl { display: flex; flex-direction: column; gap: 6px; padding: 10px; margin-bottom: 8px; border: 1px solid var(--line); border-radius: 10px; background: var(--surface); }
  .tpl textarea { resize: vertical; font: inherit; line-height: 1.5; }
  .row { gap: 6px; align-items: center; }
</style>
