<script lang="ts">
  // Une ligne de la carte « Modèles et réflexion » : agent, modèle et niveau de réflexion
  // pour un usage (contracts.md §4 `agent.tiers`).
  import { tick, untrack } from 'svelte';
  import { api } from '../../lib/api';
  import { defaultAgentLabel, effortLabel, resolveTierAgent, retargetTier, sortAgents, type TIER_ROWS } from '../../lib/agents';
  import { ensureModels, modelLists } from '../../lib/models.svelte';
  import type { AgentInfo, AgentTier, ModelInfo, TestAgentResult } from '../../lib/types';
  import AgentPicker, { type AgentOption } from '../../lib/components/AgentPicker.svelte';
  import Icon from '../../lib/components/Icon.svelte';

  let {
    tier = $bindable(),
    row,
    provider,
    agents,
  }: { tier: AgentTier; row: (typeof TIER_ROWS)[number]; provider: string; agents: AgentInfo[] } = $props();

  const OTHER = '\u0000autre';
  const id = $derived(`tier-${row.id}`);
  const eff = $derived(resolveTierAgent(tier, provider));
  const info = $derived(agents.find((a) => a.id === eff));
  const list = $derived(modelLists[eff]);
  const models = $derived(list?.models ?? []);
  const listed = $derived(info?.lists_models ?? false);
  const efforts = $derived(info?.efforts ?? []);
  const known = $derived(models.some((m) => m.id === tier.model));
  const note = $derived(models.find((m) => m.id === tier.model)?.note ?? null);

  /** « grok-4.7 — 2M » : l'identifiant (ce qui est enregistré), puis le nom s'il apporte quelque chose. */
  const modelLabel = (m: ModelInfo) => (m.name && m.name.toLowerCase() !== m.id.toLowerCase() ? `${m.id} — ${m.name}` : m.id);

  let otherOpen = $state(false);
  let otherInput: HTMLInputElement | undefined = $state();
  let testing = $state(false);
  let result = $state<TestAgentResult | null>(null);

  // Liste chargée à la demande (affichage de la ligne, changement d'agent), gardée en cache.
  $effect(() => {
    const e = eff;
    if (e) untrack(() => ensureModels(e));
  });

  const showOther = $derived(otherOpen || (!!tier.model && !known));
  const selectValue = $derived(showOther ? OTHER : tier.model);

  const agentOptions = $derived.by((): AgentOption[] => {
    const opts: AgentOption[] = [{ value: '', label: defaultAgentLabel(provider, agents), logo: provider }];
    for (const a of sortAgents(agents)) opts.push({ value: a.id, label: a.name, logo: a.id, tag: a.installed ? undefined : 'non installé' });
    if (tier.agent && !agents.some((a) => a.id === tier.agent)) opts.push({ value: tier.agent, label: tier.agent, logo: tier.agent, tag: 'inconnu' });
    return opts;
  });

  function setAgent(v: string) {
    const next = resolveTierAgent({ agent: v }, provider);
    const nextList = modelLists[next];
    tier = retargetTier(tier, v, {
      efforts: agents.find((a) => a.id === next)?.efforts ?? [],
      oldModels: models.map((m) => m.id),
      newModels: nextList?.loaded ? nextList.models.map((m) => m.id) : null,
    });
    otherOpen = false;
    result = null;
  }

  async function onModelSelect(v: string) {
    if (v === OTHER) {
      otherOpen = true;
      await tick();
      otherInput?.focus();
      otherInput?.select();
    } else {
      otherOpen = false;
      tier.model = v;
    }
  }

  async function test() {
    testing = true;
    result = null;
    try {
      result = await api.testAgent(eff, tier.model, tier.effort);
    } catch (e) {
      result = { ok: false, message: e instanceof Error ? e.message : String(e), duration_ms: 0 };
    } finally {
      testing = false;
    }
  }
</script>

<div class="trow" role="group" aria-label={row.label} data-tier={row.id}>
  <div class="thead">
    <span class="tic" aria-hidden="true"><Icon name={row.icon} size={14} stroke={1.8} /></span>
    <span class="grow ttl">
      <strong>{row.label}</strong>
      <span class="small faint">{row.hint}</span>
    </span>
    <button class="btn sm" type="button" onclick={test} disabled={testing || !eff}>
      {#if testing}<span class="spin" aria-hidden="true"></span>Test…{:else}<Icon name="check" size={13} />Tester{/if}
    </button>
  </div>

  <div class="tgrid">
    <div class="ctl">
      <span class="clabel small muted">Agent</span>
      <AgentPicker value={tier.agent} options={agentOptions} label="Agent" onchange={setAgent} />
    </div>

    <div class="ctl">
      <span class="clabel small muted">
        <label for="{id}-model">Modèle</label>
        <button
          class="refresh"
          type="button"
          onclick={() => ensureModels(eff, true)}
          disabled={list?.loading}
          title="Rafraîchir la liste des modèles"
          aria-label="Rafraîchir les modèles"
        ><Icon name="refresh" size={11} stroke={2} /></button>
      </span>
      {#if listed}
        <select id="{id}-model" class="input" value={selectValue} onchange={(e) => onModelSelect(e.currentTarget.value)}>
          <option value="">Par défaut du CLI</option>
          {#each models as m (m.id)}<option value={m.id}>{modelLabel(m)}</option>{/each}
          <option value={OTHER}>Autre…</option>
        </select>
        {#if showOther}
          <input class="input mono" bind:this={otherInput} bind:value={tier.model} placeholder="identifiant du modèle" aria-label="Autre modèle" spellcheck="false" autocapitalize="off" />
        {/if}
      {:else}
        <input
          id="{id}-model"
          class="input mono"
          bind:value={tier.model}
          list="{id}-models"
          placeholder="Par défaut du CLI"
          spellcheck="false"
          autocapitalize="off"
        />
        <datalist id="{id}-models">{#each models as m (m.id)}<option value={m.id}>{m.note ?? ''}</option>{/each}</datalist>
        {#if models.length}
          <span class="chips" role="group" aria-label="Suggestions de modèles">
            {#each models.slice(0, 4) as m (m.id)}
              <button type="button" class="chip" aria-pressed={tier.model === m.id} onclick={() => (tier.model = tier.model === m.id ? '' : m.id)} title={modelLabel(m)}>{m.id}</button>
            {/each}
          </span>
        {/if}
      {/if}
      {#if note}
        <span class="sub tiny faint">{note}</span>
      {:else if !listed && eff}
        <span class="sub tiny faint">Saisie libre : ce CLI ne liste pas ses modèles.</span>
      {/if}
      {#if list?.error}<span class="sub tiny faint lerr" title={list.error}><Icon name="warn" size={11} />{list.error}</span>{/if}
    </div>

    <div class="ctl">
      <label class="clabel small muted" for="{id}-effort">Réflexion</label>
      <select id="{id}-effort" class="input" bind:value={tier.effort} disabled={!efforts.length}>
        <option value="">Défaut</option>
        {#each efforts as e (e)}<option value={e}>{effortLabel(e)}</option>{/each}
        {#if tier.effort && !efforts.includes(tier.effort)}<option value={tier.effort}>{effortLabel(tier.effort)}</option>{/if}
      </select>
      {#if !efforts.length}<span class="sub tiny faint">Non réglable pour cet agent.</span>{/if}
    </div>
  </div>

  {#if result}
    <p class="result small" class:ok={result.ok} role="status">
      <Icon name={result.ok ? 'check' : 'x'} size={13} stroke={2} /><span>{result.message}{#if result.duration_ms}{' '}<span class="faint tnum">({(result.duration_ms / 1000).toFixed(1)} s)</span>{/if}</span>
    </p>
  {/if}
</div>

<style>
  .trow { padding: 14px 16px 16px; position: relative; display: flex; flex-direction: column; gap: 10px; }
  .trow + :global(.trow)::before { content: ''; position: absolute; top: 0; left: 16px; right: 16px; border-top: 1px solid var(--line); }
  .thead { display: flex; align-items: center; gap: 12px; }
  .tic { width: 28px; height: 28px; border-radius: 8px; display: grid; place-items: center; flex: none; background: var(--accent-soft); color: var(--accent-text); }
  .ttl { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
  .ttl strong { font-weight: 500; color: var(--text); }
  .tgrid { display: grid; grid-template-columns: minmax(0, 1.35fr) minmax(0, 1.2fr) minmax(0, 0.7fr); gap: 12px; padding-left: 40px; align-items: start; }
  .ctl { display: flex; flex-direction: column; gap: 5px; min-width: 0; }
  .clabel { display: flex; align-items: center; gap: 6px; height: 16px; }
  .refresh {
    all: unset;
    width: 18px;
    height: 18px;
    border-radius: 5px;
    display: grid;
    place-items: center;
    color: var(--text-3);
    cursor: default;
  }
  .refresh:hover:not(:disabled) { background: var(--hover); color: var(--text); }
  .refresh:focus-visible { outline: 2px solid var(--focus); }
  .refresh:disabled { opacity: 0.5; }
  .chips { display: flex; flex-wrap: wrap; gap: 4px; }
  .chip {
    border: 0;
    font: inherit;
    font-size: var(--fs-micro);
    font-family: var(--mono);
    height: 20px;
    padding: 0 7px;
    border-radius: var(--r-pill);
    background: var(--surface-2);
    color: var(--text-2);
    box-shadow: inset 0 0 0 1px var(--line);
    cursor: default;
  }
  .chip:hover { color: var(--text); }
  .chip[aria-pressed='true'] { background: var(--accent-soft); color: var(--accent-text); box-shadow: inset 0 0 0 1px var(--accent-line); }
  .chip:focus-visible { outline: 2px solid var(--focus); outline-offset: 1px; }
  .sub { line-height: 1.4; }
  .lerr { display: flex; gap: 5px; align-items: flex-start; }
  .lerr :global(.icon) { margin-top: 1px; color: var(--warn); flex: none; }
  select:disabled { opacity: 0.55; }
  .result { color: var(--err); display: flex; align-items: flex-start; gap: 6px; padding-left: 40px; line-height: 1.45; }
  .result :global(.icon) { margin-top: 2px; }
  .result.ok { color: var(--ok); }
  .spin { width: 11px; height: 11px; border-radius: 50%; border: 1.5px solid currentColor; border-right-color: transparent; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
