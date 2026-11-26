<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../../lib/api';
  import type { AgentInfo, Settings, TestAgentResult } from '../../lib/types';
  import { toastError } from '../../lib/ui.svelte';
  import Icon from '../../lib/components/Icon.svelte';

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

  const SUPPORT: Record<string, [string, string]> = {
    supported: ['Supporté', 'ok'],
    experimental: ['Expérimental', 'warn'],
    generic: ['Générique', ''],
  };
  const TIERS = [
    { id: 'quick', label: 'Rapide', hint: 'notes, récaps' },
    { id: 'search', label: 'Recherche', hint: 'demandes en direct, corrélation' },
    { id: 'summary', label: 'Compte rendu', hint: 'fin de réunion' },
  ] as const;
</script>

<div class="field">
  <div class="field-label">Agent<span class="hint">CLI officiel utilisé avec votre abonnement. Aucun appel d’API payante.</span></div>
  <div class="field-body">
    <div class="agents" role="radiogroup" aria-label="Agent IA">
      {#each agents as a (a.id)}
        <label class="agent" class:sel={settings.agent.provider === a.id} class:off={!a.installed}>
          <input type="radio" name="agent" value={a.id} bind:group={settings.agent.provider} disabled={!a.installed} />
          <span class="grow">
            <span class="row"><strong>{a.name}</strong><span class="badge {SUPPORT[a.support][1]}">{SUPPORT[a.support][0]}</span></span>
            <span class="faint small">
              {#if a.installed}{a.version ? `Version ${a.version}` : 'Installé'}{:else}{a.error ?? 'Non installé'}{/if}
              {#if a.installed && !a.capabilities.tool_deny_list}{' · '}ne sait pas bloquer l’écriture des connecteurs{/if}
            </span>
          </span>
        </label>
      {/each}
      {#if detecting && !agents.length}<p class="faint small">Détection des agents installés…</p>{/if}
    </div>
    <div class="row">
      <button class="btn" onclick={test} disabled={testing}><Icon name="check" size={14} />{testing ? 'Test en cours…' : 'Tester l’agent'}</button>
      <button class="btn ghost" onclick={detect} disabled={detecting}><Icon name="refresh" size={14} />Détecter à nouveau</button>
    </div>
    {#if result}
      <p class="result small" class:ok={result.ok} role="status">
        {result.ok ? '✓' : '✕'} {result.message}{#if result.duration_ms}{' '}<span class="faint">({(result.duration_ms / 1000).toFixed(1)} s)</span>{/if}
      </p>
    {/if}
  </div>
</div>

{#if settings.agent.provider === 'custom'}
  <div class="field">
    <label class="field-label" for="custom-cmd">Commande<span class="hint">Modèle de commande. <code>{'{prompt}'}</code> est remplacé par la demande.</span></label>
    <div class="field-body">
      <input id="custom-cmd" class="input mono" bind:value={settings.agent.custom_command} placeholder={'mon-agent --print {prompt}'} />
    </div>
  </div>
{/if}

<div class="field">
  <div class="field-label">Modèle par niveau<span class="hint">Vide = modèle par défaut du CLI.</span></div>
  <div class="field-body tiers">
    {#each TIERS as t (t.id)}
      <label class="tier">
        <span><strong>{t.label}</strong> <span class="faint small">{t.hint}</span></span>
        <input class="input" bind:value={settings.agent.models[t.id]} placeholder="Défaut du CLI" />
      </label>
    {/each}
  </div>
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
    <label class="check"><input type="checkbox" bind:checked={settings.agent.allow_api_key} /> Autoriser l’utilisation d’une clé API</label>
    <p class="faint small">Désactivé par défaut : le Scribe n’utilise que l’abonnement connecté dans le CLI. Une clé API peut entraîner une facturation.</p>
  </div>
</div>

<style>
  .agents { display: flex; flex-direction: column; border: 1px solid var(--line); border-radius: var(--radius-lg); overflow: hidden; background: var(--surface); }
  .agent { display: flex; align-items: flex-start; gap: 10px; padding: 9px 12px; border-top: 1px solid var(--line); }
  .agent:first-child { border-top: 0; }
  .agent input { margin-top: 3px; }
  .agent.sel { background: var(--selected); }
  .agent.off { opacity: 0.55; }
  .agent .row { gap: 8px; }
  .agent .faint { display: block; margin-top: 1px; }
  .result { color: var(--err); }
  .result.ok { color: var(--ok); }
  .tiers { gap: 6px; }
  .tier { display: grid; grid-template-columns: 1fr 220px; align-items: center; gap: 10px; }
  .grid3 { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; }
  .grid3 label { display: flex; flex-direction: column; gap: 4px; }
</style>
