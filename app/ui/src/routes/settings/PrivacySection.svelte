<script lang="ts">
  import { api } from '../../lib/api';
  import type { Settings } from '../../lib/types';
  import { confirmDialog, toast, toastError } from '../../lib/ui.svelte';
  import { navigate } from '../../lib/router.svelte';
  import Icon from '../../lib/components/Icon.svelte';
  import TagInput from '../../lib/components/TagInput.svelte';
  import { DEFAULT_CONSENT_MESSAGE } from '../../lib/listening';

  let { settings = $bindable() }: { settings: Settings } = $props();

  let keepForever = $state(settings.privacy.transcript_retention_days === null);
  $effect(() => {
    if (keepForever) settings.privacy.transcript_retention_days = null;
    else if (settings.privacy.transcript_retention_days === null) settings.privacy.transcript_retention_days = 90;
  });

  async function pickExport() {
    try {
      const p = await api.pickFolder();
      if (p) settings.privacy.auto_export_dir = p;
    } catch (e) {
      toastError(e);
    }
  }

  async function wipe() {
    if (!(await confirmDialog({
      title: 'Tout supprimer ?',
      message: 'Toutes les réunions, transcriptions, comptes rendus, demandes, notes et dépôts enregistrés seront effacés de ce Mac. Cette action est irréversible.',
      confirm: 'Continuer',
      danger: true,
    }))) return;
    if (!(await confirmDialog({
      title: 'Confirmation finale',
      message: 'Dernière vérification avant l’effacement définitif.',
      confirm: 'Tout supprimer',
      danger: true,
      typeToConfirm: 'SUPPRIMER',
    }))) return;
    try {
      await api.deleteAllData();
      toast('Toutes les données ont été supprimées.');
      navigate('/meetings');
    } catch (e) {
      toastError(e);
    }
  }
</script>

<div class="field">
  <label class="field-label" for="audio-ret">Audio<span class="hint">L’audio n’est jamais en base ; tampon temporaire seulement.</span></label>
  <div class="field-body">
    <label class="row small">Conserver l’audio <input id="audio-ret" class="input num" type="number" min="0" max="30" bind:value={settings.privacy.audio_retention_days} /> jours <span class="faint">(0 = supprimé après traitement)</span></label>
  </div>
</div>
<div class="field">
  <div class="field-label">Transcriptions<span class="hint">Les comptes rendus sont conservés.</span></div>
  <div class="field-body">
    <label class="check toggle"><span>Conserver indéfiniment</span><input class="switch" type="checkbox" role="switch" bind:checked={keepForever} /></label>
    {#if !keepForever && settings.privacy.transcript_retention_days !== null}
      <label class="row small">Supprimer après <input class="input num" type="number" min="1" bind:value={settings.privacy.transcript_retention_days} /> jours</label>
    {/if}
  </div>
</div>
<div class="field">
  <div class="field-label">Export automatique<span class="hint">Un fichier markdown par réunion terminée.</span></div>
  <div class="field-body">
    <div class="row">
      <span class="path mono small grow ellipsis">{settings.privacy.auto_export_dir ?? 'Désactivé'}</span>
      <button class="btn sm" onclick={pickExport}><Icon name="folder" size={13} />Choisir…</button>
      {#if settings.privacy.auto_export_dir}<button class="btn sm ghost" onclick={() => (settings.privacy.auto_export_dir = null)}>Désactiver</button>{/if}
    </div>
  </div>
</div>
<div class="field">
  <label class="field-label" for="consent-msg">Prévenir les participants<span class="hint">Le RGPD impose d’informer les personnes transcrites. Le message se copie depuis la vue En direct, pour le coller dans le chat de l’appel.</span></label>
  <div class="field-body">
    <label class="check toggle"><span>Le rappeler au début de chaque écoute</span><input class="switch" type="checkbox" role="switch" bind:checked={settings.privacy.consent_reminder} /></label>
    <textarea id="consent-msg" class="input" rows="4" bind:value={settings.privacy.consent_message}></textarea>
    <button class="btn sm ghost" type="button" onclick={() => (settings.privacy.consent_message = DEFAULT_CONSENT_MESSAGE)}>Revenir au message par défaut</button>
  </div>
</div>
<div class="field">
  <div class="field-label">Réunions exclues<span class="hint">Jamais enregistrées si le titre contient l’un de ces mots.</span></div>
  <div class="field-body"><TagInput bind:values={settings.privacy.excluded_titles} label="Titres exclus" placeholder="Ex. RH, médical…" /></div>
</div>
<div class="field danger-zone">
  <div class="field-label">Tout supprimer</div>
  <div class="field-body">
    <p class="muted small">Efface toutes les données du Scribe sur ce Mac. Les réglages sont conservés.</p>
    <div><button class="btn danger" onclick={wipe}><Icon name="trash" size={14} />Tout supprimer…</button></div>
  </div>
</div>

<style>
  .num { width: 70px; height: 24px; }
  .row.small { gap: 6px; }
  .path { height: 28px; display: flex; align-items: center; padding: 0 10px; background: var(--surface-2); border-radius: 7px; color: var(--text-2); }
  .danger-zone { margin-top: 28px; border-color: color-mix(in srgb, var(--err) 30%, transparent) !important; }
  .danger-zone > .field-label { color: var(--err); }
</style>
