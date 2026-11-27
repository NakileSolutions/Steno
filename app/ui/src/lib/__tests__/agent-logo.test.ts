import { flushSync, mount, unmount } from 'svelte';
import { afterEach, describe, expect, it } from 'vitest';
import AgentLogo, { agentInitial, hasAgentLogo } from '../components/AgentLogo.svelte';

let target: HTMLElement;
let comp: ReturnType<typeof mount> | null = null;

function render(props: { id: string; name?: string; size?: number }) {
  target = document.createElement('div');
  document.body.appendChild(target);
  comp = mount(AgentLogo, { target, props });
  flushSync();
  return target.querySelector('.alogo') as HTMLElement;
}

afterEach(() => {
  if (comp) unmount(comp);
  comp = null;
  target?.remove();
});

describe('AgentLogo', () => {
  it('affiche le SVG du dépôt pour les CLI connus, sans titre ni lien externe', () => {
    for (const id of ['claude-code', 'codex', 'antigravity', 'grok', 'gemini', 'opencode']) {
      expect(hasAgentLogo(id)).toBe(true);
      const el = render({ id, size: 18 });
      expect(el.querySelector('svg')).not.toBeNull();
      expect(el.dataset.fallback).toBeUndefined();
      expect(el.innerHTML).not.toMatch(/<script|<title|href=|https?:\/\/(?!www\.w3\.org)/);
      expect(el.style.getPropertyValue('--al-size')).toBe('18px');
      unmount(comp!);
      comp = null;
    }
  });

  it('rend uniques les identifiants internes de deux logos identiques', () => {
    const a = render({ id: 'antigravity' });
    const idA = a.querySelector('[id]')?.id;
    const b = render({ id: 'antigravity' });
    const idB = b.querySelector('[id]')?.id;
    expect(idA).toBeTruthy();
    expect(idA).not.toBe(idB);
  });

  it('repli : icône terminal pour la commande personnalisée', () => {
    const el = render({ id: 'custom', name: 'Commande personnalisée' });
    expect(hasAgentLogo('custom')).toBe(false);
    expect(el.dataset.fallback).toBe('terminal');
    expect(el.querySelector('svg.icon')).not.toBeNull();
  });

  it('repli : pastille avec l’initiale pour un agent inconnu', () => {
    const el = render({ id: 'mon-agent', name: 'Écrivain local' });
    expect(el.dataset.fallback).toBe('initial');
    expect(el.textContent?.trim()).toBe('E');
    expect(render({ id: 'zz-cli' }).textContent?.trim()).toBe('Z');
  });

  it('initiale : première lettre ou chiffre, sinon « ? »', () => {
    expect(agentInitial('  ollama')).toBe('O');
    expect(agentInitial('42-cli')).toBe('4');
    expect(agentInitial('')).toBe('?');
    expect(agentInitial(null)).toBe('?');
  });
});
