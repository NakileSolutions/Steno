<script lang="ts">
  import { onMount } from 'svelte';
  import { router, href, navigate } from '../lib/router.svelte';
  import { live, clock, STATE_LABEL } from '../lib/live.svelte';
  import { recent, initRecent } from '../lib/meetings.svelte';
  import { api, isTauri } from '../lib/api';
  import { MAIN_SECTIONS, SETTINGS_SECTIONS } from '../lib/sections';
  import { duration, relDay } from '../lib/format';
  import { toastError } from '../lib/ui.svelte';
  import Logo from '../lib/Logo.svelte';
  import Icon from '../lib/components/Icon.svelte';
  import Meetings from './Meetings.svelte';
  import Live from './Live.svelte';
  import Search from './Search.svelte';
  import Actions from './Actions.svelte';
  import Settings from './Settings.svelte';

  const KEY = 'scribe.sidebar.collapsed';
  let collapsed = $state(false);
  try {
    collapsed = localStorage.getItem(KEY) === '1';
  } catch {}
  function toggle() {
    collapsed = !collapsed;
    try {
      localStorage.setItem(KEY, collapsed ? '1' : '0');
    } catch {}
  }

  const isMac = typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform);
  const modKey = isMac ? '⌘' : 'Ctrl+';

  const section = $derived(router.route.parts[0] ?? 'meetings');
  const currentMeeting = $derived(section === 'meetings' ? router.route.parts[1] : undefined);
  const settingsSection = $derived(router.route.query.section ?? 'agent');
  const s = $derived(live.s);
  const isLive = $derived(s.state === 'listening' || s.state === 'paused');

  onMount(initRecent);

  function onkeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key === '\\') {
      e.preventDefault();
      toggle();
    }
  }

  let busy = $state(false);
  async function startStop() {
    busy = true;
    try {
      if (isLive) await api.stopMeeting();
      else {
        await api.startMeeting();
        navigate('/live');
      }
    } catch (e) {
      toastError(e);
    } finally {
      busy = false;
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="shell" class:collapsed>
  <nav class="side" aria-label="Navigation principale">
    {#if isTauri}<div class="drag" data-tauri-drag-region></div>{/if}
    <div class="brand">
      <span class="logo"><Logo size={collapsed ? 22 : 20} /></span>
      {#if !collapsed}<span class="name">Scribe</span>{/if}
      <button class="collapse" onclick={toggle} title="{collapsed ? 'Déplier' : 'Replier'} la barre latérale ({modKey}\)" aria-label="{collapsed ? 'Déplier' : 'Replier'} la barre latérale" aria-expanded={!collapsed}>
        <Icon name="sidebar" size={15} />
      </button>
    </div>

    {#if s.meeting && s.state !== 'idle'}
      <a class="live-card" href={href('/live')} title="Réunion en cours">
        <span class="dot {s.state === 'listening' ? 'live' : s.state}"></span>
        {#if !collapsed}
          <span class="grow">
            <span class="lc-state">{STATE_LABEL[s.state]}{#if s.state !== 'processing'}{' · '}{duration(s.meeting.started_at, null, clock.now)}{/if}</span>
            <span class="lc-title ellipsis">{s.meeting.title ?? 'Réunion sans titre'}</span>
          </span>
        {/if}
      </a>
    {/if}

    <div class="nav scroll">
      {#each MAIN_SECTIONS as item, i (item.id)}
        <a
          class="item"
          class:active={section === item.id}
          href={href(item.path)}
          aria-current={section === item.id ? 'page' : undefined}
          title={collapsed ? `${item.label} (${modKey}${i + 1})` : `${modKey}${i + 1}`}
        >
          <Icon name={item.icon} size={16} />
          {#if !collapsed}
            <span class="grow ellipsis">{item.label}</span>
            {#if item.id === 'live' && s.state === 'listening'}<span class="dot live small-dot" aria-label="Écoute en cours"></span>{/if}
            {#if item.id === 'live' && s.pending_requests > 0}<span class="count">{s.pending_requests}</span>{/if}
          {/if}
        </a>

        {#if !collapsed && item.id === 'meetings' && recent.list.length}
          <ul class="sub" aria-label="Réunions récentes">
            {#each recent.list.slice(0, 5) as m (m.id)}
              <li>
                <a class="subitem" class:active={currentMeeting === m.id} href={href(`/meetings/${m.id}`)} aria-current={currentMeeting === m.id ? 'page' : undefined}>
                  <span class="grow ellipsis">{m.title ?? 'Sans titre'}</span>
                  {#if m.status === 'live'}<span class="dot live small-dot"></span>{:else}<span class="when">{relDay(m.started_at)}</span>{/if}
                </a>
              </li>
            {/each}
          </ul>
        {/if}

        {#if !collapsed && item.id === 'settings' && section === 'settings'}
          <ul class="sub" aria-label="Sections des réglages">
            {#each SETTINGS_SECTIONS as sec (sec.id)}
              <li>
                <a class="subitem" class:active={settingsSection === sec.id} href={href(`/settings?section=${sec.id}`)} aria-current={settingsSection === sec.id ? 'page' : undefined}>
                  <span class="grow ellipsis">{sec.label}</span>
                </a>
              </li>
            {/each}
          </ul>
        {/if}
      {/each}
    </div>

    <div class="foot">
      <button class="start" class:stop={isLive} onclick={startStop} disabled={busy || s.state === 'processing'} title={isLive ? 'Arrêter la réunion' : 'Démarrer une réunion'}>
        <Icon name={isLive ? 'stop' : 'play'} size={14} />
        {#if !collapsed}<span>{isLive ? 'Arrêter la réunion' : 'Démarrer une réunion'}</span>{/if}
      </button>
    </div>
  </nav>

  <main class="content">
    {#if section === 'live'}
      <Live />
    {:else if section === 'search'}
      <Search />
    {:else if section === 'actions'}
      <Actions />
    {:else if section === 'settings'}
      <Settings />
    {:else}
      <Meetings />
    {/if}
  </main>
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: 228px 1fr;
    height: 100%;
    transition: grid-template-columns 0.18s ease;
  }
  .shell.collapsed { grid-template-columns: 56px 1fr; }

  .side {
    display: flex;
    flex-direction: column;
    min-width: 0;
    background: var(--side-bg);
    color: var(--side-text);
    padding: 0 10px 10px;
    border-right: 1px solid var(--side-line);
  }
  .drag { height: 22px; flex: none; }
  .brand {
    display: flex;
    align-items: center;
    gap: 9px;
    height: 48px;
    padding: 0 4px 0 6px;
    flex: none;
  }
  .collapsed .brand { flex-direction: column; height: auto; padding: 12px 0 8px; gap: 10px; }
  .logo { color: #f1f0ec; display: flex; }
  .name { font-size: 15px; font-weight: 600; letter-spacing: 0.01em; flex: 1; }
  .collapse {
    border: 0;
    background: transparent;
    color: var(--side-text-2);
    width: 26px;
    height: 26px;
    border-radius: 6px;
    display: grid;
    place-items: center;
  }
  .collapse:hover { background: var(--side-hover); color: var(--side-text); }

  .live-card {
    display: flex;
    align-items: center;
    gap: 9px;
    margin: 2px 0 10px;
    padding: 8px 10px;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid var(--side-line);
    color: inherit;
    text-decoration: none;
    min-width: 0;
  }
  .collapsed .live-card { justify-content: center; padding: 10px 0; }
  .live-card:hover { background: rgba(255, 255, 255, 0.09); }
  .lc-state { display: block; font-size: 11px; font-weight: 600; color: var(--side-text-2); letter-spacing: 0.02em; }
  .lc-title { display: block; font-size: 12.5px; font-weight: 500; }
  .live-card .dot.live { box-shadow: 0 0 0 3px rgba(232, 103, 94, 0.22); }

  .nav { flex: 1; display: flex; flex-direction: column; gap: 1px; margin: 0 -4px; padding: 0 4px; }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 32px;
    padding: 0 10px;
    border-radius: 7px;
    color: var(--side-text);
    text-decoration: none;
    font-weight: 500;
    position: relative;
  }
  .collapsed .item { justify-content: center; padding: 0; }
  .item :global(.icon) { opacity: 0.78; }
  .item:hover { background: var(--side-hover); }
  .item.active { background: var(--side-active); color: #fff; }
  .item.active :global(.icon) { opacity: 1; }
  .item.active::before {
    content: '';
    position: absolute;
    left: -4px;
    top: 8px;
    bottom: 8px;
    width: 3px;
    border-radius: 2px;
    background: #f1f0ec;
  }
  .item:focus-visible, .subitem:focus-visible, .collapse:focus-visible, .live-card:focus-visible, .start:focus-visible {
    outline: 2px solid #9fb4ea;
    outline-offset: -2px;
  }
  .small-dot { width: 7px; height: 7px; }
  .count {
    min-width: 18px;
    height: 17px;
    padding: 0 5px;
    border-radius: 9px;
    background: rgba(255, 255, 255, 0.14);
    font-size: 11px;
    display: grid;
    place-items: center;
  }

  .sub { list-style: none; margin: 1px 0 6px; padding: 0 0 0 26px; }
  .subitem {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 26px;
    padding: 0 8px;
    border-radius: 6px;
    color: var(--side-text-2);
    text-decoration: none;
    font-size: 12.5px;
  }
  .subitem:hover { background: var(--side-hover); color: var(--side-text); }
  .subitem.active { color: #fff; background: rgba(255, 255, 255, 0.07); }
  .when { font-size: 11px; opacity: 0.75; flex: none; }

  .foot { flex: none; padding-top: 10px; border-top: 1px solid var(--side-line); }
  .start {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    height: 30px;
    border-radius: 7px;
    border: 1px solid rgba(255, 255, 255, 0.16);
    background: rgba(255, 255, 255, 0.06);
    color: var(--side-text);
    font: inherit;
    font-weight: 500;
  }
  .start:hover:not(:disabled) { background: rgba(255, 255, 255, 0.11); }
  .start.stop { border-color: rgba(232, 103, 94, 0.5); color: #ffb3ad; }
  .start:disabled { opacity: 0.5; }

  .content { min-width: 0; min-height: 0; height: 100%; overflow: hidden; background: var(--bg); }
</style>
