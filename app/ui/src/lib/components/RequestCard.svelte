<script lang="ts">
  import type { Request } from '../types';
  import { api, openExternal } from '../api';
  import { extractLinks } from '../markdown';
  import { INTENT_LABEL, ORIGIN_LABEL, STATUS_CLASS, STATUS_LABEL, time } from '../format';
  import { toastError } from '../ui.svelte';
  import Markdown from './Markdown.svelte';
  import Icon from './Icon.svelte';

  let {
    req,
    compact = false,
    focused = false,
    showMeta = true,
  }: { req: Request; compact?: boolean; focused?: boolean; showMeta?: boolean } = $props();

  let open = $state(false);
  // Les sources sont listées à part seulement si la réponse ne les présente pas déjà.
  const sources = $derived(/(\*\*|#+\s*)Sources/i.test(req.answer_md ?? '') ? [] : extractLinks(req.answer_md));
  const pending = $derived(req.status === 'queued' || req.status === 'running');
  const showAnswer = $derived(!compact || open);
  let el: HTMLElement | undefined = $state();

  $effect(() => {
    if (focused && el) el.scrollIntoView({ block: 'center', behavior: 'smooth' });
  });

  async function cancel() {
    try {
      await api.cancelRequest(req.id);
    } catch (e) {
      toastError(e);
    }
  }
</script>

<article class="req" class:focused class:compact bind:this={el} aria-busy={pending}>
  <header class="row">
    <span class="intent" title="Demande {ORIGIN_LABEL[req.origin] ?? req.origin}">
      {#if req.origin === 'voice'}<Icon name="mic" size={13} />{:else}<Icon name="ask" size={13} />{/if}
      {INTENT_LABEL[req.intent] ?? req.intent}
    </span>
    <span class="grow"></span>
    {#if showMeta}<span class="faint tiny">{time(req.created_at)}</span>{/if}
    <span class="badge {STATUS_CLASS[req.status]}">
      {#if pending}<span class="spin" aria-hidden="true"></span>{/if}
      {STATUS_LABEL[req.status] ?? req.status}
    </span>
    {#if pending}
      <button class="btn ghost sm icon" title="Annuler la demande" aria-label="Annuler la demande" onclick={cancel}><Icon name="x" size={13} /></button>
    {/if}
  </header>

  <p class="query">{req.query}</p>

  {#if req.error}
    <p class="error small">{req.error}</p>
  {/if}

  {#if req.answer_md}
    {#if compact && !open}
      <button class="summary" onclick={() => (open = true)} aria-expanded="false">
        {req.summary ?? 'Afficher la réponse'}
        <span class="more">Afficher la réponse</span>
      </button>
    {/if}
    {#if showAnswer}
      <div class="answer"><Markdown source={req.answer_md} compact={compact} /></div>
    {/if}
  {:else if pending}
    <p class="faint small">{req.status === 'queued' ? 'En attente d’un créneau pour l’agent…' : 'L’agent cherche…'}</p>
  {/if}

  {#if showAnswer && (sources.length || req.tools_used.length)}
    <footer>
      {#if sources.length}
        <div class="row wrap chips">
          <span class="label">Sources</span>
          {#each sources as s (s.url)}
            <button class="chip" title={s.url} onclick={() => openExternal(s.url)}><Icon name="external" size={11} />{s.text}</button>
          {/each}
        </div>
      {/if}
      {#if req.tools_used.length}
        <div class="row wrap chips">
          <span class="label">Outils</span>
          {#each req.tools_used as t}<code class="tool">{t}</code>{/each}
        </div>
      {/if}
    </footer>
  {/if}
</article>

<style>
  .req {
    padding: 12px 14px;
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    background: var(--surface);
    display: flex;
    flex-direction: column;
    gap: 6px;
    transition: border-color 0.2s, box-shadow 0.2s;
  }
  .req.compact { padding: 10px 12px; }
  .req.focused { border-color: var(--focus); box-shadow: 0 0 0 3px color-mix(in srgb, var(--focus) 18%, transparent); }
  .intent {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-3);
  }
  .query { font-weight: 600; font-size: 13.5px; line-height: 1.4; }
  .error { color: var(--err); }
  .summary {
    all: unset;
    display: block;
    color: var(--text-2);
    font-size: 12.5px;
    line-height: 1.45;
    cursor: default;
  }
  .summary:focus-visible { outline: 2px solid var(--focus); outline-offset: 2px; border-radius: 4px; }
  .more { display: block; margin-top: 3px; color: var(--text-3); font-size: 11.5px; text-decoration: underline; text-underline-offset: 2px; }
  .answer { padding-top: 2px; }
  footer { display: flex; flex-direction: column; gap: 6px; padding-top: 8px; border-top: 1px solid var(--line); margin-top: 4px; }
  .chips { gap: 5px; }
  .chips .label { margin-right: 4px; }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    max-width: 280px;
    height: 20px;
    padding: 0 7px;
    border: 1px solid var(--line-strong);
    border-radius: 5px;
    background: transparent;
    color: var(--text-2);
    font: inherit;
    font-size: 11.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chip:hover { color: var(--text); background: var(--hover); }
  .tool { font-size: 11px; color: var(--text-2); padding: 1px 5px; border-radius: 4px; background: var(--surface-2); }
  .spin {
    width: 9px;
    height: 9px;
    border: 1.5px solid currentColor;
    border-right-color: transparent;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
