<script lang="ts">
  import { renderMarkdown } from '../markdown';
  import { openExternal } from '../api';

  let { source, compact = false }: { source: string; compact?: boolean } = $props();
  const html = $derived(renderMarkdown(source));

  function onclick(e: MouseEvent) {
    const a = (e.target as HTMLElement).closest('a');
    if (!a) return;
    e.preventDefault();
    const url = a.getAttribute('href');
    if (url && /^https?:/i.test(url)) openExternal(url);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="md" class:compact {onclick}>
  {@html html}
</div>

<style>
  .md {
    font-size: 13.5px;
    line-height: 1.6;
    color: var(--text);
    overflow-wrap: anywhere;
  }
  .md.compact { font-size: 13px; line-height: 1.5; }
  .md :global(h1) { font-size: 20px; margin: 0 0 14px; }
  .md :global(h2) {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-3);
    margin: 22px 0 6px;
  }
  .md.compact :global(h2) { margin-top: 12px; }
  .md :global(h3) { font-size: 13.5px; margin: 14px 0 4px; }
  .md :global(p) { margin: 0 0 8px; }
  .md :global(ul), .md :global(ol) { margin: 0 0 8px; padding-left: 20px; }
  .md :global(li) { margin: 2px 0; }
  .md :global(li::marker) { color: var(--text-3); }
  .md :global(a) {
    color: inherit;
    text-decoration: underline;
    text-decoration-color: var(--line-strong);
    text-underline-offset: 2px;
    cursor: pointer;
  }
  .md :global(a:hover) { text-decoration-color: currentColor; }
  .md :global(code) {
    padding: 1px 4px;
    border-radius: 4px;
    background: var(--surface-2);
    font-size: 12px;
    white-space: nowrap;
  }
  .md :global(pre) {
    padding: 10px 12px;
    border-radius: var(--radius);
    background: var(--surface-2);
    overflow-x: auto;
  }
  .md :global(pre code) { padding: 0; background: none; white-space: pre; }
  .md :global(table) {
    width: 100%;
    border-collapse: collapse;
    margin: 4px 0 10px;
    font-size: 13px;
  }
  .md :global(th) {
    text-align: left;
    font-weight: 500;
    color: var(--text-3);
    font-size: 12px;
    border-bottom: 1px solid var(--line-strong);
    padding: 4px 10px 4px 0;
  }
  .md :global(td) { border-bottom: 1px solid var(--line); padding: 5px 10px 5px 0; vertical-align: top; }
  .md :global(blockquote) { margin: 0 0 8px; padding-left: 12px; border-left: 2px solid var(--line-strong); color: var(--text-2); }
  .md :global(hr) { border: 0; border-top: 1px solid var(--line); margin: 14px 0; }
  .md :global(> :last-child) { margin-bottom: 0; }
</style>
