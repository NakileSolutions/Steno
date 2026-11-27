import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { i18n, setLang, t, tn } from '../i18n.svelte';
import en from '../i18n/en/index';

const SRC = join(__dirname, '..', '..');

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) return f === '__tests__' || f === 'i18n' ? [] : files(p);
    return /\.(svelte|ts)$/.test(f) && !/mock|i18n\.svelte/.test(f) ? [p] : [];
  });
}

/** Clés littérales de `t('…')` / `tn(n, '…', '…')` dans le code. */
function keys(): Map<string, string> {
  const out = new Map<string, string>();
  const lit = String.raw`'((?:[^'\\]|\\.)*)'|"((?:[^"\\]|\\.)*)"`;
  const re = new RegExp(String.raw`\bt\(\s*(?:${lit})`, 'g');
  const reN = new RegExp(String.raw`\btn\([^,]+,\s*(?:${lit})\s*,\s*(?:${lit})`, 'g');
  for (const f of files(SRC)) {
    const s = readFileSync(f, 'utf8');
    for (const m of s.matchAll(re)) out.set((m[1] ?? m[2]).replace(/\\(.)/g, '$1'), f);
    for (const m of s.matchAll(reN)) {
      out.set((m[1] ?? m[2]).replace(/\\(.)/g, '$1'), f);
      out.set((m[3] ?? m[4]).replace(/\\(.)/g, '$1'), f);
    }
  }
  return out;
}

afterEach(() => setLang('fr'));

describe('i18n', () => {
  it('traduit, garde le français en repli et remplit les variables', () => {
    setLang('fr');
    expect(t('Sténo {v} est disponible', { v: '0.2.0' })).toBe('Sténo 0.2.0 est disponible');
    setLang('en');
    expect(i18n.lang).toBe('en');
    expect(t('Texte sans traduction')).toBe('Texte sans traduction');
    expect(tn(2, '{n} demande', '{n} demandes')).toBe(en['{n} demandes']?.replace('{n}', '2') ?? '2 demandes');
  });

  it('chaque texte de l’interface a sa traduction anglaise', () => {
    const missing = [...keys()].filter(([k]) => !(k in en)).map(([k, f]) => `${f.replace(SRC, '')} : ${k}`);
    expect(missing).toEqual([]);
  });

  it('les traductions gardent les mêmes variables', () => {
    const vars = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(',');
    const bad = Object.entries(en).filter(([fr, e]) => vars(fr) !== vars(e)).map(([fr]) => fr);
    expect(bad).toEqual([]);
  });
});
