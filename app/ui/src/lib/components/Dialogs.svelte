<script lang="ts">
  import { dialogs } from '../ui.svelte';

  let el: HTMLDialogElement | undefined = $state();
  let value = $state('');
  let typed = $state('');
  const d = $derived(dialogs.current);

  $effect(() => {
    if (d && el && !el.open) {
      value = d.input?.value ?? '';
      typed = '';
      el.showModal();
    } else if (!d && el?.open) el.close();
  });

  function done(v: string | boolean | null) {
    const cur = dialogs.current;
    dialogs.current = null;
    cur?.resolve(v);
  }
  const canConfirm = $derived(!d?.typeToConfirm || typed.trim() === d.typeToConfirm);
</script>

<dialog bind:this={el} oncancel={(e) => { e.preventDefault(); done(null); }} aria-labelledby="dlg-title">
  {#if d}
    <form method="dialog" onsubmit={(e) => { e.preventDefault(); if (canConfirm) done(d.input ? value : true); }}>
      <h2 id="dlg-title">{d.title}</h2>
      {#if d.message}<p class="muted msg">{d.message}</p>{/if}
      {#if d.input}
        <!-- svelte-ignore a11y_autofocus -->
        <input class="input" bind:value placeholder={d.input.placeholder} aria-label={d.input.label ?? d.title} autofocus />
      {/if}
      {#if d.typeToConfirm}
        <label class="col small">
          <span>Tapez <strong>{d.typeToConfirm}</strong> pour confirmer :</span>
          <!-- svelte-ignore a11y_autofocus -->
          <input class="input" bind:value={typed} autocomplete="off" autofocus />
        </label>
      {/if}
      <div class="row actions">
        <button type="button" class="btn" onclick={() => done(null)}>Annuler</button>
        <button type="submit" class="btn {d.danger ? 'danger solid' : 'primary'}" disabled={!canConfirm}>{d.confirm}</button>
      </div>
    </form>
  {/if}
</dialog>

<style>
  dialog {
    width: min(420px, calc(100vw - 32px));
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
  form { display: flex; flex-direction: column; gap: 12px; }
  h2 { font-size: var(--fs-h3); }
  .msg { line-height: 1.55; }
  .actions { justify-content: flex-end; margin-top: 8px; }
</style>
