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

describe('changements importants', () => {
  it('isole la section ⚠️ Changements importants', () => {
    const p = splitReport('# T\n\n## En bref\nok\n\n## ⚠️ Changements importants\n- **TVA belge** : 19 % → 21 %\n\n## Décisions\n- x');
    expect(p.changes?.body).toBe('- **TVA belge** : 19 % → 21 %');
    expect(p.rest).not.toContain('Changements');
  });
});

describe('titres anglais', () => {
  const EN = `# Daily — 26/11/2026

## Summary
Team daily: slot lock.

## ⚠️ Important changes
- **Belgian VAT**: 19% → 21%

## Decisions
- keep the slot lock

## Action items — who / what / due
- **Marie** — Fix APP-1234 — Friday

## Open questions
- Export format`;

  it('reconnaît Summary, Important changes, Decisions et Action items', () => {
    const p = splitReport(EN);
    expect(p.brief).toEqual({ heading: 'Summary', body: 'Team daily: slot lock.' });
    expect(p.changes?.body).toBe('- **Belgian VAT**: 19% → 21%');
    expect(p.decisions?.body).toBe('- keep the slot lock');
    expect(p.actions?.heading).toBe('Action items — who / what / due');
    expect(p.rest).toBe('## Open questions\n- Export format');
  });

  it('TL;DR, Decision, Actions, Next steps', () => {
    expect(splitReport('## TL;DR\nok').brief?.body).toBe('ok');
    expect(splitReport('## Decision\n- a').decisions?.body).toBe('- a');
    expect(splitReport('## Actions\n- b').actions?.body).toBe('- b');
    expect(splitReport('## Next steps\n- c').actions?.body).toBe('- c');
    expect(splitReport('## Key changes\n- d').changes?.body).toBe('- d');
  });

  it('un titre anglais inconnu reste dans le document', () => {
    const p = splitReport('## Context\n- x');
    expect(p.brief).toBeNull();
    expect(p.rest).toBe('## Context\n- x');
  });
});
