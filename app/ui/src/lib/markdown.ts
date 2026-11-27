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

const TLDR = /^\s*(\*\*|__)?\s*TL;?DR\s*:?\s*(\*\*|__)?\s*:?\s*/i;

/** Retire le préfixe « TL;DR: » qu'ajoutent certains agents en tête de réponse. */
export function stripTldr(md: string): string {
  return md.replace(TLDR, '');
}

/** Résumé en texte brut (une ligne) : sans « TL;DR: », ni code, gras ou liens markdown. */
export function plainText(md: string | null | undefined): string {
  if (!md) return '';
  return stripTldr(md)
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/`([^`]*)`/g, '$1')
    .replace(/(\*\*|__)(.+?)\1/g, '$2')
    .replace(/^#+\s*/gm, '')
    .replace(/\s+/g, ' ')
    .trim();
}
