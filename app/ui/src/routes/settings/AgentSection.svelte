<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../../lib/api';
  import { TIER_ROWS, isFeaturedAgent, retargetTier, sortAgents } from '../../lib/agents';
  import { modelLists } from '../../lib/models.svelte';
  import type { AgentInfo, Settings, TestAgentResult } from '../../lib/types';
  import { toastError } from '../../lib/ui.svelte';
  import AgentLogo from '../../lib/components/AgentLogo.svelte';
  import Icon from '../../lib/components/Icon.svelte';
  import TierRow from './TierRow.svelte';

  let { settings = $bindable() }: { settings: Settings } = $props();
  let agents = $state<AgentInfo[]>([]);
  let detecting = $state(true);
  let testing = $state(false);
  let result = $state<TestAgentResult | null>(null);

  async function detect() {
    detecting = true;
    try {
      agents = await api.detectAgents();
    } catch (e) {
      toastError(e);
    } finally {
      detecting = false;
    }
  }
  onMount(detect);

  async function test() {
    testing = true;
    result = null;
    try {
      result = await api.testAgent(settings.agent.provider);
    } catch (e) {
      result = { ok: false, message: e instanceof Error ? e.message : String(e), duration_ms: 0 };
    } finally {
      testing = false;
    }
  }

  /** Nouvel agent par défaut : les usages qui le suivent (« Agent par défaut ») sont réajustés. */
  function setProvider(id: string) {
    const old = settings.agent.provider;
    if (id === old) return;
    settings.agent.provider = id;
    result = null;
    const efforts = agents.find((a) => a.id === id)?.efforts ?? [];
    const ids = (agent: string) => (modelLists[agent]?.loaded ? modelLists[agent].models.map((m) => m.id) : null);
    for (const r of TIER_ROWS) {
      const t = settings.agent.tiers[r.id];
      if (!t.agent.trim())
        settings.agent.tiers[r.id] = retargetTier(t, '', { efforts, oldModels: ids(old), newModels: ids(id) });
    }
  }

  const SUPPORT: Record<string, [string, string]> = {
    supported: ['Supporté', 'ok'],
    experimental: ['Expérimental', 'warn'],
    generic: ['Générique', ''],
  };
  const sorted = $derived(sortAgents(agents));
  const featured = $derived(sorted.filter((a) => isFeaturedAgent(a.id)));
  const others = $derived(sorted.filter((a) => !isFeaturedAgent(a.id)));
  /** La commande personnalisée se choisit même sans commande configurée (elle se règle ci-dessous). */
  const selectable = (a: AgentInfo) => a.installed || a.id === 'custom';
</script>

{#snippet agentRow(a: AgentInfo)}
  <label class="agent" class:sel={settings.agent.provider === a.id} class:off={!a.installed} data-agent={a.id}>
    <input type="radio" name="agent" value={a.id} checked={settings.agent.provider === a.id} onchange={() => setProvider(a.id)} disabled={!selectable(a)} />
    <AgentLogo id={a.id} name={a.name} size={20} />
    <span class="grow">
      <span class="row nm">
        <strong>{a.name}</strong>
        <span class="badge {SUPPORT[a.support]?.[1] ?? ''}">{SUPPORT[a.support]?.[0] ?? a.support}</span>
        {#if a.installed}<span class="badge outline">Installé</span>{:else}<span class="badge outline">Non installé</span>{/if}
      </span>
      <span class="faint small">
        {#if a.installed}{a.version ? `Version ${a.version}` : 'Installé'}{:else}{a.error ?? 'Non installé'}{/if}
        {#if a.installed && !a.capabilities.tool_deny_list}{' · '}ne sait pas bloquer l’écriture des connecteurs{/if}
      </span>
    </span>
  </label>
{/snippet}

<h2 class="group-title first">Agent par défaut</h2>
<div class="card-group">
  <p class="lead small faint">CLI officiel utilisé avec votre abonnement, pour tout usage sans réglage particulier. Aucun appel d’API payante.</p>
  <div class="agents" role="radiogroup" aria-label="Agent par défaut">
    {#each featured as a (a.id)}{@render agentRow(a)}{/each}
    {#if others.length}
      <details class="others" open={others.some((a) => a.id === settings.agent.provider)}>
        <summary class="small muted">Autres agents ({others.length})</summary>
        <div class="agents">{#each others as a (a.id)}{@render agentRow(a)}{/each}</div>
      </details>
    {/if}
    {#if detecting && !agents.length}<p class="faint small">Détection des agents installés…</p>{/if}
  </div>
  <div class="row acts">
    <button class="btn" onclick={test} disabled={testing}><Icon name="check" size={14} />{testing ? 'Test en cours…' : 'Tester l’agent'}</button>
    <button class="btn ghost" onclick={detect} disabled={detecting}><Icon name="refresh" size={14} />Détecter à nouveau</button>
  </div>
  {#if result}
    <p class="result small" class:ok={result.ok} role="status">
      <Icon name={result.ok ? 'check' : 'x'} size={13} stroke={2} />{result.message}{#if result.duration_ms}{' '}<span class="faint">({(result.duration_ms / 1000).toFixed(1)} s)</span>{/if}
    </p>
  {/if}
</div>

{#if settings.agent.provider === 'custom'}
  <div class="field">
    <label class="field-label" for="custom-cmd">Commande<span class="hint">Modèle de commande : <code>{'{prompt_file}'}</code> = fichier contenant la demande (sinon envoyée sur l'entrée standard), <code>{'{model}'}</code>, <code>{'{run_dir}'}</code>.</span></label>
    <div class="field-body">
      <input id="custom-cmd" class="input mono" bind:value={settings.agent.custom_command} placeholder={'mon-agent --print --input {prompt_file}'} />
    </div>
  </div>
{/if}

<h2 class="group-title">Modèles et réflexion</h2>
<div class="tiers" role="group" aria-label="Modèles et réflexion">
  <p class="help small"><Icon name="info" size={13} />Les recherches consomment le plus : un modèle rapide avec une réflexion moyenne convient souvent en direct. Le compte rendu peut viser plus haut.</p>
  {#each TIER_ROWS as r (r.id)}
    <TierRow bind:tier={settings.agent.tiers[r.id]} row={r} provider={settings.agent.provider} {agents} />
  {/each}
</div>

<div class="field">
  <div class="field-label">Limites</div>
  <div class="field-body grid3">
    <label><span class="small muted">Tâches IA en parallèle</span><input class="input" type="number" min="1" max="4" bind:value={settings.agent.max_parallel} /></label>
    <label><span class="small muted">Appels d’outils par demande</span><input class="input" type="number" min="1" max="50" bind:value={settings.agent.live_tool_budget} /></label>
    <label><span class="small muted">Contexte envoyé (secondes)</span><input class="input" type="number" min="10" max="900" step="10" bind:value={settings.agent.live_context_seconds} /></label>
  </div>
</div>

<div class="field">
  <div class="field-label">Clé API</div>
  <div class="field-body">
    <label class="check toggle"><span>Autoriser l’utilisation d’une clé API</span><input class="switch" type="checkbox" role="switch" bind:checked={settings.agent.allow_api_key} /></label>
    <p class="faint small">Désactivé par défaut : le Scribe n’utilise que l’abonnement connecté dans le CLI. Une clé API peut entraîner une facturation.</p>
  </div>
</div>

<style>
  .group-title { margin: 28px 0 8px 4px; font-size: var(--fs-meta); font-weight: 600; color: var(--text-2); letter-spacing: 0.01em; }
  .group-title.first { margin-top: 24px; }
  .card-group,
  .tiers {
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-sm);
  }
  .card-group { padding: 14px 16px 16px; display: flex; flex-direction: column; gap: 10px; }
  .lead { line-height: 1.45; }
  .agents { display: flex; flex-direction: column; gap: 6px; }
  .agent { display: flex; align-items: center; gap: 12px; padding: 10px 12px; border: 1px solid var(--line); border-radius: 10px; background: var(--surface); transition: border-color var(--t-fast) var(--ease), background var(--t-fast) var(--ease), box-shadow var(--t-fast) var(--ease); }
  .agent:hover:not(.off) { border-color: var(--line-strong); }
  .agent.sel { background: var(--accent-soft); border-color: var(--accent-line); box-shadow: 0 0 0 1px var(--accent-line); }
  .agent.off { opacity: 0.6; }
  .agent .nm { gap: 8px; flex-wrap: wrap; }
  .agent .faint { display: block; margin-top: 1px; }
  .others { display: flex; flex-direction: column; gap: 6px; }
  .others summary { padding: 4px 2px; cursor: default; width: max-content; }
  .others[open] summary { margin-bottom: 6px; }
  .acts { margin-top: 2px; }
  .result { color: var(--err); display: flex; align-items: center; gap: 6px; }
  .result.ok { color: var(--ok); }
  .help { display: flex; align-items: flex-start; gap: 7px; padding: 12px 16px 2px; color: var(--text-2); line-height: 1.5; }
  .help :global(.icon) { margin-top: 3px; color: var(--accent-text); flex: none; }
  .grid3 { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; align-items: end; }
  .grid3 label { display: flex; flex-direction: column; gap: 4px; }
</style>
