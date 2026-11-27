<script lang="ts">
  import { api } from '../../lib/api';
  import type { MeetingDetail, Note } from '../../lib/types';
  import { time } from '../../lib/format';
  import { confirmDialog, toastError } from '../../lib/ui.svelte';
  import Icon from '../../lib/components/Icon.svelte';
  import { t } from '../../lib/i18n.svelte';

  let { detail }: { detail: MeetingDetail } = $props();
  const m = $derived(detail.meeting);
  let draft = $state('');
  let saved = $state<Record<number, string>>({});

  async function add(e: SubmitEvent) {
    e.preventDefault();
    const text = draft.trim();
    if (!text) return;
    try {
      const n = await api.addNote(m.id, text);
      detail.notes.push(n);
      draft = '';
    } catch (err) {
      toastError(err);
    }
  }

  async function save(n: Note) {
    if (saved[n.id] === n.text) return;
    try {
      await api.updateNote(n.id, n.text);
      saved[n.id] = n.text;
    } catch (err) {
      toastError(err);
    }
  }

  async function remove(n: Note) {
    if (!(await confirmDialog({ title: t('Supprimer cette note ?'), message: n.text.slice(0, 140), confirm: t('Supprimer'), danger: true }))) return;
    try {
      await api.deleteNote(n.id);
      detail.notes = detail.notes.filter((x) => x.id !== n.id);
    } catch (err) {
      toastError(err);
    }
  }

  function grow(el: HTMLTextAreaElement) {
    const fit = () => {
      el.style.height = 'auto';
      el.style.height = el.scrollHeight + 'px';
    };
    fit();
    el.addEventListener('input', fit);
    return { destroy: () => el.removeEventListener('input', fit) };
  }

  $effect(() => {
    for (const n of detail.notes) if (!(n.id in saved)) saved[n.id] = n.text;
  });
</script>

<div class="wrap">
  <p class="faint small intro"><Icon name="info" size={13} />{t('Vos notes sont fusionnées dans le compte rendu (section « Mes notes »). Les modifications sont enregistrées en quittant le champ.')}</p>
  <ul class="notes">
    {#each detail.notes as n (n.id)}
      <li>
        <div class="meta">
          <span>{time(n.ts)}</span>
          {#if n.source === 'voice'}<span class="src" title={t('Dictée à Sténo')}><Icon name="mic" size={11} /></span>{/if}
        </div>
        <textarea class="note" rows="1" bind:value={n.text} onblur={() => save(n)} use:grow aria-label={t('Note de {time}', { time: time(n.ts) })}></textarea>
        <button class="btn ghost sm icon del" onclick={() => remove(n)} aria-label={t('Supprimer la note')} title={t('Supprimer')}><Icon name="trash" size={13} /></button>
      </li>
    {/each}
  </ul>
  <form class="add" onsubmit={add}>
    <textarea
      class="input"
      rows="2"
      bind:value={draft}
      placeholder={t('Ajouter une note…  (⌘↩ pour enregistrer)')}
      aria-label={t('Nouvelle note')}
      onkeydown={(e) => { if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); e.currentTarget.form?.requestSubmit(); } }}
    ></textarea>
    <div class="row"><span class="grow"></span><button class="btn sm primary" disabled={!draft.trim()}>{t('Ajouter')}</button></div>
  </form>
</div>

<style>
  .wrap { padding: 18px 32px 48px; max-width: calc(72ch + 64px); }
  .intro { margin-bottom: 14px; display: flex; gap: 6px; align-items: flex-start; line-height: 1.5; }
  .intro :global(.icon) { margin-top: 2px; flex: none; }
  .notes { list-style: none; margin: 0 0 16px; padding: 4px 0; }
  .notes li { display: grid; grid-template-columns: 64px 1fr 30px; gap: 8px; align-items: start; padding: 4px 0; border-bottom: 1px solid var(--line); }
  .notes li:last-child { border-bottom: 0; }
  .meta { display: flex; gap: 5px; align-items: center; color: var(--text-3); font-size: var(--fs-meta); padding-top: 9px; font-variant-numeric: tabular-nums; }
  .src { display: inline-grid; place-items: center; width: 18px; height: 18px; border-radius: 50%; background: var(--accent-soft); color: var(--accent-text); }
  .note {
    width: 100%;
    border: 1px solid transparent;
    border-radius: 8px;
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: var(--fs-body);
    line-height: 1.55;
    padding: 6px 8px;
    resize: none;
    overflow: hidden;
    transition: background var(--t-fast) var(--ease), border-color var(--t-fast) var(--ease), box-shadow var(--t-fast) var(--ease);
  }
  .note:hover { background: var(--hover); }
  .note:focus { outline: none; border-color: var(--focus); background: var(--surface); box-shadow: var(--ring); }
  .del { opacity: 0; margin-top: 4px; transition: opacity var(--t-fast) var(--ease); }
  li:hover .del, .del:focus-visible { opacity: 1; }
  .add { display: flex; flex-direction: column; gap: 8px; padding: 12px; border-radius: 12px; background: var(--surface-2); border: 1px solid var(--line); }
  .add textarea { font-size: var(--fs-body); }
</style>
