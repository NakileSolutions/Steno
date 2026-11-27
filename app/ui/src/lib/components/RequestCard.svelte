<script lang="ts">
  import type { Request } from '../types';
  import { api, openExternal } from '../api';
  import { extractLinks, plainText, stripTldr } from '../markdown';
  import { INTENT_LABEL, ORIGIN_LABEL, STATUS_CLASS, STATUS_LABEL, shortDuration, time } from '../format';
  import { toastError } from '../ui.svelte';
  import Markdown from './Markdown.svelte';
  import Icon from './Icon.svelte';
  import { t } from '../i18n.svelte';

  let {
    req,
    compact = false,
    focused = false,
    showMeta = true,
  }: { req: Request; compact?: boolean; focused?: boolean; showMeta?: boolean } = $props();

  const INTENT_ICON: Record<string, string> = { search: 'search', check: 'check', note: 'note', recap: 'note', cancel: 'x', stop: 'stop', history: 'clock' };
  let open = $state(false);
  // Les sources sont listées à part seulement si la réponse ne les présente pas déjà.
  const sources = $derived(/(\*\*|#+\s*)Sources/i.test(req.answer_md ?? '') ? [] : extractLinks(req.answer_md));
  const pending = $derived(req.status === 'queued' || req.status === 'running');
  const took = $derived(req.status === 'done' ? shortDuration(req.started_at, req.finished_at) : null);
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

<article class="req" class:focused class:compact class:pending bind:this={el} aria-busy={pending}>
  <header class="row hd">
    <span class="iic {req.intent}" aria-hidden="true"><Icon name={INTENT_ICON[req.intent] ?? 'ask'} size={13} stroke={1.8} /></span>
    <span class="intent" title={t('Demande {origin}', { origin: ORIGIN_LABEL[req.origin] ? t(ORIGIN_LABEL[req.origin]) : req.origin })}>
      {INTENT_LABEL[req.intent] ? t(INTENT_LABEL[req.intent]) : req.intent}
      {#if req.origin === 'voice'}<span class="origin" title={t('Demande vocale')}><Icon name="mic" size={11} /></span>{/if}
    </span>
    <span class="grow"></span>
    {#if showMeta}<time class="faint tiny">{time(req.created_at)}</time>{/if}
    {#if took}<span class="faint tiny" title={t('Durée de la recherche')}>{t('en {took}', { took })}</span>{/if}
    <span class="badge status {STATUS_CLASS[req.status]}">
      {#if pending}<span class="spin" aria-hidden="true"></span>{/if}
      {STATUS_LABEL[req.status] ? t(STATUS_LABEL[req.status]) : req.status}
    </span>
    {#if pending}
      <button class="btn ghost sm icon" title={t('Annuler la demande')} aria-label={t('Annuler la demande')} onclick={cancel}><Icon name="x" size={13} /></button>
    {/if}
  </header>

  <p class="query">{req.query}</p>

  {#if req.error}
    <p class="error small"><Icon name="warn" size={13} />{req.error}</p>
  {/if}

  {#if req.answer_md}
    {#if compact && !open}
      <button class="summary" onclick={() => (open = true)} aria-expanded="false">
        {plainText(req.summary) || t('Afficher la réponse')}
        <span class="more">{t('Afficher la réponse')} <Icon name="chevron" size={11} stroke={2} /></span>
      </button>
    {/if}
    {#if showAnswer}
      <div class="answer"><Markdown source={stripTldr(req.answer_md)} compact={compact} /></div>
    {/if}
  {:else if pending}
    <div class="thinking">
      <span class="skeleton" style="width: 82%"></span>
      <span class="skeleton" style="width: 56%"></span>
      <p class="faint small">{req.status === 'queued' ? t('En attente d’un créneau pour l’agent…') : t('L’agent cherche…')}</p>
    </div>
  {/if}

  {#if showAnswer && (sources.length || req.tools_used.length)}
    <footer>
      {#if sources.length}
        <div class="row wrap chips">
          <span class="label">{t('Sources')}</span>
          {#each sources as s (s.url)}
            <button class="chip" title={s.url} onclick={() => openExternal(s.url)}><Icon name="link" size={11} />{s.text}</button>
          {/each}
        </div>
      {/if}
      {#if req.tools_used.length}
        <div class="row wrap chips">
          <span class="label">{t('Outils')}</span>
          {#each req.tools_used as tool}<code class="tool">{tool}</code>{/each}
        </div>
      {/if}
    </footer>
  {/if}
</article>

<style>
  .req {
    padding: 14px 16px;
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    background: var(--surface);
    box-shadow: var(--shadow-sm);
    display: flex;
    flex-direction: column;
    gap: 8px;
    animation: appear var(--t) var(--ease) both;
    transition: border-color 0.2s, box-shadow 0.2s;
  }
  .req.compact { padding: 12px 14px; gap: 6px; }
  .req.pending { border-color: var(--accent-line); }
  .req.focused { border-color: var(--focus); box-shadow: 0 0 0 3px color-mix(in srgb, var(--focus) 18%, transparent), var(--shadow-md); }
  .hd { gap: 8px; }
  .iic {
    width: 24px;
    height: 24px;
    border-radius: 7px;
    display: grid;
    place-items: center;
    flex: none;
    background: var(--accent-soft);
    color: var(--accent-text);
  }
  .iic.note, .iic.recap { background: var(--ok-soft); color: var(--ok); }
  .iic.cancel, .iic.stop { background: var(--surface-2); color: var(--text-2); }
  .iic.check { background: var(--warn-soft); color: var(--warn); }
  .intent {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-micro);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-2);
  }
  .origin { display: inline-grid; place-items: center; width: 18px; height: 18px; border-radius: 50%; background: var(--surface-2); color: var(--text-3); }
  .query { font-weight: 600; font-size: 14px; line-height: 1.4; letter-spacing: -0.005em; }
  .compact .query { font-size: 13.5px; }
  .error { color: var(--err); display: flex; gap: 6px; align-items: flex-start; }
  .error :global(.icon) { margin-top: 2px; }
  .summary {
    all: unset;
    display: block;
    color: var(--text-2);
    font-size: 13px;
    line-height: 1.5;
    cursor: default;
    border-radius: 6px;
  }
  .summary:focus-visible { outline: 2px solid var(--focus); outline-offset: 2px; }
  .more { display: inline-flex; align-items: center; gap: 3px; margin-left: 4px; color: var(--accent-text); font-size: 12px; font-weight: 500; vertical-align: 0; }
  .summary:hover .more { text-decoration: underline; text-underline-offset: 2px; }
  .more :global(.icon) { transform: rotate(90deg); }
  .answer { padding-top: 2px; }
  /* Dans une carte, les intertitres de la réponse restent discrets. */
  .answer :global(.md h2) { font-size: var(--fs-micro); text-transform: uppercase; letter-spacing: 0.07em; color: var(--text-3); margin: 16px 0 4px; }
  .answer :global(.md h2::before) { display: none; }
  .answer :global(.md > h2:first-child) { margin-top: 4px; }
  .thinking { display: flex; flex-direction: column; gap: 0; padding: 4px 0 2px; }
  .thinking p { margin-top: 10px; }
  footer { display: flex; flex-direction: column; gap: 8px; padding-top: 10px; border-top: 1px solid var(--line); margin-top: 4px; }
  .chips { gap: 6px; }
  .chips .label { margin-right: 4px; min-width: 52px; }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    max-width: 280px;
    height: 24px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--r-pill);
    background: var(--accent-soft);
    color: var(--accent-text);
    font: inherit;
    font-size: 12px;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    transition: background var(--t-fast) var(--ease);
  }
  .chip:hover { background: var(--accent-soft-2); }
  .tool { font-size: 11px; color: var(--text-2); padding: 2px 7px; border-radius: 6px; background: var(--surface-2); box-shadow: inset 0 0 0 1px var(--line); }
  .spin {
    width: 9px;
    height: 9px;
    border: 1.5px solid currentColor;
    border-right-color: transparent;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  .badge.status:has(.spin)::before { display: none; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
