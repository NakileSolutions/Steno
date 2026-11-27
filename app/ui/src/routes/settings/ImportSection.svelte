<script lang="ts">
  import { api } from '../../lib/api';
  import type { Settings } from '../../lib/types';
  import { confirmDialog, copyText, downloadText, toast, toastError } from '../../lib/ui.svelte';
  import Icon from '../../lib/components/Icon.svelte';

  let { onimported, dirty }: { onimported: (s: Settings) => void; dirty: boolean } = $props();
  let exported = $state('');
  let toImport = $state('');
  let fileInput: HTMLInputElement | undefined = $state();

  async function doExport() {
    try {
      exported = await api.exportSettings();
    } catch (e) {
      toastError(e);
    }
  }
  async function readFile(e: Event) {
    const f = (e.currentTarget as HTMLInputElement).files?.[0];
    if (f) toImport = await f.text();
  }
  async function doImport() {
    if (!toImport.trim()) return;
    if (!(await confirmDialog({
      title: 'Importer ces réglages ?',
      message: (dirty ? 'Vos modifications non enregistrées seront perdues. ' : '') + 'Les sections présentes dans le fichier remplacent vos réglages actuels (dépôts, nom et chemins non concernés).',
      confirm: 'Importer',
    }))) return;
    try {
      const s = await api.importSettings(toImport);
      onimported(s);
      toImport = '';
      toast('Réglages importés.');
    } catch (e) {
      toastError(e);
    }
  }
</script>

<p class="intro muted">Partagez une configuration de départ avec l’équipe (vocabulaire, verbes, exclusions). Le fichier TOML ne contient ni chemin ni donnée personnelle.</p>

<div class="field">
  <div class="field-label">Exporter</div>
  <div class="field-body">
    <div class="row">
      <button class="btn" onclick={doExport}><Icon name="download" size={14} />Générer le TOML</button>
      {#if exported}
        <button class="btn ghost" onclick={async () => (await copyText(exported)) && toast('TOML copié.')}><Icon name="copy" size={14} />Copier</button>
        <button class="btn ghost" onclick={() => downloadText('steno-reglages.toml', exported, 'application/toml')}>Télécharger</button>
      {/if}
    </div>
    {#if exported}<textarea class="input mono" rows="12" readonly value={exported} aria-label="Réglages exportés"></textarea>{/if}
  </div>
</div>
<div class="field">
  <div class="field-label">Importer</div>
  <div class="field-body">
    <div class="row">
      <button class="btn" onclick={() => fileInput?.click()}><Icon name="folder" size={14} />Choisir un fichier .toml…</button>
      <input bind:this={fileInput} type="file" accept=".toml,text/plain" hidden onchange={readFile} />
    </div>
    <textarea class="input mono" rows="8" bind:value={toImport} placeholder="…ou collez le contenu TOML ici" aria-label="TOML à importer"></textarea>
    <div><button class="btn primary" onclick={doImport} disabled={!toImport.trim()}>Importer</button></div>
  </div>
</div>

<style>
  .intro { margin: 20px 0 0; line-height: 1.5; }
  textarea { font-size: 12px; }
</style>
