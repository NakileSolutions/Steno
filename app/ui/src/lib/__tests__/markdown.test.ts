import { describe, expect, it } from 'vitest';
import { extractLinks, plainText, renderMarkdown, stripTldr } from '../markdown';

function dom(md: string): HTMLElement {
  const el = document.createElement('div');
  el.innerHTML = renderMarkdown(md);
  return el;
}

describe('renderMarkdown', () => {
  it('rend le markdown courant (GFM)', () => {
    const el = dom('# Titre\n\n- **gras** et `code`\n\n| a | b |\n|---|---|\n| 1 | 2 |');
    expect(el.querySelector('h1')?.textContent).toBe('Titre');
    expect(el.querySelector('strong')?.textContent).toBe('gras');
    expect(el.querySelector('code')?.textContent).toBe('code');
    expect(el.querySelectorAll('td')).toHaveLength(2);
  });

  it('removes interactive elements and id / name attributes', () => {
    const el = dom('<dialog open>bloque les raccourcis</dialog><details><summary>s</summary>d</details>\n\n<button>b</button><select><option>o</option></select><textarea>t</textarea>\n\n<p id="app" name="n">texte</p>');
    for (const tag of ['dialog', 'details', 'summary', 'button', 'select', 'textarea']) expect(el.querySelector(tag)).toBeNull();
    expect(el.querySelector('[id], [name]')).toBeNull();
    expect(el.textContent).toContain('texte');
  });

  it('retire les balises <script>', () => {
    const html = renderMarkdown('Avant\n\n<script>alert(1)</script>\n\nAprès');
    expect(html).not.toMatch(/<script/i);
    expect(html).not.toContain('alert(1)');
    expect(html).toContain('Après');
  });

  it('retire les gestionnaires d’événements (onerror, onclick…)', () => {
    const html = renderMarkdown('<p onclick="alert(1)">x</p><img src=x onerror="alert(2)"><a href="https://ok.example" onmouseover="alert(3)">lien</a>');
    expect(html).not.toMatch(/on(error|click|mouseover)/i);
    expect(html).not.toMatch(/<img/i);
    expect(html).toContain('https://ok.example');
  });

  it('retire les liens javascript: et data:, en gardant le texte', () => {
    const el = dom('[piège](javascript:alert(1)) <a href="JaVaScRiPt:alert(2)">bis</a> [d](data:text/html;base64,PHNjcmlwdD4=)');
    const links = [...el.querySelectorAll('a')];
    expect(links.map((a) => a.textContent)).toEqual(['piège', 'bis', 'd']);
    for (const a of links) expect(a.getAttribute('href')).toBeNull();
    expect(el.innerHTML).not.toMatch(/javascript:/i);
  });

  it('garde les liens http(s), mailto et ancres, sans target et avec rel', () => {
    const el = dom('[a](https://jira.example.com/browse/APP-1) [b](mailto:x@example.com) [c](#sec) <a href="http://x.example" target="_blank">d</a>');
    const links = [...el.querySelectorAll('a')];
    expect(links.map((a) => a.getAttribute('href'))).toEqual(['https://jira.example.com/browse/APP-1', 'mailto:x@example.com', '#sec', 'http://x.example']);
    for (const a of links) {
      expect(a.getAttribute('rel')).toBe('noopener noreferrer');
      expect(a.hasAttribute('target')).toBe(false);
    }
  });

  it('interdit iframes, styles et formulaires', () => {
    const html = renderMarkdown('<iframe src="https://x"></iframe><style>body{}</style><form><input></form><span style="color:red">t</span>');
    expect(html).not.toMatch(/<(iframe|style|form|input)/i);
    expect(html).not.toMatch(/style=/i);
    expect(html).toContain('t');
  });

  it('tolère une entrée vide', () => {
    expect(renderMarkdown('')).toBe('');
  });
});

describe('extractLinks', () => {
  it('extrait les liens http(s) uniques, sans mise en forme', () => {
    const md = '- [`StationClient.java:48`](https://gitlab.example.com/a#L48)\n- [**APP-1251**](https://jira.example.com/browse/APP-1251)\n- [doublon](https://gitlab.example.com/a#L48)\n- [js](javascript:x)';
    expect(extractLinks(md)).toEqual([
      { text: 'StationClient.java:48', url: 'https://gitlab.example.com/a#L48' },
      { text: 'APP-1251', url: 'https://jira.example.com/browse/APP-1251' },
    ]);
  });
  it('renvoie une liste vide sans markdown', () => {
    expect(extractLinks(null)).toEqual([]);
  });
});

describe('résumés', () => {
  it('stripTldr retire le préfixe en tête seulement', () => {
    expect(stripTldr('TL;DR: Le verrou est dans `SlotLock.kt:42`.\n\n## Code')).toBe('Le verrou est dans `SlotLock.kt:42`.\n\n## Code');
    expect(stripTldr('**TL;DR** : court')).toBe('court');
    expect(stripTldr('Rien à retirer (TL;DR: plus loin)')).toBe('Rien à retirer (TL;DR: plus loin)');
  });
  it('plainText enlève code, gras et liens', () => {
    expect(plainText('TL;DR: Le verrou de créneau est géré dans `src/SlotLock.kt:42` ; voir **[APP-1234](https://jira.example.com/browse/APP-1234)**.'))
      .toBe('Le verrou de créneau est géré dans src/SlotLock.kt:42 ; voir APP-1234.');
    expect(plainText(null)).toBe('');
  });
});
