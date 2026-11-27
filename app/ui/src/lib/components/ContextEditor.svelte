<script lang="ts">
  // Contexte d'une réunion pour le compte rendu : texte libre, format de sortie et documents
  // joints. Utilisé à l'arrêt de l'écoute, pendant la réunion et sur la page de la réunion.
  // Le texte est enregistré à la sortie du champ (et par `flush()` avant un arrêt).
  import { onMount } from 'svelte';
  import { api } from '../api';
  import type { ContextFile, MeetingContext, ReportTemplate, Settings } from '../types';
  import { toastError } from '../ui.svelte';
  import Icon from './Icon.svelte';
  import { t, tk } from '../i18n.svelte';

  let { meetingId, compact = false, onchange }: { meetingId: string; compact?: boolean; onchange?: (c: MeetingContext) => void } = $props();

  const ACCEPT = '.pdf,.xlsx,.xlsm,.xls,.ods,.csv,.tsv,.docx,.doc,.rtf,.odt,.html,.htm,.md,.markdown,.txt,.json,.yaml,.yml,.xml,.log';
  const KIND_LABEL: Record<string, string> = { pdf: 'PDF', tableur: tk('Tableur'), csv: 'CSV', document: tk('Document'), texte: tk('Texte') };

  let ctx = $state<MeetingContext | null>(null);
  let templates = $state<ReportTemplate[]>([]);
  let defaultTemplate = $state('standard');
  let text = $state('');
  let uploading = $state(0);
  let input: HTMLInputElement | undefined = $state();
  let creating = $state(false);
  let newName = $state('');
  let newInstr = $state('');

  async function load() {
    try {
      const [c, tl, s] = await Promise.all([api.getMeetingContext(meetingId), api.listReportTemplates(), api.getSettings()]);
      ctx = c;
      text = c.text;
      templates = tl;
      defaultTemplate = s.report?.default_template ?? 'standard';
    } catch (e) {
      toastError(e);
    }
  }
  onMount(load);

  const defaultName = $derived(templates.find((x) => x.id === defaultTemplate)?.name ?? t('Standard'));
  const current = $derived(templates.find((x) => x.id === (ctx?.template ?? defaultTemplate)));

  async function save(template = ctx?.template ?? null) {
    if (!ctx) return;
    if (text.trim() === ctx.text && template === ctx.template) return;
    try {
      ctx = await api.setMeetingContext(meetingId, text, template);
      onchange?.(ctx);
    } catch (e) {
      toastError(e);
    }
  }

  /** Enregistre le texte en cours de saisie (avant un arrêt d'écoute, par exemple). */
  export async function flush(): Promise<void> {
    await save();
  }

  function pickTemplate(v: string) {
    if (v === '__new') {
      creating = true;
      return;
    }
    save(v === '' ? null : v);
  }

  async function createTemplate() {
    const name = newName.trim();
    if (!name || !newInstr.trim()) return;
    try {
      const s: Settings = await api.getSettings();
      const saved = await api.saveSettings({ ...s, report: { ...s.report, templates: [...s.report.templates, { id: '', name, instructions: newInstr.trim() }] } });
      const tpl = saved.report.templates.at(-1);
      templates = await api.listReportTemplates();
      creating = false;
      newName = '';
      newInstr = '';
      if (tpl) await save(tpl.id);
    } catch (e) {
      toastError(e);
    }
  }

  async function addFiles(list: FileList | null) {
    if (!list?.length || !ctx) return;
    for (const f of Array.from(list)) {
      uploading++;
      try {
        const added: ContextFile = await api.addContextFile(meetingId, f);
        ctx.files.push(added);
        onchange?.(ctx);
      } catch (e) {
        toastError(e);
      } finally {
        uploading--;
      }
    }
    if (input) input.value = '';
  }

  async function removeFile(id: number) {
    if (!ctx) return;
    try {
      await api.deleteContextFile(id);
      ctx.files = ctx.files.filter((f) => f.id !== id);
      onchange?.(ctx);
    } catch (e) {
      toastError(e);
    }
  }

  const size = (n: number) =>
    n < 1024 ? t('{n} o', { n }) : n < 1048576 ? t('{n} Ko', { n: Math.round(n / 1024) }) : t('{n} Mo', { n: (n / 1048576).toFixed(1) });
</script>

<div class="ctx" class:compact>
  <label class="lbl" for="ctx-text-{meetingId}">{t('Contexte')} <span class="faint">{t('facultatif')}</span></label>
  <textarea
    id="ctx-text-{meetingId}"
    class="input"
    rows={compact ? 3 : 4}
    bind:value={text}
    onblur={() => save()}
    placeholder={t('De quoi parle la réunion, quel projet, ce qu’il faut surveiller… Ex. « Point Nova : vérifier la TVA belge (19 % dans la règle actuelle) ».')}
    disabled={!ctx}
  ></textarea>

  <div class="row docs-h">
    <span class="lbl">{t('Documents')}</span>
    <span class="faint small">{t('PDF, Excel, CSV, Word, Markdown ou texte : lus par l’IA pour le compte rendu.')}</span>
  </div>
  {#if ctx?.files.length}
    <ul class="files">
      {#each ctx.files as f (f.id)}
        <li>
          <Icon name="file" size={14} />
          <span class="grow ellipsis" title={f.name}>{f.name}</span>
          <span class="badge outline">{KIND_LABEL[f.kind] ? t(KIND_LABEL[f.kind]) : f.kind}</span>
          <span class="faint small tnum">{size(f.size)}</span>
          {#if f.truncated}<span class="badge warn" title={t('Document long : seul le début est lu par l’IA')}>{t('extrait')}</span>{/if}
          <button class="btn ghost sm icon" onclick={() => removeFile(f.id)} title={t('Retirer ce document')} aria-label={t('Retirer {name}', { name: f.name })}><Icon name="x" size={13} /></button>
        </li>
      {/each}
    </ul>
  {/if}
  <div class="row">
    <button class="btn sm" onclick={() => input?.click()} disabled={!ctx || uploading > 0}>
      {#if uploading}<span class="spin" aria-hidden="true"></span>{t('Lecture…')}{:else}<Icon name="paperclip" size={14} />{t('Ajouter des documents')}{/if}
    </button>
    <input bind:this={input} type="file" multiple accept={ACCEPT} hidden onchange={(e) => addFiles(e.currentTarget.files)} />
  </div>

  <label class="lbl" for="ctx-tpl-{meetingId}">{t('Format du compte rendu')}</label>
  <select id="ctx-tpl-{meetingId}" class="input tpl" value={ctx?.template ?? ''} onchange={(e) => pickTemplate(e.currentTarget.value)} disabled={!ctx}>
    <option value="">{t('Par défaut ({name})', { name: defaultName })}</option>
    <optgroup label={t('Formats prédéfinis')}>
      {#each templates.filter((x) => x.builtin) as tpl (tpl.id)}<option value={tpl.id}>{tpl.name}</option>{/each}
    </optgroup>
    {#if templates.some((x) => !x.builtin)}
      <optgroup label={t('Mes formats')}>
        {#each templates.filter((x) => !x.builtin) as tpl (tpl.id)}<option value={tpl.id}>{tpl.name}</option>{/each}
      </optgroup>
    {/if}
    <option value="__new">＋ {t('Nouveau format…')}</option>
  </select>
  {#if current && !creating}<p class="faint small">{current.description}</p>{/if}

  {#if creating}
    <div class="newtpl">
      <input class="input" bind:value={newName} placeholder={t('Nom du format (ex. Note COMEX)')} aria-label={t('Nom du format')} />
      <textarea class="input" rows="3" bind:value={newInstr} placeholder={t('Consignes : structure, ton, longueur… Ex. « Trois puces au plus, ton exécutif, chiffres en gras ».')} aria-label={t('Consignes du format')}></textarea>
      <p class="faint small">{t('Enregistré sur ce Mac et réutilisable pour les prochaines réunions (Réglages → Comptes rendus).')}</p>
      <div class="row end">
        <button class="btn sm ghost" onclick={() => (creating = false)}>{t('Annuler')}</button>
        <button class="btn sm primary" onclick={createTemplate} disabled={!newName.trim() || !newInstr.trim()}>{t('Enregistrer et utiliser')}</button>
      </div>
    </div>
  {/if}
</div>

<style>
  .ctx { display: flex; flex-direction: column; gap: 8px; }
  .lbl { font-weight: 600; font-size: var(--fs-ui); margin-top: 6px; }
  .ctx > .lbl:first-child { margin-top: 0; }
  textarea { resize: vertical; min-height: 64px; line-height: 1.5; font: inherit; }
  .docs-h { gap: 8px; align-items: baseline; flex-wrap: wrap; margin-top: 6px; }
  .docs-h .lbl { margin-top: 0; }
  .files { list-style: none; margin: 0; padding: 0; border: 1px solid var(--line); border-radius: var(--radius-lg, 10px); background: var(--surface); }
  .files li { display: flex; align-items: center; gap: 8px; padding: 6px 8px 6px 10px; border-top: 1px solid var(--line); color: var(--text-2); }
  .files li:first-child { border-top: 0; }
  .files li .grow { color: var(--text); }
  .tpl { max-width: 360px; }
  .newtpl { display: flex; flex-direction: column; gap: 8px; padding: 10px; border-radius: 10px; background: var(--surface-2); }
  .end { justify-content: flex-end; gap: 6px; }
  .spin { width: 12px; height: 12px; border: 1.5px solid currentColor; border-right-color: transparent; border-radius: 50%; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
