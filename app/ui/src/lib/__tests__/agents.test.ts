import { describe, expect, it } from 'vitest';
import {
  agentName, defaultAgentLabel, defaultTiers, effortLabel, normalizeAgentSettings, resolveTierAgent,
  retargetTier, sortAgents,
} from '../agents';
import { normalizeSettings } from '../listening';
import type { Settings } from '../types';

describe('effortLabel', () => {
  it('traduit les niveaux connus', () => {
    expect(['none', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max', 'ultra'].map(effortLabel)).toEqual([
      'Aucune', 'Minimale', 'Basse', 'Moyenne', 'Élevée', 'Très élevée', 'Maximale', 'Ultra',
    ]);
  });
  it('vide = « Défaut » ; valeur inconnue telle quelle', () => {
    expect(effortLabel('')).toBe('Défaut');
    expect(effortLabel(null)).toBe('Défaut');
    expect(effortLabel('turbo')).toBe('turbo');
    expect(effortLabel('HIGH')).toBe('Élevée');
  });
});

describe('agent par défaut', () => {
  const agents = [{ id: 'claude-code', name: 'Claude Code' }, { id: 'grok', name: 'Grok Build' }];
  it('un usage sans agent suit l’agent par défaut', () => {
    expect(resolveTierAgent({ agent: '' }, 'claude-code')).toBe('claude-code');
    expect(resolveTierAgent({ agent: '  ' }, 'codex')).toBe('codex');
    expect(resolveTierAgent({ agent: 'grok' }, 'claude-code')).toBe('grok');
    expect(resolveTierAgent(undefined, 'codex')).toBe('codex');
  });
  it('libellé « Agent par défaut (X) »', () => {
    expect(defaultAgentLabel('claude-code', agents)).toBe('Agent par défaut (Claude Code)');
    expect(defaultAgentLabel('antigravity')).toBe('Agent par défaut (Antigravity)');
    expect(agentName('inconnu')).toBe('inconnu');
    expect(agentName('grok', agents)).toBe('Grok Build');
  });
});

describe('retargetTier', () => {
  const claudeModels = ['fable', 'opus', 'sonnet', 'haiku', 'claude-opus-5-5'];
  const grokModels = ['grok-4.7', 'grok-4.6', 'grok-4.5'];
  const grokEfforts = ['low', 'medium', 'high', 'xhigh'];

  it('remet à vide l’effort absent du nouvel agent, garde un effort commun', () => {
    expect(retargetTier({ agent: 'claude-code', model: '', effort: 'max' }, 'grok', { efforts: grokEfforts }).effort).toBe('');
    expect(retargetTier({ agent: 'claude-code', model: '', effort: 'high' }, 'grok', { efforts: grokEfforts }).effort).toBe('high');
    expect(retargetTier({ agent: 'claude-code', model: '', effort: 'high' }, 'gemini', { efforts: [] }).effort).toBe('');
  });

  it('remet à vide un modèle de l’ancienne liste absent de la nouvelle', () => {
    const t = retargetTier({ agent: 'claude-code', model: 'opus', effort: '' }, 'grok', { efforts: grokEfforts, oldModels: claudeModels, newModels: grokModels });
    expect(t).toEqual({ agent: 'grok', model: '', effort: '' });
  });

  it('remet aussi à vide si la nouvelle liste n’est pas encore connue', () => {
    expect(retargetTier({ agent: 'claude-code', model: 'opus', effort: '' }, 'grok', { efforts: [], oldModels: claudeModels, newModels: null }).model).toBe('');
  });

  it('garde un modèle présent dans les deux listes', () => {
    expect(retargetTier({ agent: 'a', model: 'opus', effort: '' }, 'b', { efforts: [], oldModels: claudeModels, newModels: ['opus'] }).model).toBe('opus');
  });

  it('garde un modèle saisi librement', () => {
    const t = retargetTier({ agent: 'codex', model: 'mon-modele-maison', effort: 'medium' }, 'grok', { efforts: grokEfforts, oldModels: [], newModels: grokModels });
    expect(t).toEqual({ agent: 'grok', model: 'mon-modele-maison', effort: 'medium' });
  });

  it('retour à l’agent par défaut (valeur vide)', () => {
    expect(retargetTier({ agent: 'grok', model: 'grok-4.7', effort: 'xhigh' }, '', { efforts: ['low', 'medium', 'high', 'xhigh', 'max'], oldModels: grokModels, newModels: claudeModels }))
      .toEqual({ agent: '', model: '', effort: 'xhigh' });
  });
});

describe('sortAgents', () => {
  it('met Claude Code, Codex, Antigravity et Grok en avant, dans cet ordre', () => {
    const ids = ['gemini', 'grok', 'custom', 'codex', 'opencode', 'antigravity', 'claude-code'].map((id) => ({ id }));
    expect(sortAgents(ids).map((a) => a.id)).toEqual(['claude-code', 'codex', 'antigravity', 'grok', 'gemini', 'custom', 'opencode']);
  });
});

describe('normalizeAgentSettings', () => {
  it('complète agent.tiers absent (cœur plus ancien) sans toucher agent.models', () => {
    const old = { agent: { provider: 'codex', models: { quick: '', search: 'x', summary: '' } }, audio: { auto_detect_calls: true, call_apps: [] } } as unknown as Settings;
    const n = normalizeSettings(old);
    expect(n.agent.tiers).toEqual(defaultTiers());
    expect(n.agent.models.search).toBe('x');
  });
  it('complète un usage partiel et laisse des réglages complets tels quels', () => {
    const partial = { agent: { provider: 'codex', tiers: { search: { agent: 'grok', model: 'grok-4.7' } } } } as unknown as Settings;
    expect(normalizeAgentSettings(partial).agent.tiers.search).toEqual({ agent: 'grok', model: 'grok-4.7', effort: '' });
    expect(normalizeAgentSettings(partial).agent.tiers.quick).toEqual({ agent: '', model: '', effort: '' });
    const full = { agent: { provider: 'codex', tiers: defaultTiers() } } as unknown as Settings;
    expect(normalizeAgentSettings(full)).toBe(full);
  });
});
