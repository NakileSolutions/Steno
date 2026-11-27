<script lang="ts" module>
  export type AgentOption = {
    /** Valeur enregistrée ('' = agent par défaut). */
    value: string;
    label: string;
    /** Identifiant de l'agent dont on affiche le logo. */
    logo: string;
    /** Mention discrète (« non installé »). */
    tag?: string;
  };
  let seq = 0;
</script>

<script lang="ts">
  // Liste déroulante d'agents avec leur logo (un <select> natif ne sait pas afficher d'image).
  // Bouton + liste d'options au clavier : flèches, Début/Fin, Entrée, Échap.
  import { tick } from 'svelte';
  import AgentLogo from './AgentLogo.svelte';
  import Icon from './Icon.svelte';

  let { value, options, label, onchange }: { value: string; options: AgentOption[]; label: string; onchange: (v: string) => void } =
    $props();

  const uid = `agp${++seq}`;
  let open = $state(false);
  let active = $state(0);
  let root: HTMLDivElement | undefined = $state();
  let btn: HTMLButtonElement | undefined = $state();
  let list: HTMLDivElement | undefined = $state();

  const current = $derived(options.find((o) => o.value === value) ?? options[0]);

  async function focusActive() {
    await tick();
    list?.querySelectorAll<HTMLButtonElement>('[role="option"]')[active]?.focus();
  }
  function show() {
    open = true;
    active = Math.max(0, options.findIndex((o) => o.value === value));
    focusActive();
  }
  function close(refocus = true) {
    open = false;
    if (refocus) btn?.focus();
  }
  function pick(o: AgentOption) {
    close();
    if (o.value !== value) onchange(o.value);
  }
  function onButtonKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      show();
    }
  }
  function onListKey(e: KeyboardEvent) {
    const n = options.length;
    if (e.key === 'ArrowDown') active = (active + 1) % n;
    else if (e.key === 'ArrowUp') active = (active - 1 + n) % n;
    else if (e.key === 'Home') active = 0;
    else if (e.key === 'End') active = n - 1;
    else if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      return close();
    } else if (e.key === 'Tab') return close(false);
    else return;
    e.preventDefault();
    focusActive();
  }
  function onWindowDown(e: PointerEvent) {
    if (open && root && !root.contains(e.target as Node)) close(false);
  }
</script>

<svelte:window onpointerdown={onWindowDown} />

<div class="picker" bind:this={root}>
  <button
    bind:this={btn}
    type="button"
    class="input pbtn"
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-controls="{uid}-list"
    aria-label="{label} : {current?.label ?? ''}"
    onclick={() => (open ? close() : show())}
    onkeydown={onButtonKey}
  >
    {#if current}<AgentLogo id={current.logo} name={current.label} size={16} />{/if}
    <span class="plabel ellipsis">{current?.label ?? ''}</span>
    {#if current?.tag}<span class="ptag">{current.tag}</span>{/if}
  </button>
  {#if open}
    <div class="plist" id="{uid}-list" role="listbox" aria-label={label} tabindex="-1" bind:this={list} onkeydown={onListKey}>
      {#each options as o, i (o.value)}
        <button
          type="button"
          role="option"
          class="popt"
          class:active={i === active}
          aria-selected={o.value === value}
          tabindex={i === active ? 0 : -1}
          onclick={() => pick(o)}
          onpointermove={() => (active = i)}
        >
          <AgentLogo id={o.logo} name={o.label} size={18} />
          <span class="grow ellipsis">{o.label}</span>
          {#if o.tag}<span class="ptag">{o.tag}</span>{/if}
          <span class="pcheck" aria-hidden="true">{#if o.value === value}<Icon name="check" size={13} stroke={2} />{/if}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .picker { position: relative; min-width: 0; }
  .pbtn {
    display: flex;
    align-items: center;
    gap: 8px;
    text-align: left;
    cursor: default;
    padding-right: 28px;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='%23878c99' stroke-width='2.2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M7 9.5l5-5 5 5M7 14.5l5 5 5-5'/%3E%3C/svg%3E");
    background-repeat: no-repeat;
    background-position: right 9px center;
  }
  .plabel { flex: 1; min-width: 0; }
  .ptag { flex: none; font-size: var(--fs-micro); color: var(--warn); font-weight: 500; }
  .plist {
    position: absolute;
    z-index: 30;
    top: calc(100% + 4px);
    left: 0;
    min-width: 100%;
    width: max-content;
    max-width: 320px;
    padding: 4px;
    border-radius: 10px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    box-shadow: var(--shadow-md);
    display: flex;
    flex-direction: column;
    gap: 1px;
    animation: appear var(--t-fast) var(--ease);
  }
  .plist:focus { outline: none; }
  .popt {
    all: unset;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    gap: 9px;
    height: 32px;
    padding: 0 8px;
    border-radius: 7px;
    color: var(--text);
    cursor: default;
  }
  .popt.active { background: var(--accent-soft); }
  .popt:focus-visible { outline: 2px solid var(--focus); outline-offset: -2px; }
  .pcheck { width: 14px; display: grid; place-items: center; color: var(--accent-text); flex: none; }
</style>
