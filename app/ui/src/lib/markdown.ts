// Rendu du markdown produit par l'IA : marked puis DOMPurify, toujours.

import { marked } from 'marked';
import DOMPurify from 'dompurify';

marked.setOptions({ gfm: true, breaks: false });

DOMPurify.addHook('afterSanitizeAttributes', (node) => {
  if (node.tagName === 'A') {
    const href = node.getAttribute('href') ?? '';
    if (!/^(https?:|mailto:|#)/i.test(href)) node.removeAttribute('href');
    node.setAttribute('rel', 'noopener noreferrer');
    node.removeAttribute('target');
  }
});

export function renderMarkdown(md: string): string {
  const html = marked.parse(md ?? '', { async: false }) as string;
  return DOMPurify.sanitize(html, {
    USE_PROFILES: { html: true },
    FORBID_TAGS: ['style', 'img', 'iframe', 'form', 'input', 'video', 'audio', 'svg', 'math'],
    FORBID_ATTR: ['style', 'srcset'],
  });
}

/** Extrait les liens http(s) d'un markdown (sources d'une réponse). */
export function extractLinks(md: string | null): { text: string; url: string }[] {
  if (!md) return [];
  const out: { text: string; url: string }[] = [];
  const seen = new Set<string>();
  for (const m of md.matchAll(/\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g)) {
    if (seen.has(m[2])) continue;
    seen.add(m[2]);
    out.push({ text: m[1].replace(/[`*_]/g, ''), url: m[2] });
  }
  return out;
}
