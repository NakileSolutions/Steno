<script lang="ts">
  import { onMount } from 'svelte';
  import { router, href, navigate } from '../lib/router.svelte';
  import { live, clock, STATE_LABEL } from '../lib/live.svelte';
  import { isListening, teamsOffer } from '../lib/listening';
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
  const isLive = $derived(isListening(s));
  const offer = $derived(teamsOffer(s));

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
        <Icon name="sidebar" size={16} />
      </button>
    </div>

    {#if s.meeting && s.state !== 'idle'}
      <a class="live-card" class:rec={s.state === 'listening'} href={href('/live')} title="Réunion en cours">
        <span class="dot {s.state === 'listening' ? 'live' : s.state}"></span>
        {#if !collapsed}
          <span class="grow">
            <span class="lc-state">{STATE_LABEL[s.state]}{#if s.state !== 'processing'}<span class="tnum">{' · '}{duration(s.meeting.started_at, null, clock.now)}</span>{/if}</span>
            <span class="lc-title ellipsis">{s.meeting.title ?? 'Réunion sans titre'}</span>
          </span>
        {/if}
      </a>
    {:else if offer}
      <a class="live-card offer" href={href('/live')} title="Appel Teams détecté : {offer}. Le Scribe n’écoute pas encore.">
        <span class="lc-ic"><Icon name="speaker" size={14} /></span>
        {#if !collapsed}
          <span class="grow">
            <span class="lc-state">Appel Teams détecté</span>
            <span class="lc-title ellipsis">{offer}</span>
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
          <Icon name={item.icon} size={17} />
          {#if !collapsed}
            <span class="grow ellipsis">{item.label}</span>
            {#if item.id === 'live' && s.state === 'listening'}<span class="dot live small-dot" aria-label="Écoute en cours"></span>{/if}
            {#if item.id === 'live' && s.pending_requests > 0}<span class="count">{s.pending_requests}</span>{/if}
            {#if !(item.id === 'live' && (s.state === 'listening' || s.pending_requests > 0))}<span class="kbd-hint" aria-hidden="true">{modKey}{i + 1}</span>{/if}
          {/if}
        </a>

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

      {#if !collapsed && recent.list.length}
        <p class="side-label" aria-hidden="true">Récentes</p>
        <ul class="recents" aria-label="Réunions récentes">
          {#each recent.list.slice(0, 5) as m (m.id)}
            <li>
              <a class="subitem" class:active={currentMeeting === m.id} href={href(`/meetings/${m.id}`)} aria-current={currentMeeting === m.id ? 'page' : undefined}>
                <span class="grow ellipsis">{m.title ?? 'Sans titre'}</span>
                {#if m.status === 'live'}<span class="dot live small-dot"></span>{:else}<span class="when tnum">{relDay(m.started_at)}</span>{/if}
              </a>
            </li>
          {/each}
        </ul>
      {/if}
    </div>

    <div class="foot">
      <button class="start" class:stop={isLive} onclick={startStop} disabled={busy || s.state === 'processing'} title={isLive ? 'Arrêter l’écoute' : 'Démarrer l’écoute'} aria-label={isLive ? 'Arrêter l’écoute' : 'Démarrer l’écoute'}>
        {#if isLive}<span class="stopsq" aria-hidden="true"></span>{:else}<Icon name="play" size={14} stroke={2} />{/if}
        {#if !collapsed}<span>{isLive ? 'Arrêter l’écoute' : 'Démarrer l’écoute'}</span>{/if}
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
    grid-template-columns: 232px 1fr;
    height: 100%;
    background: var(--side-bg);
    transition: grid-template-columns var(--t) var(--ease);
  }
  .shell.collapsed { grid-template-columns: 60px 1fr; }

  .side {
    display: flex;
    flex-direction: column;
    min-width: 0;
    color: var(--side-text);
    padding: 0 10px 12px;
  }
  .drag { height: 22px; flex: none; }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 52px;
    padding: 0 2px 0 8px;
    flex: none;
  }
  .collapsed .brand { flex-direction: column; height: auto; padding: 14px 0 10px; gap: 12px; }
  .logo { color: #eef0f6; display: flex; }
  .name { font: 600 15px/1 var(--font-display); letter-spacing: -0.005em; flex: 1; color: #f3f4f8; }
  .collapse {
    border: 0;
    background: transparent;
    color: var(--side-text-3);
    width: 28px;
    height: 28px;
    border-radius: 7px;
    display: grid;
    place-items: center;
    transition: background var(--t-fast) var(--ease), color var(--t-fast) var(--ease);
  }
  .collapse:hover { background: var(--side-hover); color: var(--side-text); }

  .live-card {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0 0 12px;
    padding: 9px 11px;
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.05);
    box-shadow: inset 0 0 0 1px var(--side-line);
    color: inherit;
    text-decoration: none;
    min-width: 0;
    animation: appear var(--t) var(--ease);
    transition: background var(--t-fast) var(--ease);
  }
  .collapsed .live-card { justify-content: center; padding: 11px 0; }
  .live-card:hover { background: rgba(255, 255, 255, 0.08); }
  .live-card.rec { background: linear-gradient(180deg, rgba(255, 99, 105, 0.12), rgba(255, 99, 105, 0.06)); box-shadow: inset 0 0 0 1px rgba(255, 99, 105, 0.22); }
  .live-card.rec .dot.live { background: #ff6b70; }
  .live-card.rec .dot.live::after { background: #ff6b70; }
  .lc-state { display: block; font-size: 11px; font-weight: 600; color: var(--side-text-2); letter-spacing: 0.02em; }
  .live-card.rec .lc-state { color: #ffb1b4; }
  .lc-title { display: block; font-size: 13px; font-weight: 500; color: #f3f4f8; margin-top: 1px; }
  .lc-ic { width: 26px; height: 26px; border-radius: 8px; display: grid; place-items: center; flex: none; background: rgba(169, 178, 255, 0.16); color: var(--side-accent); }
  .live-card.offer { background: linear-gradient(180deg, rgba(169, 178, 255, 0.13), rgba(169, 178, 255, 0.06)); box-shadow: inset 0 0 0 1px rgba(169, 178, 255, 0.22); }
  .live-card.offer:hover { background: rgba(169, 178, 255, 0.16); }
  .live-card.offer .lc-state { color: #c4caff; }

  .nav { flex: 1; display: flex; flex-direction: column; gap: 2px; margin: 0 -4px; padding: 0 4px; }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 32px;
    padding: 0 10px;
    border-radius: 8px;
    color: var(--side-text);
    text-decoration: none;
    font-weight: 500;
    position: relative;
    transition: background var(--t-fast) var(--ease), color var(--t-fast) var(--ease);
  }
  .collapsed .item { justify-content: center; padding: 0; }
  .item :global(.icon) { opacity: 0.66; transition: opacity var(--t-fast) var(--ease), color var(--t-fast) var(--ease); }
  .item:hover { background: var(--side-hover); }
  .item:hover :global(.icon) { opacity: 0.9; }
  .item.active { background: var(--side-active); color: #fff; box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.04), 0 1px 2px rgba(0, 0, 0, 0.18); }
  .item.active :global(.icon) { opacity: 1; color: var(--side-accent); }
  .kbd-hint { font-size: 11px; color: var(--side-text-3); opacity: 0; transition: opacity var(--t-fast) var(--ease); font-variant-numeric: tabular-nums; }
  .item:hover .kbd-hint { opacity: 1; }
  .item:focus-visible, .subitem:focus-visible, .collapse:focus-visible, .live-card:focus-visible, .start:focus-visible {
    outline: 2px solid var(--side-accent);
    outline-offset: 1px;
  }
  .small-dot { width: 7px; height: 7px; }
  .count {
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    border-radius: 9px;
    background: rgba(169, 178, 255, 0.22);
    color: #dfe2ff;
    font-size: 11px;
    font-weight: 600;
    display: grid;
    place-items: center;
    font-variant-numeric: tabular-nums;
  }

  .sub { list-style: none; margin: 2px 0 6px 19px; padding: 0 0 0 10px; border-left: 1px solid var(--side-line); }
  .side-label {
    margin: 20px 10px 6px;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--side-text-3);
  }
  .recents { list-style: none; margin: 0; padding: 0; }
  .recents .subitem { padding: 0 10px; height: 30px; }
  .subitem {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 28px;
    padding: 0 9px;
    border-radius: 7px;
    color: var(--side-text-2);
    text-decoration: none;
    font-size: 12.5px;
    transition: background var(--t-fast) var(--ease), color var(--t-fast) var(--ease);
  }
  .subitem:hover { background: var(--side-hover); color: var(--side-text); }
  .subitem.active { color: #fff; background: rgba(255, 255, 255, 0.08); }
  .when { font-size: 11px; color: var(--side-text-3); flex: none; }

  .foot { flex: none; padding-top: 12px; }

  /* « Démarrer l'écoute » : l'action principale, bien visible quand rien n'écoute. */
  .start {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    height: 36px;
    border-radius: 10px;
    border: 0;
    background: linear-gradient(180deg, #6170f5, #4f5ee8);
    color: #fff;
    font: inherit;
    font-weight: 600;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.3), 0 4px 14px rgba(79, 94, 232, 0.35), inset 0 1px 0 rgba(255, 255, 255, 0.18);
    transition: filter var(--t-fast) var(--ease), background var(--t-fast) var(--ease), transform var(--t-fast) var(--ease);
  }
  .start:hover:not(:disabled) { filter: brightness(1.08); }
  .start:active:not(:disabled) { transform: translateY(0.5px); }
  .start.stop {
    background: rgba(255, 99, 105, 0.1);
    color: #ffb1b4;
    box-shadow: inset 0 0 0 1px rgba(255, 99, 105, 0.4);
    font-weight: 500;
  }
  .start.stop:hover:not(:disabled) { background: rgba(255, 99, 105, 0.17); filter: none; }
  .stopsq { width: 10px; height: 10px; border-radius: 2.5px; background: currentColor; }
  .start:disabled { opacity: 0.5; }

  .content {
    min-width: 0;
    min-height: 0;
    height: calc(100% - 16px);
    margin: 8px 8px 8px 0;
    overflow: hidden;
    background: var(--bg);
    border-radius: 12px;
    box-shadow: var(--content-ring);
    isolation: isolate;
  }
</style>
