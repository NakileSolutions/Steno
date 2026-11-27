import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  cap, clockDuration, dayLabel, debounce, duration, fullDate, highlight, INTENT_LABEL, isoDate,
  offset, relDay, shortDate, STATUS_LABEL, time, timeS,
} from '../format';

// Jeudi 26 novembre 2026, 15:00 à Paris (TZ fixé dans vitest.config.ts).
const NOW = new Date('2026-11-26T15:00:00+01:00');

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(NOW);
});
afterEach(() => vi.useRealTimers());

describe('dates', () => {
  it('formate heures et dates en français', () => {
    expect(time('2026-11-26T10:30:06+01:00')).toBe('10:30');
    expect(timeS('2026-11-26T10:30:06+01:00')).toBe('10:30:06');
    expect(shortDate('2026-11-26T10:30:06+01:00')).toBe('26 nov.');
    expect(fullDate('2026-11-26T10:30:06+01:00')).toBe('Jeudi 26 novembre 2026');
  });

  it('rend telles quelles les valeurs qui ne sont pas des dates', () => {
    expect(time('vendredi')).toBe('vendredi');
    expect(timeS('fin de semaine')).toBe('fin de semaine');
    expect(shortDate('vendredi')).toBe('vendredi');
    expect(fullDate('vendredi')).toBe('Vendredi');
    expect(dayLabel('vendredi')).toBe('vendredi');
    expect(relDay('la semaine prochaine')).toBe('la semaine prochaine');
  });

  it('accepte les valeurs vides', () => {
    expect(time(null)).toBe('');
    expect(time(undefined)).toBe('');
    expect(timeS('')).toBe('');
  });

  it('nomme les jours relatifs', () => {
    expect(dayLabel('2026-11-26T09:00:00+01:00')).toBe("Aujourd'hui");
    expect(dayLabel('2026-11-25T23:59:00+01:00')).toBe('Hier');
    expect(dayLabel('2026-11-23T09:00:00+01:00')).toBe('Lundi 23 novembre');
    expect(relDay('2026-11-26T09:05:00+01:00')).toBe('09:05');
    expect(relDay('2026-11-25T09:05:00+01:00')).toBe('Hier');
    expect(relDay('2026-11-23T09:05:00+01:00')).toBe('Lun');
    expect(relDay('2026-11-01T09:05:00+01:00')).toBe('1 nov.');
  });

  it('isoDate utilise la date locale', () => {
    expect(isoDate(new Date('2026-11-26T23:30:00+01:00'))).toBe('2026-11-26');
  });
});

describe('durées', () => {
  it('formate les durées de réunion', () => {
    expect(duration('2026-11-26T10:00:00+01:00', '2026-11-26T10:17:40+01:00')).toBe('17 min');
    expect(duration('2026-11-26T10:00:00+01:00', '2026-11-26T11:05:00+01:00')).toBe('1 h 05');
    expect(duration('2026-11-26T14:58:00+01:00', null)).toBe('2 min');
    // Jamais négative.
    expect(duration('2026-11-26T16:00:00+01:00', null)).toBe('0 min');
  });

  it('formate les décalages', () => {
    expect(clockDuration(0)).toBe('0:00');
    expect(clockDuration(65)).toBe('1:05');
    expect(clockDuration(3725)).toBe('1:02:05');
    expect(offset(640.8)).toBe('10:40');
    expect(offset(null)).toBe('');
    expect(offset(-3)).toBe('0:00');
  });
});

describe('libellés', () => {
  it('connaît l’intention « stop »', () => {
    expect(INTENT_LABEL.stop).toBe('Arrêt de l’écoute');
    expect(STATUS_LABEL.refused).toBe('Refusée');
    expect(STATUS_LABEL.done).toBe('Terminée');
  });
  it('cap met une majuscule', () => {
    expect(cap('lundi')).toBe('Lundi');
    expect(cap('')).toBe('');
  });
});

describe('highlight', () => {
  it('échappe le HTML avant de surligner', () => {
    expect(highlight('<b>ms-booking</b> & co', 'customer')).toBe('&lt;b&gt;ms-<mark>customer</mark>&lt;/b&gt; &amp; co');
  });
  it('surligne plusieurs termes, sans tenir compte de la casse', () => {
    expect(highlight('Export Brindille quotidien', 'brindille export')).toBe('<mark>Export</mark> <mark>Brindille</mark> quotidien');
  });
  it('ignore les termes d’un caractère et les caractères spéciaux', () => {
    expect(highlight('a (b) c', 'a')).toBe('a (b) c');
    expect(highlight('prix 3.5 (env.)', '(env.)')).toBe('prix 3.5 <mark>(env.)</mark>');
  });
});

describe('debounce', () => {
  it('n’appelle qu’une fois, avec les derniers arguments', () => {
    const fn = vi.fn();
    const d = debounce(fn, 200);
    d(1);
    d(2);
    vi.advanceTimersByTime(199);
    expect(fn).not.toHaveBeenCalled();
    d(3);
    vi.advanceTimersByTime(200);
    expect(fn).toHaveBeenCalledTimes(1);
    expect(fn).toHaveBeenCalledWith(3);
  });
});
