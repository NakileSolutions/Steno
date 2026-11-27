<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { api, onEvent } from '../../lib/api';
  import type { MeetingDetail, Segment } from '../../lib/types';
  import { offset, timeS } from '../../lib/format';
  import { setQuery } from '../../lib/router.svelte';
  import { toastError } from '../../lib/ui.svelte';
  import { initial, speakerColor, speakerName } from '../../lib/components/speakers';
  import Icon from '../../lib/components/Icon.svelte';

  let { detail }: { detail: MeetingDetail } = $props();
  const m = $derived(detail.meeting);

  let segments = $state<Segment[]>([]);
  let showPrivate = $state(false);
  let loaded = $state(false);
  let editing = $state<{ label: string; key: string } | null>(null);
  let editValue = $state('');

  async function load() {
    try {
      segments = await api.getTranscript(m.id, showPrivate);
      loaded = true;
    } catch (e) {
      toastError(e);
    }
  }

  $effect(() => {
    void showPrivate;
    load();
  });

  onMount(() =>
    onEvent('scribe://segment', (s) => {
      if (s.meeting_id !== m.id || (s.private && !showPrivate)) return;
      if (!segments.some((x) => x.id === s.id)) segments.push(s);
    }),
  );

  const visible = $derived(segments.filter((s) => !s.is_echo));
  const echoCount = $derived(segments.length - visible.length);
  const privateCount = $derived(segments.filter((s) => s.private).length);
  const labels = $derived([...new Set([...detail.speakers.map((s) => s.label), ...segments.map((s) => s.speaker)])]);

  async function startEdit(label: string, key: string) {
    editing = { label, key };
    editValue = detail.speakers.find((s) => s.label === label)?.display_name ?? '';
    await tick();
    document.getElementById('sp-edit')?.focus();
  }

  async function saveEdit() {
    if (!editing) return;
    const { label } = editing;
    const name = editValue.trim();
    editing = null;
    try {
      await api.renameSpeaker(m.id, label, name);
      const sp = detail.speakers.find((s) => s.label === label);
      if (sp) sp.display_name = name || null;
      else detail.speakers.push({ label, display_name: name || null });
    } catch (e) {
      toastError(e);
    }
  }

  function editKey(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      saveEdit();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      editing = null;
    }
  }
</script>

{#snippet speakerTag(label: string, key: string)}
  {#if editing?.key === key}
    <input
      id="sp-edit"
      class="sp-input"
      bind:value={editValue}
      onkeydown={editKey}
      onblur={saveEdit}
      placeholder={label}
      aria-label="Nom affiché pour {label}"
      style="--c: {speakerColor(label)}"
    />
  {:else}
    <button
      class="sp"
      style="--c: {speakerColor(label)}"
      onclick={() => startEdit(label, key)}
      title={label === 'Moi' ? 'Moi' : `${label} — cliquer pour renommer`}
    >
      {#if key.startsWith('bar-')}<span class="avatar mini" aria-hidden="true">{initial(speakerName(label, detail.speakers))}</span>{/if}
      {speakerName(label, detail.speakers)}
      {#if speakerName(label, detail.speakers) !== label}<span class="orig">{label}</span>{/if}
    </button>
  {/if}
{/snippet}

<div class="wrap">
  <div class="toolbar row wrap">
    <span class="label">Intervenants</span>
    {#each labels as l (l)}
      {@render speakerTag(l, `bar-${l}`)}
    {/each}
    <span class="grow"></span>
    <label class="check small priv-toggle" title="Les phrases dites en aparté ne sont jamais envoyées à l’IA">
      <input class="switch" type="checkbox" role="switch" bind:checked={showPrivate} /> Afficher les phrases privées{#if showPrivate && privateCount} ({privateCount}){/if}
    </label>
  </div>

  {#if !loaded}
    <div class="skel" aria-hidden="true">
      {#each [72, 54, 80, 40] as w, k (k)}<span class="skeleton" style="width: {w}%"></span>{/each}
    </div>
  {:else if !visible.length}
    <div class="empty"><Icon name="wave" size={22} stroke={1.5} /><p>Aucune phrase transcrite.</p></div>
  {/if}

  <ol class="segs">
    {#each visible as s, i (s.id)}
      {@const sameAsPrev = i > 0 && visible[i - 1].speaker === s.speaker && !visible[i - 1].private && !s.private}
      <li class="seg" class:cont={sameAsPrev} class:private={s.private} class:cmdseg={!!s.request_id} style="--c: {speakerColor(s.speaker)}">
        <time class="ts" datetime={s.ts} title="{offset(s.t0)} depuis le début">{timeS(s.ts)}</time>
        <span class="av">{#if !sameAsPrev}<span class="avatar" aria-hidden="true">{initial(speakerName(s.speaker, detail.speakers))}</span>{/if}</span>
        <div class="bc">
          {#if !sameAsPrev}<div class="who">{@render speakerTag(s.speaker, `seg-${s.id}`)}</div>{/if}
          <p class="text">
            {#if s.private}<span class="priv" title="Aparté : exclu du compte rendu"><Icon name="lock" size={11} /> Privé</span>{/if}
            {s.text}
            {#if s.request_id}
              <button class="cmd" onclick={() => setQuery({ tab: 'requests', request: s.request_id })} title="Voir la demande">
                <Icon name="ask" size={11} /> Demande
              </button>
            {/if}
          </p>
        </div>
      </li>
    {/each}
  </ol>

  {#if echoCount || (!showPrivate && loaded)}
    <p class="foot faint small"><Icon name="info" size={13} />
      {#if echoCount}{echoCount} {echoCount > 1 ? 'échos masqués' : 'écho masqué'} (votre micro a capté le son de l’appel).{/if}
      {#if !showPrivate} Les phrases privées sont masquées.{/if}
    </p>
  {/if}
</div>

<style>
  .wrap { padding: 0 32px 48px; max-width: 920px; }
  .toolbar {
    gap: 6px;
    padding: 14px 0 12px;
    margin-bottom: 6px;
    border-bottom: 1px solid var(--line);
    position: sticky;
    top: 0;
    background: var(--surface);
    z-index: 2;
  }
  .toolbar .label { margin-right: 6px; }
  .priv-toggle { gap: 8px; color: var(--text-2); }
  .sp {
    border: 0;
    background: transparent;
    color: var(--c);
    font: inherit;
    font-weight: 600;
    font-size: var(--fs-meta);
    height: 24px;
    padding: 0 8px;
    border-radius: var(--r-pill);
    display: inline-flex;
    align-items: center;
    gap: 6px;
    max-width: 100%;
    white-space: nowrap;
    transition: background var(--t-fast) var(--ease);
  }
  .toolbar .sp { background: color-mix(in srgb, var(--c) 10%, transparent); padding-left: 3px; }
  .sp:hover { background: color-mix(in srgb, var(--c) 16%, transparent); }
  .who .sp { margin-left: -8px; height: 20px; }
  .avatar.mini { width: 18px; height: 18px; font-size: 9.5px; --c: inherit; color: var(--c); }
  .orig { font-weight: 500; opacity: 0.65; font-size: var(--fs-micro); }
  .sp-input {
    height: 24px;
    width: 130px;
    padding: 0 8px;
    border: 1px solid var(--c);
    border-radius: 7px;
    font: inherit;
    font-size: var(--fs-meta);
    background: var(--surface);
    color: var(--text);
    outline: none;
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--c) 18%, transparent);
  }
  .segs { list-style: none; margin: 0; padding: 0; }
  .seg { display: grid; grid-template-columns: 58px 32px 1fr; column-gap: 10px; padding-top: 14px; align-items: start; }
  .seg.cont { padding-top: 0; }
  .ts { font-size: var(--fs-micro); color: var(--text-3); padding-top: 0; line-height: 24px; text-align: right; transition: opacity var(--t-fast) var(--ease); }
  .seg.cont .ts { opacity: 0; }
  .seg:hover .ts { opacity: 1; }
  .av { padding-top: 0; }
  .bc { min-width: 0; }
  .who { min-width: 0; line-height: 24px; display: flex; align-items: center; }
  .text { line-height: 1.55; font-size: var(--fs-body); padding: 1px 0; }
  .seg.private .text { color: var(--text-2); font-style: italic; }
  .seg.cmdseg .text { margin: 3px 0; padding: 6px 12px; border-radius: 0 9px 9px 0; background: var(--accent-soft); box-shadow: inset 2px 0 0 var(--accent); }
  .priv { display: inline-flex; align-items: center; gap: 3px; font-style: normal; font-size: var(--fs-micro); color: var(--text-3); margin-right: 4px; padding: 0 6px; border-radius: var(--r-pill); background: var(--surface-2); }
  .cmd {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-left: 8px;
    height: 20px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--r-pill);
    background: var(--accent-soft-2);
    color: var(--accent-text);
    font: inherit;
    font-size: var(--fs-micro);
    font-weight: 600;
    vertical-align: 1px;
    transition: background var(--t-fast) var(--ease);
  }
  .cmd:hover { background: var(--accent); color: var(--accent-ink); }
  .foot { margin-top: 24px; display: flex; align-items: center; gap: 6px; }
  .skel { padding: 24px 0 0 100px; }
</style>
