<script lang="ts">
  import { t } from '../i18n.svelte';
  // Liste de mots éditable (Entrée ou virgule pour ajouter, ⌫ pour retirer le dernier).
  let {
    values = $bindable([]),
    placeholder,
    label,
    onadd,
  }: { values: string[]; placeholder?: string; label: string; onadd?: (v: string) => boolean | Promise<boolean> } = $props();
  let draft = $state('');

  async function add() {
    const v = draft.trim().replace(/,$/, '').trim();
    if (!v || values.includes(v)) {
      draft = '';
      return;
    }
    if (onadd && !(await onadd(v))) return;
    values = [...values, v];
    draft = '';
  }
  function keydown(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ',') {
      e.preventDefault();
      add();
    } else if (e.key === 'Backspace' && !draft && values.length) {
      values = values.slice(0, -1);
    }
  }
</script>

<div class="tags" role="group" aria-label={label}>
  {#each values as v, i (v)}
    <span class="tag">
      {v}
      <button type="button" aria-label={t('Retirer {v}', { v })} onclick={() => (values = values.filter((_, j) => j !== i))}>×</button>
    </span>
  {/each}
  <input bind:value={draft} onkeydown={keydown} onblur={add} placeholder={placeholder ?? t('Ajouter…')} aria-label={t('{label} : ajouter', { label })} />
</div>

<style>
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 4px;
    min-height: 30px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow-xs);
    transition: border-color var(--t-fast) var(--ease), box-shadow var(--t-fast) var(--ease);
  }
  .tags:focus-within { border-color: var(--focus); box-shadow: var(--ring); }
  .tag {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    height: 22px;
    padding: 0 2px 0 8px;
    border-radius: 6px;
    background: var(--surface-2);
    box-shadow: inset 0 0 0 1px var(--line);
    font-size: 12px;
    font-weight: 500;
    animation: appear 120ms var(--ease);
  }
  .tag button {
    border: 0;
    background: none;
    color: var(--text-3);
    font-size: 14px;
    line-height: 1;
    width: 18px;
    height: 18px;
    border-radius: 5px;
    padding: 0;
  }
  .tag button:hover { color: var(--text); background: var(--hover); }
  input {
    flex: 1;
    min-width: 90px;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--text);
    font: inherit;
    padding: 0 5px;
  }
  input::placeholder { color: var(--text-3); }
</style>
