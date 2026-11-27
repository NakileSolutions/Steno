<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { router, href, navigate } from '../lib/router.svelte';
  import { live, clock, STATE_LABEL } from '../lib/live.svelte';
  import { callOffer, isListening } from '../lib/listening';
  import { prefs } from '../lib/prefs.svelte';
  import { recent, initRecent } from '../lib/meetings.svelte';
  import { markOpened } from '../lib/opened.svelte';
  import { updates, initUpdates, updateBadge } from '../lib/updates.svelte';
  import { api, isTauri } from '../lib/api';
  import { MAIN_SECTIONS, SETTINGS_SECTIONS } from '../lib/sections';
  import { clockDuration, relDay } from '../lib/format';
  import { toastError } from '../lib/ui.svelte';
  import { SidebarController, readCollapsed, writeCollapsed, type SidebarState } from '../lib/sidebar';
  import Logo from '../lib/Logo.svelte';
  import Icon from '../lib/components/Icon.svelte';
  import CommandPalette from '../lib/components/CommandPalette.svelte';
  import Meetings from './Meetings.svelte';
  import Live from './Live.svelte';
  import Search from './Search.svelte';
  import Actions from './Actions.svelte';
  import Settings from './Settings.svelte';

  // ---- Barre latérale : déployée / réduite (mémorisé), aperçu flottant au survol du bord ----
  const storage = (() => {
    try {
      return window.localStorage;
    } catch {
      return null;
    }
  })();
  const startCollapsed = readCollapsed(storage);
  let sb = $state<SidebarState>({ mode: startCollapsed ? 'collapsed' : 'expanded', peek: 'hidden' });
  const ctl = new SidebarController({
    collapsed: startCollapsed,
    timers: { set: (fn, ms) => window.setTimeout(fn, ms), clear: (id) => window.clearTimeout(id as number) },
    onChange: (st) => (sb = st),
    persist: (v) => writeCollapsed(storage, v),
  });
  const collapsed = $derived(sb.mode === 'collapsed');
  const peeking = $derived(collapsed && sb.peek === 'shown');

  let navEl: HTMLElement | undefined = $state();
  let sideBtn: HTMLButtonElement | undefined = $state();
  let floatBtn: HTMLButtonElement | undefined = $state();

  async function toggle() {
    const active = document.activeElement;
    const fromSide = !!navEl && navEl.contains(active);
    const fromFloat = active === floatBtn;
    ctl.toggle();
    await tick();
    // Le focus suit le bouton de bascule visible.
    if (sb.mode === 'collapsed' && fromSide) floatBtn?.focus();
    else if (sb.mode === 'expanded' && fromFloat) sideBtn?.focus();
  }

  function pin() {
    ctl.pin();
  }

  /** Focus clavier (pas un simple clic) ou champ actif : l'aperçu reste ouvert. */
  function onfocusin(e: FocusEvent) {
    if (!collapsed) return;
    const el = e.target as HTMLElement;
    const field = /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName);
    let kb = false;
    try {
      kb = el.matches(':focus-visible');
    } catch {}
    if (field || kb) ctl.hold(true);
  }
  function onfocusout(e: FocusEvent) {
    if (!navEl?.contains(e.relatedTarget as Node | null)) ctl.hold(false);
  }
  function onnavclick(e: MouseEvent) {
    // Cliquer un élément de l'aperçu : on navigue et l'aperçu se referme.
    if (collapsed && (e.target as HTMLElement).closest('a[href]')) ctl.dismiss();
  }

  // Toute navigation referme l'aperçu.
  $effect(() => {
    void router.route.path;
    void router.route.query.section;
    ctl.dismiss();
  });

  // ---- Palette de commandes ----
  let paletteOpen = $state(false);
  function openPalette() {
    ctl.dismiss();
    paletteOpen = true;
  }

  const isMac = typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform);
  const modKey = isMac ? '⌘' : 'Ctrl+';

  const section = $derived(router.route.parts[0] ?? 'meetings');
  const currentMeeting = $derived(section === 'meetings' ? router.route.parts[1] : undefined);
  const settingsSection = $derived(router.route.query.section ?? 'agent');
  const s = $derived(live.s);
  const isLive = $derived(isListening(s));
  const offer = $derived(callOffer(s, prefs.callApps));
  const elapsed = $derived(s.meeting ? clockDuration(Math.max(0, Math.round((clock.now - new Date(s.meeting.started_at).getTime()) / 1000))) : '');

  onMount(initRecent);
  $effect(() => {
    if (currentMeeting) markOpened(currentMeeting);
  });

  function onkeydown(e: KeyboardEvent) {
    if (document.querySelector('dialog[open]')) return;
    const mod = (e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey;
    if (mod && (e.key === '\\' || e.code === 'Backslash')) {
      e.preventDefault();
      toggle();
    } else if (mod && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      openPalette();
    } else if (e.key === 'Escape' && peeking) {
      ctl.dismiss();
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

  // ---- Mises à jour : signalées en bas de la barre latérale ----
  initUpdates();
  const badge = $derived(updateBadge(updates.status));
  async function restart() {
    try {
      await api.restartApp();
    } catch (e) {
      toastError(e);
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="shell" class:collapsed class:peeking>
  {#if collapsed}
    <!-- Mode réduit : barre de titre discrète (boutons de fenêtre macOS à gauche), bouton flottant. -->
    <div class="titlebar" data-tauri-drag-region>
      <button
        class="tb-btn"
        bind:this={floatBtn}
        onclick={toggle}
        aria-label="Afficher la barre latérale"
        title="Afficher la barre latérale ({modKey}\)"
        aria-expanded="false"
        aria-controls="sidebar"
      ><Icon name="sidebar" size={16} /></button>
      <button class="tb-btn" onclick={openPalette} aria-label="Rechercher" title="Rechercher ({modKey}K)" aria-keyshortcuts="Meta+K"><Icon name="search" size={15} /></button>
      {#if s.meeting && s.state !== 'idle'}
        <a class="tb-live" class:rec={s.state === 'listening'} href={href('/live')} title="Réunion en cours : {STATE_LABEL[s.state]}">
          <span class="dot {s.state === 'listening' ? 'live' : s.state}"></span>
          <span class="ellipsis">{s.meeting.title ?? 'Réunion sans titre'}</span>
          {#if s.state !== 'processing'}<span class="tnum tb-time">{elapsed}</span>{/if}
        </a>
      {:else if offer}
        <a class="tb-live offer" href={href('/live')} title="{offer.label}. Sténo n’écoute pas encore.">
          <Icon name="phone" size={12} />
          <span class="ellipsis">{offer.heading}</span>
        </a>
      {/if}
      <span class="grow" data-tauri-drag-region></span>
    </div>
    <!-- Zone sensible invisible sur le bord gauche : aperçu au survol, comme Arc. -->
    <div class="edge" aria-hidden="true" onpointerenter={() => ctl.edgeEnter()} onpointerleave={() => ctl.edgeLeave()}></div>
  {/if}

  <!-- svelte-ignore a11y_no_noninteractive_element_interactions, a11y_click_events_have_key_events -->
  <nav
    class="side"
    class:peek={peeking}
    id="sidebar"
    aria-label="Navigation principale"
    bind:this={navEl}
    inert={collapsed && !peeking}
    onpointerenter={() => ctl.panelEnter()}
    onpointerleave={() => ctl.panelLeave()}
    {onfocusin}
    {onfocusout}
    onclick={onnavclick}
  >
    {#if isTauri}<div class="drag" data-tauri-drag-region></div>{/if}
    <div class="brand">
      <span class="logo"><Logo size={20} /></span>
      <span class="name">Sténo</span>
      {#if peeking}
        <button class="collapse" onclick={pin} title="Épingler la barre latérale ({modKey}\)" aria-label="Épingler la barre latérale" aria-expanded="false" aria-controls="sidebar">
          <Icon name="pin" size={15} />
        </button>
      {:else}
        <button class="collapse" bind:this={sideBtn} onclick={toggle} title="Replier la barre latérale ({modKey}\)" aria-label="Replier la barre latérale" aria-expanded={!collapsed} aria-controls="sidebar">
          <Icon name="sidebar" size={16} />
        </button>
      {/if}
    </div>

    <button class="cmdk" onclick={openPalette} aria-keyshortcuts="Meta+K" title="Palette de commandes">
      <Icon name="search" size={14} />
      <span class="grow">Rechercher…</span>
      <kbd>{modKey}K</kbd>
    </button>

    {#if s.meeting && s.state !== 'idle'}
      <a class="live-card" class:rec={s.state === 'listening'} href={href('/live')} title="Réunion en cours">
        <span class="dot {s.state === 'listening' ? 'live' : s.state}"></span>
        <span class="grow">
          <span class="lc-state">{STATE_LABEL[s.state]}{#if s.state !== 'processing'}<span class="tnum">{' · '}{elapsed}</span>{/if}</span>
          <span class="lc-title ellipsis">{s.meeting.title ?? 'Réunion sans titre'}</span>
        </span>
      </a>
    {:else if offer}
      <a class="live-card offer" href={href('/live')} title="{offer.label}. Sténo n’écoute pas encore.">
        <span class="lc-ic"><Icon name="phone" size={14} /></span>
        <span class="grow">
          <span class="lc-state">{offer.heading}</span>
          <span class="lc-title ellipsis">{offer.title}</span>
        </span>
      </a>
    {/if}

    <div class="nav scroll">
      {#each MAIN_SECTIONS as item, i (item.id)}
        <a
          class="item"
          class:active={section === item.id}
          href={href(item.path)}
          aria-current={section === item.id ? 'page' : undefined}
          title={`${modKey}${i + 1}`}
        >
          <Icon name={item.icon} size={17} />
          <span class="grow ellipsis">{item.label}</span>
          {#if item.id === 'live' && s.state === 'listening'}<span class="dot live small-dot" aria-label="Écoute en cours"></span>{/if}
          {#if item.id === 'live' && s.pending_requests > 0}<span class="count">{s.pending_requests}</span>{/if}
          {#if !(item.id === 'live' && (s.state === 'listening' || s.pending_requests > 0))}<span class="kbd-hint" aria-hidden="true">{modKey}{i + 1}</span>{/if}
        </a>

        {#if item.id === 'settings' && section === 'settings'}
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

      {#if recent.list.length}
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
      {#if badge}
        {#if badge.kind === 'installed'}
          <button class="upd installed" onclick={restart} title="Redémarrer Sténo pour utiliser la nouvelle version"><Icon name="refresh" size={13} /><span class="ellipsis">{badge.label}</span></button>
        {:else}
          <a class="upd {badge.kind}" href={href('/settings?section=updates')}>
            {#if badge.kind === 'installing'}<span class="spin" aria-hidden="true"></span>{:else}<Icon name="download" size={13} />{/if}
            <span class="ellipsis">{badge.label}</span>
          </a>
        {/if}
      {/if}
      <button class="start" class:stop={isLive} onclick={startStop} disabled={busy || s.state === 'processing'} title={isLive ? 'Arrêter l’écoute' : 'Démarrer l’écoute'} aria-label={isLive ? 'Arrêter l’écoute' : 'Démarrer l’écoute'}>
        {#if isLive}<span class="stopsq" aria-hidden="true"></span>{:else}<Icon name="play" size={14} stroke={2} />{/if}
        <span>{isLive ? 'Arrêter l’écoute' : 'Démarrer l’écoute'}</span>
      </button>
    </div>
  </nav>

  <main class="content">
    {#key section}
      <div class="view">
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
      </div>
    {/key}
  </main>
</div>

<CommandPalette bind:open={paletteOpen} sidebarCollapsed={collapsed} ontogglesidebar={toggle} />

<style>
  .shell {
    --side-w: 240px;
    /* Mode réduit : barre de titre et bouton flottant. Dans Tauri sur macOS, le bouton se place
       juste après les boutons de fenêtre (≈ 78 px) et s'aligne sur eux. */
    --tb-h: 42px;
    --tb-left: 10px;
    --tb-top: 7px;
    --slide: 200ms;
    position: relative;
    height: 100%;
    background: var(--side-bg);
    isolation: isolate;
    overflow: hidden;
  }
  :global(html.native-vibrancy) .shell { --tb-h: 38px; --tb-left: 78px; --tb-top: 2px; }
  /* Grain imperceptible sur la grande surface sombre. */
  .shell::before {
    content: '';
    position: absolute;
    inset: 0;
    background-image: var(--noise);
    opacity: var(--noise-opacity);
    pointer-events: none;
    z-index: -1;
  }

  /* ---------- Barre latérale ---------- */
  .side {
    position: absolute;
    z-index: 20;
    top: 0;
    left: 0;
    bottom: 0;
    width: var(--side-w);
    display: flex;
    flex-direction: column;
    min-width: 0;
    color: var(--side-text);
    padding: 0 10px 12px;
    border-radius: 0;
    background-color: transparent;
    transition:
      transform var(--slide) var(--ease-out),
      opacity var(--slide) var(--ease-out),
      top var(--slide) var(--ease-out),
      left var(--slide) var(--ease-out),
      bottom var(--slide) var(--ease-out),
      border-radius var(--slide) var(--ease-out),
      box-shadow var(--slide) var(--ease-out),
      background-color var(--slide) var(--ease-out),
      visibility 0s;
  }
  /* Réduite : la barre sort par la gauche en fondu, puis devient invisible (ni focus, ni lecteur d'écran). */
  .collapsed .side {
    top: 8px;
    left: 8px;
    bottom: 8px;
    border-radius: 14px;
    background-color: var(--peek-bg);
    box-shadow: var(--peek-shadow);
    transform: translateX(calc(-100% - 20px));
    opacity: 0;
    visibility: hidden;
    transition-property: transform, opacity, visibility;
    transition-delay: 0s, 0s, var(--slide);
  }
  /* Aperçu : panneau flottant par-dessus le contenu. */
  .collapsed .side.peek {
    transform: none;
    opacity: 1;
    visibility: visible;
    transition-delay: 0s;
    -webkit-backdrop-filter: blur(24px) saturate(170%);
    backdrop-filter: blur(24px) saturate(170%);
  }
  .side.peek::after {
    /* Liseré lumineux du verre */
    content: '';
    position: absolute;
    inset: 0;
    border-radius: inherit;
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.08), inset 0 1px 0 rgba(255, 255, 255, 0.06);
    pointer-events: none;
  }

  .drag { height: 22px; flex: none; }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 50px;
    padding: 0 2px 0 8px;
    flex: none;
  }
  .logo { color: #eef0f6; display: flex; filter: drop-shadow(0 1px 6px rgba(169, 178, 255, 0.25)); }
  .name { font: 600 15px/1 var(--font-display); letter-spacing: -0.01em; flex: 1; color: #f3f4f8; }
  .collapse, .tb-btn {
    border: 0;
    background: transparent;
    color: var(--side-text-3);
    width: 28px;
    height: 28px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    flex: none;
    transition: background var(--t-fast) var(--ease), color var(--t-fast) var(--ease), transform var(--t-fast) var(--ease);
  }
  .collapse:hover, .tb-btn:hover { background: var(--side-hover); color: var(--side-text); }
  .collapse:active, .tb-btn:active { transform: scale(0.94); background: var(--side-active); }

  .cmdk {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    margin: 0 0 12px;
    padding: 0 6px 0 10px;
    border: 0;
    border-radius: 9px;
    background: rgba(255, 255, 255, 0.055);
    box-shadow: inset 0 0 0 1px var(--side-line), inset 0 1px 0 rgba(255, 255, 255, 0.03);
    color: var(--side-text-2);
    font: inherit;
    text-align: left;
    flex: none;
    transition: background var(--t-fast) var(--ease), color var(--t-fast) var(--ease), box-shadow var(--t-fast) var(--ease);
  }
  .cmdk:hover { background: rgba(255, 255, 255, 0.085); color: var(--side-text); }
  .cmdk:active { background: rgba(255, 255, 255, 0.11); }
  .cmdk :global(.icon) { opacity: 0.8; }
  .cmdk kbd {
    background: rgba(255, 255, 255, 0.07);
    border-color: rgba(255, 255, 255, 0.1);
    color: var(--side-text-2);
    font-family: var(--font);
    font-size: 10.5px;
    height: 20px;
    padding: 0 6px;
    border-bottom-width: 1px;
  }

  .live-card {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0 0 12px;
    padding: 9px 11px;
    border-radius: 11px;
    background: rgba(255, 255, 255, 0.05);
    box-shadow: inset 0 0 0 1px var(--side-line);
    color: inherit;
    text-decoration: none;
    min-width: 0;
    flex: none;
    animation: appear var(--t) var(--ease);
    transition: background var(--t-fast) var(--ease);
  }
  .live-card:hover { background: rgba(255, 255, 255, 0.08); }
  .live-card.rec { background: linear-gradient(180deg, rgba(255, 99, 105, 0.14), rgba(255, 99, 105, 0.06)); box-shadow: inset 0 0 0 1px rgba(255, 99, 105, 0.24), 0 6px 20px -8px rgba(255, 99, 105, 0.35); }
  .live-card.rec .dot.live { background: #ff6b70; }
  .live-card.rec .dot.live::after { background: #ff6b70; }
  .lc-state { display: block; font-size: 11px; font-weight: 600; color: var(--side-text-2); letter-spacing: 0.02em; }
  .live-card.rec .lc-state { color: #ffb1b4; }
  .lc-title { display: block; font-size: 13px; font-weight: 500; color: #f3f4f8; margin-top: 1px; }
  .lc-ic { width: 26px; height: 26px; border-radius: 8px; display: grid; place-items: center; flex: none; background: rgba(169, 178, 255, 0.16); color: var(--side-accent); }
  .live-card.offer { background: linear-gradient(180deg, rgba(169, 178, 255, 0.14), rgba(169, 178, 255, 0.06)); box-shadow: inset 0 0 0 1px rgba(169, 178, 255, 0.24); }
  .live-card.offer:hover { background: rgba(169, 178, 255, 0.17); }
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
    flex: none;
    transition: background var(--t-fast) var(--ease), color var(--t-fast) var(--ease), transform var(--t-fast) var(--ease);
  }
  .item :global(.icon) { opacity: 0.66; transition: opacity var(--t-fast) var(--ease), color var(--t-fast) var(--ease); }
  .item:hover { background: var(--side-hover); }
  .item:active { background: var(--side-active); }
  .item:hover :global(.icon) { opacity: 0.9; }
  .item.active {
    background: linear-gradient(180deg, rgba(255, 255, 255, 0.115), rgba(255, 255, 255, 0.085));
    color: #fff;
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.05), inset 0 1px 0 rgba(255, 255, 255, 0.05), 0 1px 2px rgba(0, 0, 0, 0.2);
  }
  .item.active :global(.icon) { opacity: 1; color: var(--side-accent); }
  .kbd-hint { font-size: 11px; color: var(--side-text-3); opacity: 0; transition: opacity var(--t-fast) var(--ease); font-variant-numeric: tabular-nums; }
  .item:hover .kbd-hint { opacity: 1; }
  .item:focus-visible, .subitem:focus-visible, .collapse:focus-visible, .live-card:focus-visible, .start:focus-visible, .cmdk:focus-visible, .tb-btn:focus-visible, .tb-live:focus-visible {
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

  .foot { flex: none; padding-top: 12px; display: flex; flex-direction: column; gap: 8px; }
  .upd {
    all: unset;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    padding: 7px 10px;
    border-radius: 9px;
    font-size: var(--fs-meta);
    font-weight: 550;
    cursor: pointer;
    color: #dfe3ff;
    background: rgba(124, 137, 255, 0.16);
    box-shadow: inset 0 0 0 1px rgba(124, 137, 255, 0.3);
  }
  .upd:hover { background: rgba(124, 137, 255, 0.24); }
  .upd:focus-visible { outline: 2px solid var(--focus); }
  .upd.installed { color: #d9f7e4; background: rgba(52, 199, 120, 0.18); box-shadow: inset 0 0 0 1px rgba(52, 199, 120, 0.35); }
  @keyframes spin { to { transform: rotate(360deg); } }
  .upd .spin { width: 11px; height: 11px; border: 1.5px solid currentColor; border-right-color: transparent; border-radius: 50%; animation: spin 0.8s linear infinite; flex: none; }

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
    background: linear-gradient(180deg, #6573f6, #4f5ee8);
    color: #fff;
    font: inherit;
    font-weight: 600;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.3), 0 6px 18px -4px rgba(79, 94, 232, 0.55), inset 0 1px 0 rgba(255, 255, 255, 0.2);
    transition: filter var(--t-fast) var(--ease), background var(--t-fast) var(--ease), transform var(--t-fast) var(--ease);
  }
  .start:hover:not(:disabled) { filter: brightness(1.08); }
  .start:active:not(:disabled) { transform: scale(0.985); filter: brightness(0.96); }
  .start.stop {
    background: rgba(255, 99, 105, 0.1);
    color: #ffb1b4;
    box-shadow: inset 0 0 0 1px rgba(255, 99, 105, 0.4);
    font-weight: 500;
  }
  .start.stop:hover:not(:disabled) { background: rgba(255, 99, 105, 0.17); filter: none; }
  .stopsq { width: 10px; height: 10px; border-radius: 2.5px; background: currentColor; }
  .start:disabled { opacity: 0.5; }

  /* ---------- Mode réduit : barre de titre, bord sensible ---------- */
  .titlebar {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: var(--tb-h);
    display: flex;
    align-items: flex-start;
    gap: 4px;
    padding: var(--tb-top) 12px 0 var(--tb-left);
    z-index: 2;
    animation: tb-in 220ms var(--ease-out) both;
  }
  .tb-btn { color: var(--side-text-2); }
  .tb-live {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 26px;
    max-width: 340px;
    margin: 1px 0 0 6px;
    padding: 0 10px;
    border-radius: var(--r-pill);
    font-size: 12px;
    font-weight: 500;
    color: var(--side-text);
    text-decoration: none;
    background: rgba(255, 255, 255, 0.06);
    box-shadow: inset 0 0 0 1px var(--side-line);
    transition: background var(--t-fast) var(--ease);
  }
  .tb-live:hover { background: rgba(255, 255, 255, 0.1); }
  .tb-live.rec { background: rgba(255, 99, 105, 0.12); box-shadow: inset 0 0 0 1px rgba(255, 99, 105, 0.26); }
  .tb-live.rec .dot.live, .tb-live.rec .dot.live::after { background: #ff6b70; }
  .tb-live .dot { width: 7px; height: 7px; }
  .tb-live.offer { color: #c4caff; background: rgba(169, 178, 255, 0.12); box-shadow: inset 0 0 0 1px rgba(169, 178, 255, 0.22); }
  .tb-time { color: #ffb1b4; font-weight: 600; }
  @keyframes tb-in { from { opacity: 0; transform: translateX(-6px); } }

  .edge {
    position: absolute;
    z-index: 25;
    top: 0;
    left: 0;
    bottom: 0;
    width: 8px;
  }

  /* ---------- Contenu ---------- */
  .content {
    position: absolute;
    z-index: 1;
    top: 8px;
    right: 8px;
    bottom: 8px;
    left: var(--side-w);
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: var(--bg);
    border-radius: 12px;
    box-shadow: var(--content-ring);
    isolation: isolate;
    view-transition-name: content;
    transition: left var(--slide) var(--ease-out), top var(--slide) var(--ease-out);
  }
  .collapsed .content { left: 8px; top: var(--tb-h); }
  .view { height: 100%; }
  /* Sans View Transitions : simple fondu au changement de section. */
  :global(html:not(.vt)) .view { animation: view-in 180ms var(--ease-out) both; }
  @keyframes view-in { from { opacity: 0; transform: translateY(4px); } }
</style>
