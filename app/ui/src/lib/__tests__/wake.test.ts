import { describe, expect, it } from 'vitest';
import { detectWake, fold } from '../wake';
import { defaultSettings } from '../mock-data';
import type { Settings } from '../types';

const voice = (patch: Partial<Settings['voice']> = {}): Settings['voice'] => ({ ...defaultSettings().voice, ...patch });

describe('fold', () => {
  it('retire accents, majuscules et apostrophes typographiques', () => {
    expect(fold('Arrête l’Écoute')).toBe("arrete l'ecoute");
  });
});

describe('detectWake', () => {
  it('détecte une recherche et extrait la requête', () => {
    expect(detectWake('Sténo, cherche les erreurs Datadog sur ms-booking depuis hier.', voice())).toEqual({
      detected: true, intent: 'search', query: 'les erreurs Datadog sur ms-booking depuis hier',
    });
  });

  it('reconnaît l’arrêt de l’écoute, avec ou sans accents ni apostrophe typographique', () => {
    for (const t of ["Sténo, arrête l'écoute.", 'Sténo arrête l’écoute', 'steno, ARRETE L’ECOUTE !', 'Sténo, termine la réunion.', 'Sténo, fin de la réunion']) {
      expect(detectWake(t, voice()).intent, t).toBe('stop');
    }
    expect(detectWake("Sténo, arrête l'écoute.", voice()).query).toBeNull();
  });

  it('préfère le verbe le plus long (« ajoute une action » plutôt qu’une recherche)', () => {
    expect(detectWake('Sténo, ajoute une action relancer Marie', voice())).toEqual({ detected: true, intent: 'note', query: 'relancer Marie' });
  });

  it('accepte le mot d’appel en deuxième position et les variantes', () => {
    expect(detectWake('Ok Sténo, résume', voice()).intent).toBe('recap');
    expect(detectWake('Skribe, note : vérifier le quota', voice()).intent).toBe('note');
  });

  it('ignore le mot d’appel au-delà des deux premiers mots et les mots pièges', () => {
    expect(detectWake("Au fait, l'outil Sténo dont tu parlais", voice()).detected).toBe(false);
    expect(detectWake('Le script de migration plante', voice()).detected).toBe(false);
    expect(detectWake("Thomas s'est inscrit à la revue", voice()).detected).toBe(false);
  });

  it('exige un préfixe en mode strict', () => {
    const v = voice({ strict: true });
    expect(detectWake('Sténo, cherche APP-1234', v).detected).toBe(false);
    expect(detectWake('Dis Sténo, cherche APP-1234', v)).toEqual({ detected: true, intent: 'search', query: 'APP-1234' });
  });

  it('sans verbe connu : recherche avec toute la phrase', () => {
    expect(detectWake('Sténo, le runbook de ms-pricing', voice())).toEqual({ detected: true, intent: 'search', query: 'le runbook de ms-pricing' });
    expect(detectWake('Sténo.', voice())).toEqual({ detected: true, intent: 'search', query: null });
  });
});
