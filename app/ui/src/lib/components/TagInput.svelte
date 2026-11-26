<script lang="ts">
  // Liste de mots éditable (Entrée ou virgule pour ajouter, ⌫ pour retirer le dernier).
  let {
    values = $bindable([]),
    placeholder = 'Ajouter…',
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
      <button type="button" aria-label="Retirer {v}" onclick={() => (values = values.filter((_, j) => j !== i))}>×</button>
    </span>
  {/each}
  <input bind:value={draft} onkeydown={keydown} onblur={add} {placeholder} aria-label="{label} : ajouter" />
</div>

<style>
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 3px;
    min-height: 28px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    background: var(--surface);
  }
  .tags:focus-within { border-color: var(--focus); box-shadow: 0 0 0 3px color-mix(in srgb, var(--focus) 22%, transparent); }
  .tag {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    height: 20px;
    padding: 0 2px 0 7px;
    border-radius: 5px;
    background: var(--surface-2);
    font-size: 12px;
  }
  .tag button {
    border: 0;
    background: none;
    color: var(--text-3);
    font-size: 14px;
    line-height: 1;
    width: 16px;
    height: 16px;
    border-radius: 4px;
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
