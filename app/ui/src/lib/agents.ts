// Logique pure autour des agents IA : choix par usage (contracts.md §4 `agent.tiers`),
// libellés des niveaux de réflexion, ordre d'affichage et noms des CLI.

import type { AgentInfo, AgentTier, AgentTiers, Settings } from './types';
import { t, tk } from './i18n.svelte';

/** Usages réglables, dans l'ordre d'affichage. */
export const TIER_ROWS = [
  { id: 'search', label: tk('Recherches et corrélation'), hint: tk('Recherches en direct, questions sur l’historique, corrélation.'), icon: 'search' },
  { id: 'summary', label: tk('Compte rendu'), hint: tk('Fin de réunion.'), icon: 'note' },
  { id: 'quick', label: tk('Notes et récapitulatifs'), hint: tk('Notes, récaps à la voix, tests.'), icon: 'flag' },
] as const;
export type TierId = (typeof TIER_ROWS)[number]['id'];

const EFFORT_LABELS: Record<string, string> = {
  none: tk('Aucune'),
  minimal: tk('Minimale'),
  low: tk('Basse'),
  medium: tk('Moyenne'),
  high: tk('Élevée'),
  xhigh: tk('Très élevée'),
  max: tk('Maximale'),
  ultra: tk('Ultra'),
};

/** Libellé (traduit) d'un niveau de réflexion ; vide = « Défaut » ; valeur inconnue telle quelle. */
export function effortLabel(effort: string | null | undefined): string {
  const e = effort?.trim() ?? '';
  if (!e) return t('Défaut');
  const l = EFFORT_LABELS[e.toLowerCase()];
  return l ? t(l) : e;
}

/** Agents mis en avant, dans cet ordre ; les autres (Gemini, OpenCode, commande) viennent après. */
export const FEATURED_AGENTS = ['claude-code', 'codex', 'antigravity', 'grok'] as const;

export const isFeaturedAgent = (id: string): boolean => (FEATURED_AGENTS as readonly string[]).includes(id);

/** Agents triés : mis en avant d'abord (ordre fixe), puis les autres dans l'ordre reçu. */
export function sortAgents<T extends Pick<AgentInfo, 'id'>>(agents: readonly T[]): T[] {
  const rank = (id: string) => {
    const i = (FEATURED_AGENTS as readonly string[]).indexOf(id);
    return i < 0 ? FEATURED_AGENTS.length : i;
  };
  return agents.map((a, i) => [a, i] as const).sort((x, y) => rank(x[0].id) - rank(y[0].id) || x[1] - y[1]).map(([a]) => a);
}

const AGENT_NAMES: Record<string, string> = {
  'claude-code': 'Claude Code',
  codex: 'Codex',
  antigravity: 'Antigravity',
  grok: 'Grok Build',
  gemini: 'Gemini CLI',
  opencode: 'OpenCode',
  custom: tk('Commande personnalisée'),
};

/** Nom affiché d'un agent : celui détecté, sinon un nom connu, sinon l'identifiant. */
export function agentName(id: string | null | undefined, agents?: readonly Pick<AgentInfo, 'id' | 'name'>[] | null): string {
  const k = id?.trim() ?? '';
  const known = AGENT_NAMES[k];
  return agents?.find((a) => a.id === k)?.name || (known ? t(known) : k);
}

export const emptyTier = (): AgentTier => ({ agent: '', model: '', effort: '' });
export const defaultTiers = (): AgentTiers => ({ search: emptyTier(), summary: emptyTier(), quick: emptyTier() });

/** Agent réellement utilisé pour un usage : celui choisi, sinon l'agent par défaut. */
export function resolveTierAgent(tier: Pick<AgentTier, 'agent'> | null | undefined, provider: string): string {
  return tier?.agent?.trim() || provider;
}

/** Libellé de l'option « agent par défaut » : « Agent par défaut (Claude Code) ». */
export function defaultAgentLabel(provider: string, agents?: readonly Pick<AgentInfo, 'id' | 'name'>[] | null): string {
  const n = agentName(provider, agents);
  return n ? t('Agent par défaut ({name})', { name: n }) : t('Agent par défaut');
}

/**
 * Nouveau réglage d'un usage quand son agent change (`agent` = '' pour l'agent par défaut).
 * - L'effort est gardé seulement s'il fait partie des efforts du nouvel agent (ensemble connu).
 * - Le modèle est remis à vide s'il venait de la liste de l'ancien agent et n'existe pas dans
 *   celle du nouveau ; un modèle saisi librement (absent de l'ancienne liste) est gardé.
 */
export function retargetTier(
  tier: AgentTier,
  agent: string,
  ctx: { efforts: readonly string[]; oldModels?: readonly string[] | null; newModels?: readonly string[] | null },
): AgentTier {
  const effort = tier.effort && ctx.efforts.includes(tier.effort) ? tier.effort : '';
  const m = tier.model.trim();
  const fromOldList = !!m && !!ctx.oldModels?.includes(m);
  const model = fromOldList && !ctx.newModels?.includes(m) ? '' : tier.model;
  return { agent, model, effort };
}

function normTier(t: unknown): AgentTier {
  const o = (t && typeof t === 'object' ? t : {}) as Partial<Record<keyof AgentTier, unknown>>;
  const str = (v: unknown) => (typeof v === 'string' ? v : '');
  return { agent: str(o.agent), model: str(o.model), effort: str(o.effort) };
}

/** Complète `agent.tiers` (cœur plus ancien, réglages partiels). Renvoie `s` tel quel s'il est complet. */
export function normalizeAgentSettings(s: Settings): Settings {
  const agent = s.agent as Settings['agent'] | undefined;
  if (!agent) return s;
  const t = agent.tiers as Partial<AgentTiers> | undefined;
  const ok = (x: unknown) =>
    !!x && typeof x === 'object' && ['agent', 'model', 'effort'].every((k) => typeof (x as Record<string, unknown>)[k] === 'string');
  if (t && ok(t.search) && ok(t.summary) && ok(t.quick)) return s;
  return { ...s, agent: { ...agent, tiers: { search: normTier(t?.search), summary: normTier(t?.summary), quick: normTier(t?.quick) } } };
}
