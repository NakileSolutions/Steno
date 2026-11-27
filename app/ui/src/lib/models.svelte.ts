// Modèles proposés par chaque agent (`list_models`), chargés à la demande et gardés
// pendant la session ; « Rafraîchir » relance la requête.

import { api } from './api';
import type { ModelInfo } from './types';

export type ModelList = { models: ModelInfo[]; loading: boolean; error: string | null; loaded: boolean };

export const modelLists: Record<string, ModelList> = $state({});

const inflight = new Map<string, Promise<ModelInfo[]>>();

/** Liste en cache pour `agent` (vide tant qu'elle n'est pas chargée). */
export function modelsOf(agent: string): ModelInfo[] {
  return modelLists[agent]?.models ?? [];
}

/** Charge la liste de `agent` si besoin (ou toujours avec `force`) et la renvoie. */
export function ensureModels(agent: string, force = false): Promise<ModelInfo[]> {
  if (!agent) return Promise.resolve([]);
  const cur = modelLists[agent];
  if (cur?.loaded && !force) return Promise.resolve(cur.models);
  const running = inflight.get(agent);
  if (running && !force) return running;
  modelLists[agent] = { models: cur?.models ?? [], loading: true, error: null, loaded: cur?.loaded ?? false };
  const p = api
    .listModels(agent)
    .then((models) => {
      modelLists[agent] = { models: Array.isArray(models) ? models : [], loading: false, error: null, loaded: true };
      return modelLists[agent].models;
    })
    .catch((e: unknown) => {
      const error = e instanceof Error ? e.message : String(e);
      modelLists[agent] = { models: cur?.models ?? [], loading: false, error, loaded: true };
      return modelLists[agent].models;
    })
    .finally(() => inflight.delete(agent));
  inflight.set(agent, p);
  return p;
}
