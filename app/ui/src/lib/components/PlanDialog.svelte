<script lang="ts">
  // Prévoir une réunion : titre, date et heure.
  import { api } from '../api';
  import { navigate } from '../router.svelte';
  import { toastError } from '../ui.svelte';
  import { t } from '../i18n.svelte';

  let { open = $bindable(false) }: { open?: boolean } = $props();
  let el: HTMLDialogElement | undefined = $state();
  let title = $state('');
  let date = $state('');
  let hour = $state('');
  let busy = $state(false);

  const pad = (n: number) => String(n).padStart(2, '0');
  $effect(() => {
    if (open && el && !el.open) {
      const d = new Date(Date.now() + 60 * 60 * 1000);
      date = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
      hour = `${pad(d.getHours())}:00`;
      title = '';
      el.showModal();
    } else if (!open && el?.open) el.close();
  });

  async function submit() {
    if (!title.trim() || !date || !hour) return;
    busy = true;
    try {
      const at = new Date(`${date}T${hour}:00`);
      const m = await api.planMeeting(title.trim(), at.toISOString());
      open = false;
      navigate(`/meetings/${m.id}`);
    } catch (e) {
      toastError(e);
    } finally {
      busy = false;
    }
  }
</script>

<dialog bind:this={el} oncancel={(e) => { e.preventDefault(); open = false; }} aria-labelledby="plan-title">
  {#if open}
    <form onsubmit={(e) => { e.preventDefault(); submit(); }}>
      <h2 id="plan-title">{t('Prévoir une réunion')}</h2>
      <label class="col">
        <span class="small muted">{t('Titre')}</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input class="input" bind:value={title} autofocus aria-label={t('Titre de la réunion')} />
      </label>
      <div class="row two">
        <label class="col"><span class="small muted">{t('Date')}</span><input class="input" type="date" bind:value={date} aria-label={t('Date de la réunion')} /></label>
        <label class="col"><span class="small muted">{t('Heure')}</span><input class="input" type="time" bind:value={hour} aria-label={t('Heure de la réunion')} /></label>
      </div>
      <p class="faint small">{t('Quand vous démarrerez l’écoute à cette heure-là, Sténo reprendra cette réunion avec son contexte.')}</p>
      <div class="row actions">
        <button type="button" class="btn" onclick={() => (open = false)}>{t('Annuler')}</button>
        <button type="submit" class="btn primary" disabled={busy || !title.trim()}>{t('Prévoir')}</button>
      </div>
    </form>
  {/if}
</dialog>

<style>
  dialog { width: min(440px, calc(100vw - 32px)); padding: 22px 22px 18px; border: 1px solid var(--line); border-radius: 16px; background: var(--surface); color: var(--text); box-shadow: var(--shadow); }
  dialog::backdrop { background: rgba(15, 17, 23, 0.38); }
  form { display: flex; flex-direction: column; gap: 12px; }
  h2 { font-size: var(--fs-h3); }
  .col { display: flex; flex-direction: column; gap: 4px; flex: 1; }
  .two { gap: 10px; }
  .actions { justify-content: flex-end; gap: 8px; margin-top: 6px; }
</style>
