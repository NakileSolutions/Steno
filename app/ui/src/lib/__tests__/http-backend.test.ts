import { afterEach, describe, expect, it, vi } from 'vitest';
import { httpBackend, httpErrorMessage } from '../api';

afterEach(() => vi.unstubAllGlobals());

describe('httpErrorMessage', () => {
  it('lit {"error": "…"}', () => {
    expect(httpErrorMessage(400, '{"error":"Réunion introuvable."}')).toBe('Réunion introuvable.');
  });
  it('replie sur le texte brut ou le statut', () => {
    expect(httpErrorMessage(500, 'boum')).toBe('boum');
    expect(httpErrorMessage(502, '')).toBe('Erreur 502 du serveur Scribe.');
  });
});

describe('httpBackend', () => {
  it('poste les arguments en JSON sur /api/invoke/<commande>', async () => {
    const fetch = vi.fn(async () => new Response('{"id":"m1"}', { status: 200 }));
    vi.stubGlobal('fetch', fetch);
    const b = httpBackend();
    await expect(b.invoke('get_meeting', { id: 'm1' })).resolves.toEqual({ id: 'm1' });
    expect(fetch).toHaveBeenCalledWith('/api/invoke/get_meeting', expect.objectContaining({
      method: 'POST',
      body: '{"id":"m1"}',
      headers: { 'Content-Type': 'application/json' },
    }));
  });

  it('envoie {} sans arguments et accepte null', async () => {
    const fetch = vi.fn(async () => new Response('null', { status: 200 }));
    vi.stubGlobal('fetch', fetch);
    await expect(httpBackend().invoke('stop_meeting')).resolves.toBeNull();
    expect((fetch.mock.calls[0] as unknown as [string, RequestInit])[1].body).toBe('{}');
  });

  it('lève Error(message) en cas d’échec', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('{"error":"Aucune réunion en cours."}', { status: 400 })));
    await expect(httpBackend().invoke('stop_meeting')).rejects.toThrow('Aucune réunion en cours.');
  });

  it('pick_folder renvoie null sans appeler le serveur', async () => {
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(httpBackend().invoke('pick_folder')).resolves.toBeNull();
    expect(fetch).not.toHaveBeenCalled();
  });

  it('ouvre une seule EventSource et distribue par nom d’événement', async () => {
    const sources: FakeES[] = [];
    class FakeES {
      url: string;
      handlers = new Map<string, ((e: MessageEvent) => void)[]>();
      constructor(url: string) {
        this.url = url;
        sources.push(this);
      }
      addEventListener(name: string, fn: (e: MessageEvent) => void) {
        this.handlers.set(name, [...(this.handlers.get(name) ?? []), fn]);
      }
      fire(name: string, data: string) {
        this.handlers.get(name)?.forEach((fn) => fn(new MessageEvent(name, { data })));
      }
    }
    vi.stubGlobal('EventSource', FakeES);
    const b = httpBackend();
    const live = vi.fn();
    const meetings = vi.fn();
    const un = await b.listen('scribe://live', live);
    await b.listen('scribe://meetings', meetings);
    expect(sources).toHaveLength(1);
    expect(sources[0].url).toBe('/api/events');
    sources[0].fire('scribe://live', '{"state":"listening"}');
    sources[0].fire('scribe://meetings', 'null');
    expect(live).toHaveBeenCalledWith({ state: 'listening' });
    expect(meetings).toHaveBeenCalledWith(null);
    un();
    sources[0].fire('scribe://live', '{"state":"idle"}');
    expect(live).toHaveBeenCalledTimes(1);
  });
});
