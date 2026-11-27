<script lang="ts">
  // Fenêtre de contexte : à l'arrêt de l'écoute (`stop`) ou pendant la réunion (`edit`).
  import { api } from '../api';
  import { toastError } from '../ui.svelte';
  import ContextEditor from './ContextEditor.svelte';
  import { t } from '../i18n.svelte';

  let {
    meetingId,
    mode,
    open = $bindable(false),
    onstop,
  }: { meetingId: string; mode: 'stop' | 'edit'; open?: boolean; onstop?: () => Promise<void> | void } = $props();

  let el: HTMLDialogElement | undefined = $state();
  let editor: ReturnType<typeof ContextEditor> | undefined = $state();
  let dontAsk = $state(false);
  let busy = $state(false);

  $effect(() => {
    if (open && el && !el.open) el.showModal();
    else if (!open && el?.open) el.close();
  });

  async function close() {
    await editor?.flush();
    open = false;
  }

  async function stop() {
    busy = true;
    try {
      await editor?.flush();
      if (dontAsk) {
        const s = await api.getSettings();
        await api.saveSettings({ ...s, report: { ...s.report, ask_context_on_stop: false } });
      }
      open = false;
      await onstop?.();
    } catch (e) {
      toastError(e);
    } finally {
      busy = false;
    }
  }
</script>

<dialog bind:this={el} oncancel={(e) => { e.preventDefault(); close(); }} aria-labelledby="ctxdlg-title">
  {#if open}
    <h2 id="ctxdlg-title">{mode === 'stop' ? t('Arrêter l’écoute') : t('Contexte du compte rendu')}</h2>
    <p class="muted small intro">
      {mode === 'stop'
        ? t('Rien n’est obligatoire : « Arrêter et rédiger » (ou Entrée) lance directement le compte rendu. Vous pouvez aussi préciser le contexte, joindre des documents ou choisir le format.')
        : t('Pris en compte à la rédaction du compte rendu, en fin de réunion.')}
    </p>
    <ContextEditor bind:this={editor} {meetingId} compact />
    <div class="row actions">
      {#if mode === 'stop'}
        <label class="check small dont"><input type="checkbox" bind:checked={dontAsk} /> {t('Ne plus proposer')}</label>
        <span class="grow"></span>
        <button class="btn" onclick={close} disabled={busy}>{t('Continuer l’écoute')}</button>
        <!-- svelte-ignore a11y_autofocus -->
        <button class="btn danger solid" onclick={stop} disabled={busy} autofocus><span class="sq" aria-hidden="true"></span>{t('Arrêter et rédiger')}</button>
      {:else}
        <span class="grow"></span>
        <button class="btn primary" onclick={close}>{t('Terminé')}</button>
      {/if}
    </div>
  {/if}
</dialog>

<style>
  dialog {
    width: min(560px, calc(100vw - 32px));
    max-height: calc(100vh - 48px);
    overflow: auto;
    padding: 22px 22px 18px;
    border: 1px solid var(--line);
    border-radius: 16px;
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
  }
  dialog[open] { animation: pop 180ms var(--ease); }
  dialog::backdrop { background: rgba(15, 17, 23, 0.38); }
  @keyframes pop { from { opacity: 0; transform: translateY(6px) scale(0.98); } }
  h2 { font-size: var(--fs-h3); margin-bottom: 6px; }
  .intro { line-height: 1.5; margin-bottom: 14px; }
  .actions { margin-top: 18px; gap: 8px; align-items: center; }
  .dont { color: var(--text-2); gap: 6px; }
  .sq { width: 9px; height: 9px; border-radius: 2px; background: currentColor; display: inline-block; margin-right: 6px; }
</style>
