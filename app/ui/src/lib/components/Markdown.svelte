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
    font-size: var(--fs-body);
    line-height: 1.6;
    color: var(--text);
    overflow-wrap: anywhere;
  }
  .md.compact { font-size: 13px; line-height: 1.55; }
  .md :global(h1) { font: 600 var(--fs-h1) / 1.25 var(--font-display); letter-spacing: -0.02em; margin: 0 0 16px; }
  .md :global(h2) {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--fs-h3);
    font-weight: 600;
    letter-spacing: -0.01em;
    color: var(--text);
    margin: 30px 0 8px;
  }
  .md :global(h2::before) {
    content: '';
    width: 3px;
    height: 14px;
    border-radius: 2px;
    background: var(--accent);
    opacity: 0.75;
    flex: none;
  }
  .md :global(> h2:first-child) { margin-top: 4px; }
  .md.compact :global(h2) {
    font-size: var(--fs-micro);
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--text-3);
    margin: 14px 0 4px;
  }
  .md.compact :global(h2::before) { display: none; }
  .md :global(h3) { font-size: 14px; font-weight: 600; margin: 18px 0 6px; }
  .md :global(p) { margin: 0 0 10px; }
  .md :global(ul), .md :global(ol) { margin: 0 0 12px; padding-left: 22px; }
  .md.compact :global(ul), .md.compact :global(ol) { margin-bottom: 8px; padding-left: 18px; }
  .md :global(li) { margin: 5px 0; padding-left: 2px; }
  .md.compact :global(li) { margin: 2px 0; }
  .md :global(li::marker) { color: color-mix(in srgb, var(--accent) 55%, var(--text-3)); }
  .md :global(strong) { font-weight: 600; }
  .md :global(a) {
    color: var(--accent-text);
    text-decoration: underline;
    text-decoration-color: color-mix(in srgb, var(--accent-text) 35%, transparent);
    text-underline-offset: 2.5px;
    cursor: pointer;
    transition: text-decoration-color var(--t-fast) var(--ease);
  }
  .md :global(a:hover) { text-decoration-color: currentColor; }
  .md :global(code) {
    padding: 1px 6px;
    border-radius: 6px;
    background: var(--surface-2);
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--text);
    font-size: 0.86em;
    white-space: nowrap;
  }
  .md :global(pre) {
    padding: 12px 14px;
    border-radius: var(--radius);
    background: var(--surface-2);
    border: 1px solid var(--line);
    overflow-x: auto;
    margin: 0 0 12px;
  }
  .md :global(pre code) { padding: 0; background: none; box-shadow: none; white-space: pre; }
  .md :global(table) {
    width: 100%;
    border-collapse: separate;
    border-spacing: 0;
    margin: 6px 0 14px;
    font-size: 13.5px;
    border: 1px solid var(--line);
    border-radius: 10px;
    overflow: hidden;
  }
  .md :global(th) {
    text-align: left;
    font-weight: 600;
    color: var(--text-3);
    font-size: var(--fs-micro);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    background: var(--surface-2);
    border-bottom: 1px solid var(--line);
    padding: 7px 12px;
  }
  .md :global(td) { border-bottom: 1px solid var(--line); padding: 8px 12px; vertical-align: top; }
  .md :global(tr:last-child td) { border-bottom: 0; }
  .md :global(blockquote) { margin: 0 0 12px; padding: 8px 14px; border-left: 3px solid var(--accent-line); background: var(--accent-soft); border-radius: 0 8px 8px 0; color: var(--text-2); }
  .md :global(blockquote p:last-child) { margin: 0; }
  .md :global(hr) { border: 0; border-top: 1px solid var(--line); margin: 20px 0; }
  .md :global(> :last-child) { margin-bottom: 0; }
</style>
