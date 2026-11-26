<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { api, onEvent } from '../../lib/api';
  import type { MeetingDetail, Segment } from '../../lib/types';
  import { offset, timeS } from '../../lib/format';
  import { setQuery } from '../../lib/router.svelte';
  import { toastError } from '../../lib/ui.svelte';
  import { speakerColor, speakerName } from '../../lib/components/speakers';
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
    <label class="check small" title="Les phrases dites en aparté ne sont jamais envoyées à l’IA">
      <input type="checkbox" bind:checked={showPrivate} /> Afficher les phrases privées{#if showPrivate && privateCount} ({privateCount}){/if}
    </label>
  </div>

  {#if loaded && !visible.length}
    <div class="empty"><p>Aucune phrase transcrite.</p></div>
  {/if}

  <ol class="segs">
    {#each visible as s, i (s.id)}
      {@const sameAsPrev = i > 0 && visible[i - 1].speaker === s.speaker && !visible[i - 1].private && !s.private}
      <li class="seg" class:cont={sameAsPrev} class:private={s.private}>
        <time class="ts" datetime={s.ts} title="{offset(s.t0)} depuis le début">{timeS(s.ts)}</time>
        <div class="who">
          {#if !sameAsPrev}{@render speakerTag(s.speaker, `seg-${s.id}`)}{/if}
        </div>
        <p class="text">
          {#if s.private}<span class="priv" title="Aparté : exclu du compte rendu"><Icon name="lock" size={11} /> Privé</span>{/if}
          {s.text}
          {#if s.request_id}
            <button class="cmd" onclick={() => setQuery({ tab: 'requests', request: s.request_id })} title="Voir la demande">
              <Icon name="ask" size={11} /> Demande
            </button>
          {/if}
        </p>
      </li>
    {/each}
  </ol>

  {#if echoCount || (!showPrivate && loaded)}
    <p class="foot faint small">
      {#if echoCount}{echoCount} {echoCount > 1 ? 'échos masqués' : 'écho masqué'} (votre micro a capté le son de Teams).{/if}
      {#if !showPrivate} Les phrases privées sont masquées.{/if}
    </p>
  {/if}
</div>

<style>
  .wrap { padding: 14px 28px 40px; max-width: 900px; }
  .toolbar { gap: 6px; padding-bottom: 12px; margin-bottom: 8px; border-bottom: 1px solid var(--line); position: sticky; top: 0; background: var(--surface); z-index: 1; }
  .toolbar .label { margin-right: 4px; }
  .sp {
    border: 0;
    background: color-mix(in srgb, var(--c) 12%, transparent);
    color: var(--c);
    font: inherit;
    font-weight: 600;
    font-size: 12px;
    height: 21px;
    padding: 0 8px;
    border-radius: 5px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    max-width: 100%;
    white-space: nowrap;
  }
  .sp:hover { background: color-mix(in srgb, var(--c) 20%, transparent); }
  .orig { font-weight: 500; opacity: 0.6; font-size: 11px; }
  .sp-input {
    height: 21px;
    width: 120px;
    padding: 0 6px;
    border: 1px solid var(--c);
    border-radius: 5px;
    font: inherit;
    font-size: 12px;
    background: var(--surface);
    color: var(--text);
    outline: none;
  }
  .segs { list-style: none; margin: 0; padding: 0; }
  .seg { display: grid; grid-template-columns: 62px 110px 1fr; gap: 10px; padding: 7px 0 2px; align-items: baseline; }
  .seg.cont { padding-top: 0; }
  .ts { font-size: 11px; color: var(--text-3); font-variant-numeric: tabular-nums; }
  .seg.cont .ts { visibility: hidden; }
  .seg:hover .ts { visibility: visible; }
  .who { min-width: 0; }
  .text { line-height: 1.55; font-size: 13.5px; }
  .seg.private .text { color: var(--text-2); font-style: italic; }
  .priv { display: inline-flex; align-items: center; gap: 3px; font-style: normal; font-size: 11px; color: var(--text-3); margin-right: 4px; }
  .cmd {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    margin-left: 6px;
    height: 18px;
    padding: 0 6px;
    border: 1px solid var(--line-strong);
    border-radius: 5px;
    background: transparent;
    color: var(--text-2);
    font: inherit;
    font-size: 11px;
    vertical-align: 1px;
  }
  .cmd:hover { color: var(--text); background: var(--hover); }
  .foot { margin-top: 18px; }
</style>
