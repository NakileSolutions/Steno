import { describe, expect, it } from 'vitest';
import { isEmptyBody, splitReport } from '../report';

const MD = `# Daily — 26/11/2026

## En bref
Daily d'équipe : verrou de créneau.

## Décisions
- garder le verrou de créneau

## Actions — qui / quoi / échéance
- **Marie** — Corriger le APP-1234 — vendredi

## Questions ouvertes
- Format de l'export

\`\`\`md
## Pas une section
\`\`\`

## Liens avec le code et les outils
- APP-1234`;

describe('splitReport', () => {
  it('isole En bref, Décisions et Actions ; le reste garde son ordre', () => {
    const p = splitReport(MD);
    expect(p.brief).toEqual({ heading: 'En bref', body: "Daily d'équipe : verrou de créneau." });
    expect(p.decisions?.body).toBe('- garder le verrou de créneau');
    expect(p.actions?.heading).toBe('Actions — qui / quoi / échéance');
    expect(p.rest.startsWith('## Questions ouvertes')).toBe(true);
    expect(p.rest).toContain('## Pas une section');
    expect(p.rest.indexOf('Questions ouvertes')).toBeLessThan(p.rest.indexOf('Liens avec le code'));
    expect(p.rest).not.toContain('# Daily');
    expect(p.rest).not.toContain('En bref');
  });

  it('sans sections connues : tout reste dans le document', () => {
    const p = splitReport('Un texte libre.\n\n## Autre\n- x');
    expect(p.brief).toBeNull();
    expect(p.decisions).toBeNull();
    expect(p.rest).toBe('Un texte libre.\n\n## Autre\n- x');
  });

  it('accents et pluriels : « Decision », « Action »', () => {
    const p = splitReport('## Decision\n- a\n## Action\n- b');
    expect(p.decisions?.body).toBe('- a');
    expect(p.actions?.body).toBe('- b');
  });

  it('corps vides', () => {
    expect(isEmptyBody('')).toBe(true);
    expect(isEmptyBody('- \n- —')).toBe(true);
    expect(isEmptyBody('- garder le verrou')).toBe(false);
  });
});
