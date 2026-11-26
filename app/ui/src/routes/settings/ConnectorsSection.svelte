<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../../lib/api';
  import type { Connector } from '../../lib/types';
  import { relDay } from '../../lib/format';
  import { toastError } from '../../lib/ui.svelte';
  import Icon from '../../lib/components/Icon.svelte';

  let { provider }: { provider: string } = $props();
  let list = $state<Connector[]>([]);
  let refreshing = $state(false);

  onMount(async () => {
    try {
      list = await api.listConnectors();
    } catch (e) {
      toastError(e);
    }
  });

  async function refresh() {
    refreshing = true;
    try {
      list = await api.refreshConnectors();
    } catch (e) {
      toastError(e);
    } finally {
      refreshing = false;
    }
  }
  async function update(c: Connector, live: boolean, corr: boolean) {
    try {
      const n = await api.updateConnector(c.id, live, corr);
      Object.assign(c, n);
    } catch (e) {
      toastError(e);
      list = await api.listConnectors();
    }
  }
  const shown = $derived(list.filter((c) => c.provider === provider || !provider));
</script>

<div class="row top">
  <p class="muted grow">Serveurs MCP configurés dans l’agent choisi. Le Scribe ne les utilise qu’en <strong>lecture seule</strong>. Enregistré automatiquement.</p>
  <button class="btn" onclick={refresh} disabled={refreshing}><Icon name="refresh" size={14} />{refreshing ? 'Détection…' : 'Rafraîchir'}</button>
</div>

<table class="tbl">
  <thead>
    <tr><th>Connecteur</th><th>État</th><th class="c">En direct</th><th class="c">Corrélation</th></tr>
  </thead>
  <tbody>
    {#each shown as c (c.id)}
      <tr class:blocked={!c.write_blockable}>
        <td>
          <strong>{c.name}</strong>
          {#if !c.write_blockable}
            <p class="why small">L’agent ne sait pas bloquer les outils d’écriture de ce connecteur : il ne peut pas être activé.</p>
          {/if}
        </td>
        <td><span class="faint small">{c.status} · {relDay(c.detected_at)}</span></td>
        <td class="c">
          <input type="checkbox" checked={c.use_live} disabled={!c.write_blockable} onchange={(e) => update(c, e.currentTarget.checked, c.use_correlation)} aria-label="{c.name} en direct" />
        </td>
        <td class="c">
          <input type="checkbox" checked={c.use_correlation} disabled={!c.write_blockable} onchange={(e) => update(c, c.use_live, e.currentTarget.checked)} aria-label="{c.name} pour la corrélation" />
        </td>
      </tr>
    {:else}
      <tr><td colspan="4" class="faint">Aucun connecteur détecté. Ajoutez des serveurs MCP dans votre agent puis cliquez sur Rafraîchir.</td></tr>
    {/each}
  </tbody>
</table>

<style>
  .top { gap: 16px; margin: 14px 0; }
  .tbl { width: 100%; border-collapse: collapse; background: var(--surface); border: 1px solid var(--line); border-radius: var(--radius-lg); overflow: hidden; }
  th { text-align: left; font-weight: 500; font-size: 12px; color: var(--text-3); padding: 8px 12px; border-bottom: 1px solid var(--line); }
  td { padding: 9px 12px; border-bottom: 1px solid var(--line); vertical-align: top; }
  tr:last-child td { border-bottom: 0; }
  .c { text-align: center; width: 100px; }
  .blocked strong { color: var(--text-3); }
  .why { color: var(--text-3); margin-top: 2px; max-width: 380px; }
</style>
