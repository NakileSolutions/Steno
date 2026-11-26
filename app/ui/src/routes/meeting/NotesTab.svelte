<script lang="ts">
  import { api } from '../../lib/api';
  import type { MeetingDetail, Note } from '../../lib/types';
  import { time } from '../../lib/format';
  import { confirmDialog, toastError } from '../../lib/ui.svelte';
  import Icon from '../../lib/components/Icon.svelte';

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
    if (!(await confirmDialog({ title: 'Supprimer cette note ?', message: n.text.slice(0, 140), confirm: 'Supprimer', danger: true }))) return;
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
  <p class="faint small intro">Vos notes sont fusionnées dans le compte rendu (section « Mes notes »). Les modifications sont enregistrées en quittant le champ.</p>
  <ul class="notes">
    {#each detail.notes as n (n.id)}
      <li>
        <div class="meta">
          <span>{time(n.ts)}</span>
          {#if n.source === 'voice'}<span class="src" title="Dictée au Scribe"><Icon name="mic" size={11} /></span>{/if}
        </div>
        <textarea class="note" rows="1" bind:value={n.text} onblur={() => save(n)} use:grow aria-label="Note de {time(n.ts)}"></textarea>
        <button class="btn ghost sm icon del" onclick={() => remove(n)} aria-label="Supprimer la note" title="Supprimer"><Icon name="trash" size={13} /></button>
      </li>
    {/each}
  </ul>
  <form class="add" onsubmit={add}>
    <textarea
      class="input"
      rows="2"
      bind:value={draft}
      placeholder="Ajouter une note…  (⌘↩ pour enregistrer)"
      aria-label="Nouvelle note"
      onkeydown={(e) => { if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); e.currentTarget.form?.requestSubmit(); } }}
    ></textarea>
    <div class="row"><span class="grow"></span><button class="btn sm primary" disabled={!draft.trim()}>Ajouter</button></div>
  </form>
</div>

<style>
  .wrap { padding: 16px 28px 40px; max-width: 760px; }
  .intro { margin-bottom: 12px; }
  .notes { list-style: none; margin: 0 0 14px; padding: 0; }
  .notes li { display: grid; grid-template-columns: 56px 1fr 28px; gap: 8px; align-items: start; padding: 4px 0; border-bottom: 1px solid var(--line); }
  .meta { display: flex; gap: 4px; align-items: center; color: var(--text-3); font-size: 11.5px; padding-top: 7px; font-variant-numeric: tabular-nums; }
  .note {
    width: 100%;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: 13.5px;
    line-height: 1.5;
    padding: 4px 6px;
    resize: none;
    overflow: hidden;
  }
  .note:hover { background: var(--hover); }
  .note:focus { outline: none; border-color: var(--focus); background: var(--surface); }
  .del { opacity: 0; margin-top: 3px; }
  li:hover .del, .del:focus-visible { opacity: 1; }
  .add { display: flex; flex-direction: column; gap: 8px; }
</style>
