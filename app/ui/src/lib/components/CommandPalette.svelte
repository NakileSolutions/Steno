<script lang="ts">
  // Palette de commandes ⌘K (à la Raycast / Linear) : réunions (plein texte), actions, récemment ouvert.
  // N'appelle que des commandes existantes de l'API.
  import { tick } from 'svelte';
  import { api } from '../api';
  import { navigate } from '../router.svelte';
  import { live } from '../live.svelte';
  import { isListening, formatShortcut } from '../listening';
  import { prefs } from '../prefs.svelte';
  import { recent } from '../meetings.svelte';
  import { opened } from '../opened.svelte';
  import { MAIN_SECTIONS, SETTINGS_SECTIONS } from '../sections';
  import { cleanSnippet, dayLabel, debounce, relDay, time } from '../format';
  import { toastError } from '../ui.svelte';
  import { buildGroups, filterItems, flatten, mergeById, moveIndex, type PaletteItem } from '../palette';
  import type { MeetingSummary } from '../types';
  import Icon from './Icon.svelte';

  type Item = PaletteItem & { run: () => void | Promise<void> };

  let {
    open = $bindable(false),
    sidebarCollapsed = false,
    ontogglesidebar,
  }: { open?: boolean; sidebarCollapsed?: boolean; ontogglesidebar?: () => void } = $props();

  let el: HTMLDialogElement | undefined = $state();
  let input: HTMLInputElement | undefined = $state();
  let listEl: HTMLElement | undefined = $state();
  let query = $state('');
  let active = $state(0);
  let all = $state<MeetingSummary[]>([]);
  let found = $state<MeetingSummary[]>([]);
  let seq = 0;

  const isMac = typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform);
  const mod = isMac ? '⌘' : 'Ctrl+';

  $effect(() => {
    if (open && el && !el.open) {
      query = '';
      found = [];
      active = 0;
      el.showModal();
      tick().then(() => input?.focus());
      api.listMeetings({ limit: 200 }).then((l) => (all = l)).catch(() => (all = recent.list));
    } else if (!open && el?.open) el.close();
  });

  function close() {
    open = false;
  }

  // Recherche plein texte côté backend (anti-rebond), en plus des titres connus localement.
  const searchSoon = debounce((q: string) => {
    const my = ++seq;
    api
      .listMeetings({ query: q, limit: 8 })
      .then((l) => {
        if (my === seq) found = l;
      })
      .catch(() => {});
  }, 120);

  function oninput() {
    active = 0;
    const q = query.trim();
    if (q.length >= 2) searchSoon(q);
    else {
      seq++;
      found = [];
    }
  }

  async function go(path: string) {
    navigate(path, { transition: true });
  }

  function meetingItem(m: MeetingSummary): Item {
    const q = query.trim();
    const snippet = m.snippet && q ? cleanSnippet(m.snippet, q) : '';
    return {
      id: `m:${m.id}`,
      title: m.title ?? 'Réunion sans titre',
      subtitle: snippet || `${dayLabel(m.started_at)} · ${time(m.started_at)}`,
      hint: m.status === 'live' ? 'En cours' : relDay(m.started_at),
      icon: m.status === 'live' ? 'live' : 'meetings',
      run: () => go(`/meetings/${m.id}`),
    };
  }

  async function startStop() {
    try {
      if (isListening(live.s)) await api.stopMeeting();
      else {
        await api.startMeeting();
        go('/live');
      }
    } catch (e) {
      toastError(e);
    }
  }

  /** Nouvelle demande : pendant l'écoute, champ « Demander au Scribe » ; sinon, recherche dans l'historique. */
  async function newRequest(q: string) {
    if (isListening(live.s) && live.s.meeting) {
      navigate('/live');
      await tick();
      await tick();
      const field = document.getElementById('live-ask') as HTMLInputElement | null;
      if (field) {
        if (q) {
          field.value = q;
          field.dispatchEvent(new Event('input', { bubbles: true }));
        }
        field.focus();
      }
    } else navigate(q ? `/search?q=${encodeURIComponent(q)}` : '/search');
  }

  const listening = $derived(isListening(live.s));
  const toggleKey = $derived(formatShortcut(prefs.toggle));

  const actions = $derived.by((): Item[] => {
    const items: Item[] = [
      {
        id: 'listen',
        title: listening ? 'Arrêter l’écoute' : 'Démarrer l’écoute',
        keywords: 'écoute enregistrer réunion micro transcrire stop',
        icon: listening ? 'stop' : 'play',
        shortcut: toggleKey || undefined,
        run: startStop,
      },
      {
        id: 'ask',
        title: 'Nouvelle demande au Scribe',
        keywords: 'question agent ia demander chercher',
        icon: 'ask',
        run: () => newRequest(''),
      },
      ...MAIN_SECTIONS.map((s, i) => ({
        id: `go:${s.id}`,
        title: `Aller à ${s.label}`,
        subtitle: 'Navigation',
        keywords: s.id === 'actions' ? 'décisions tâches' : '',
        icon: s.icon,
        shortcut: `${mod}${i + 1}`,
        run: () => go(s.path),
      })),
      ...SETTINGS_SECTIONS.map((s) => ({
        id: `settings:${s.id}`,
        title: `Réglages › ${s.label}`,
        keywords: 'préférences configuration',
        icon: s.icon,
        shortcut: s.id === 'agent' ? `${mod},` : undefined,
        run: () => go(`/settings?section=${s.id}`),
      })),
      {
        id: 'sidebar',
        title: sidebarCollapsed ? 'Afficher la barre latérale' : 'Réduire la barre latérale',
        keywords: 'sidebar panneau masquer',
        icon: 'sidebar',
        shortcut: `${mod}\\`,
        run: () => ontogglesidebar?.(),
      },
    ];
    return items;
  });

  const recentItems = $derived.by((): Item[] => {
    const byId = new Map(mergeById(all, recent.list).map((m) => [m.id, m]));
    return opened.ids.map((id) => byId.get(id)).filter((m): m is MeetingSummary => !!m).map(meetingItem);
  });

  const meetingItems = $derived.by((): Item[] => {
    const q = query.trim();
    if (!q) return [];
    const byTitle = filterItems(all.map(meetingItem), q);
    return mergeById(byTitle, found.map(meetingItem)).slice(0, 8);
  });

  /** Replis proposés pour toute requête : la poser au Scribe, ou la chercher partout. */
  const fallback = $derived.by((): Item[] => {
    const q = query.trim();
    if (!q) return [];
    return [
      {
        id: 'ask-q',
        title: `Demander au Scribe : « ${q} »`,
        subtitle: listening ? 'Demande pendant la réunion en cours' : 'Question sur l’historique des réunions',
        icon: 'ask',
        run: () => newRequest(q),
      },
      {
        id: 'search-all',
        title: `Rechercher « ${q} » dans toutes les réunions`,
        subtitle: 'Transcriptions et comptes rendus',
        icon: 'search',
        run: () => navigate(`/search?q=${encodeURIComponent(q)}`, { transition: true }),
      },
    ];
  });

  const groups = $derived(buildGroups({ query, recent: recentItems, meetings: meetingItems, actions, fallback }));
  const flat = $derived(flatten(groups));
  const activeItem = $derived(flat[Math.min(active, flat.length - 1)]);
  const optId = (it: Item) => `cmdk-${it.id.replace(/[^\w-]/g, '_')}`;

  $effect(() => {
    if (!activeItem || !listEl) return;
    listEl.querySelector(`#${optId(activeItem)}`)?.scrollIntoView({ block: 'nearest' });
  });

  async function run(it: Item | undefined) {
    if (!it) return;
    close();
    await tick();
    await it.run();
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp' || (e.ctrlKey && (e.key === 'n' || e.key === 'p'))) {
      e.preventDefault();
      const d = e.key === 'ArrowDown' || e.key === 'n' ? 1 : -1;
      active = moveIndex(Math.min(active, flat.length - 1), d, flat.length);
    } else if (e.key === 'Enter' && !e.isComposing) {
      e.preventDefault();
      run(activeItem);
    } else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      close();
    }
  }

  function onclick(e: MouseEvent) {
    // Clic sur le fond assombri : fermeture.
    if (e.target === el) close();
  }

  const indexOf = (it: Item) => flat.indexOf(it);
</script>

<dialog
  class="palette"
  bind:this={el}
  aria-label="Palette de commandes"
  oncancel={(e) => { e.preventDefault(); close(); }}
  onclose={() => (open = false)}
  {onclick}
>
  {#if open}
    <div class="box">
      <div class="qfield">
        <Icon name="search" size={18} stroke={1.8} />
        <input
          bind:this={input}
          bind:value={query}
          {oninput}
          {onkeydown}
          type="text"
          role="combobox"
          aria-expanded="true"
          aria-controls="cmdk-list"
          aria-autocomplete="list"
          aria-activedescendant={activeItem ? optId(activeItem) : undefined}
          aria-label="Rechercher une réunion ou une commande"
          placeholder="Rechercher une réunion, une commande…"
          autocomplete="off"
          spellcheck="false"
        />
        <kbd class="esc">échap</kbd>
      </div>

      <div class="list scroll" id="cmdk-list" role="listbox" aria-label="Résultats" bind:this={listEl}>
        {#each groups as g (g.id)}
          <div class="group" role="group" aria-labelledby="cmdk-g-{g.id}">
            <div class="glabel" id="cmdk-g-{g.id}">{g.label}</div>
            {#each g.items as it (it.id)}
              {@const i = indexOf(it)}
              <div
                class="opt"
                id={optId(it)}
                role="option"
                tabindex="-1"
                aria-selected={i === active}
                onpointermove={() => (active = i)}
                onclick={() => run(it)}
                onkeydown={() => {}}
              >
                <span class="oic" class:live={it.id.startsWith('m:') && it.icon === 'live'}><Icon name={it.icon ?? 'chevron'} size={15} /></span>
                <span class="otext">
                  <span class="otitle ellipsis">{it.title}</span>
                  {#if it.subtitle}<span class="osub ellipsis">{it.subtitle}</span>{/if}
                </span>
                {#if it.shortcut}<kbd class="sc">{it.shortcut}</kbd>
                {:else if it.hint}<span class="hint tnum">{it.hint}</span>{/if}
              </div>
            {/each}
          </div>
        {:else}
          <div class="none">
            <p>Aucun résultat pour « {query.trim()} ».</p>
          </div>
        {/each}
      </div>

      <footer class="foot" aria-hidden="true">
        <span><kbd>↑</kbd><kbd>↓</kbd> naviguer</span>
        <span><kbd>↵</kbd> ouvrir</span>
        <span><kbd>échap</kbd> fermer</span>
        <span class="grow"></span>
        <span class="brand">Scribe</span>
      </footer>
    </div>
  {/if}
</dialog>

<style>
  .palette {
    width: min(640px, calc(100vw - 32px));
    max-height: min(560px, calc(100vh - 120px));
    margin: 12vh auto auto;
    padding: 0;
    border: 0;
    border-radius: 16px;
    background: transparent;
    color: var(--text);
    overflow: visible;
  }
  .palette[open] { animation: cmdk-pop 170ms var(--ease-out); }
  .palette::backdrop {
    background: rgba(12, 14, 22, 0.32);
    -webkit-backdrop-filter: blur(3px) saturate(120%);
    backdrop-filter: blur(3px) saturate(120%);
    animation: cmdk-fade 170ms var(--ease-out);
  }
  @keyframes cmdk-pop { from { opacity: 0; transform: translateY(-6px) scale(0.98); } }
  @keyframes cmdk-fade { from { opacity: 0; } }

  .box {
    display: flex;
    flex-direction: column;
    max-height: min(560px, calc(100vh - 120px));
    border-radius: 16px;
    background: color-mix(in srgb, var(--surface) 94%, transparent);
    -webkit-backdrop-filter: blur(30px) saturate(180%);
    backdrop-filter: blur(30px) saturate(180%);
    box-shadow:
      0 0 0 0.5px rgba(0, 0, 0, 0.18),
      0 0 0 1px color-mix(in srgb, var(--line-strong) 60%, transparent) inset,
      0 4px 10px rgba(16, 18, 30, 0.08),
      0 30px 80px -12px rgba(16, 18, 30, 0.4);
    overflow: hidden;
  }

  .qfield {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 56px;
    padding: 0 16px 0 18px;
    border-bottom: 1px solid var(--line);
    color: var(--text-3);
    flex: none;
  }
  .qfield input {
    flex: 1;
    min-width: 0;
    height: 100%;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--text);
    font: 400 16px/1 var(--font);
    letter-spacing: -0.005em;
  }
  .qfield input::placeholder { color: var(--text-3); }
  .esc { font-family: var(--font); font-size: 10.5px; height: 20px; padding: 0 6px; }

  .list { flex: 1; min-height: 0; padding: 6px 8px 8px; scroll-padding: 8px 0; }
  .group + .group { margin-top: 4px; }
  .glabel {
    padding: 10px 10px 6px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    color: var(--text-3);
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 44px;
    padding: 0 10px;
    border-radius: 10px;
    cursor: default;
    user-select: none;
    transition: background 80ms linear;
  }
  .opt[aria-selected='true'] {
    background: var(--accent-soft-2);
    box-shadow: inset 0 0 0 1px var(--accent-line);
  }
  .opt:active { background: color-mix(in srgb, var(--accent) 22%, transparent); }
  .oic {
    width: 28px;
    height: 28px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    flex: none;
    color: var(--text-2);
    background: var(--surface-2);
    box-shadow: inset 0 0 0 1px var(--line);
  }
  .opt[aria-selected='true'] .oic { color: var(--accent-text); background: var(--surface); box-shadow: inset 0 0 0 1px var(--accent-line), 0 1px 2px rgba(0, 0, 0, 0.06); }
  .oic.live { color: var(--live); }
  .otext { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .otitle { font-size: 13.5px; font-weight: 500; color: var(--text); }
  .osub { font-size: 11.5px; color: var(--text-3); }
  .sc { font-family: var(--font); font-size: 11px; height: 22px; min-width: 24px; padding: 0 7px; background: var(--surface-2); flex: none; }
  .hint { font-size: 11.5px; color: var(--text-3); flex: none; }
  .none { padding: 36px 16px; text-align: center; color: var(--text-3); }

  .foot {
    display: flex;
    align-items: center;
    gap: 14px;
    height: 38px;
    padding: 0 14px;
    border-top: 1px solid var(--line);
    background: color-mix(in srgb, var(--surface-2) 55%, transparent);
    font-size: 11.5px;
    color: var(--text-3);
    flex: none;
  }
  .foot span { display: inline-flex; align-items: center; gap: 4px; }
  .foot kbd { font-family: var(--font); height: 18px; min-width: 18px; font-size: 10.5px; padding: 0 4px; }
  .brand { font-weight: 600; letter-spacing: 0.02em; }
</style>
