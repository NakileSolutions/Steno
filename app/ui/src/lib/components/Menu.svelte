<script lang="ts">
  // Menu déroulant simple (bouton + liste d'actions), fermé au clic extérieur ou sur Échap.
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';

  export type MenuItem = { label: string; icon?: string; hint?: string; action: () => void | Promise<void>; separatorBefore?: boolean };

  let { label, icon, items, title, children }: { label?: string; icon?: string; items: MenuItem[]; title?: string; children?: Snippet } = $props();

  let open = $state(false);
  let root: HTMLElement | undefined = $state();

  function onwindowclick(e: MouseEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
  function onkeydown(e: KeyboardEvent) {
    if (open && e.key === 'Escape') {
      e.stopPropagation();
      open = false;
    }
  }
  async function run(it: MenuItem) {
    open = false;
    await it.action();
  }
</script>

<svelte:window onclick={onwindowclick} />

<div class="menu" bind:this={root} {onkeydown} role="presentation">
  <button class="btn sm ghost" class:icon={!label} onclick={() => (open = !open)} aria-haspopup="menu" aria-expanded={open} {title} aria-label={title ?? label}>
    {#if icon}<Icon name={icon} size={15} />{/if}{#if label}{label}<Icon name="chevron" size={12} stroke={2} />{/if}{@render children?.()}
  </button>
  {#if open}
    <ul class="pop" role="menu">
      {#each items as it (it.label)}
        {#if it.separatorBefore}<li class="sep" role="separator"></li>{/if}
        <li role="none">
          <button role="menuitem" onclick={() => run(it)}>
            {#if it.icon}<Icon name={it.icon} size={14} />{/if}
            <span class="grow">{it.label}</span>
            {#if it.hint}<span class="hint">{it.hint}</span>{/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .menu { position: relative; display: inline-block; }
  .pop {
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    z-index: 20;
    min-width: 250px;
    list-style: none;
    margin: 0;
    padding: 4px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--surface);
    box-shadow: var(--shadow-md, 0 8px 24px rgba(0, 0, 0, 0.16));
    animation: appear 120ms var(--ease);
  }
  .pop button {
    all: unset;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    padding: 7px 10px;
    border-radius: 7px;
    font-size: var(--fs-ui);
    color: var(--text);
    cursor: pointer;
  }
  .pop button:hover, .pop button:focus-visible { background: var(--accent-soft); }
  .hint { color: var(--text-3); font-size: var(--fs-micro); }
  .sep { height: 1px; margin: 4px 6px; background: var(--line); }
</style>
