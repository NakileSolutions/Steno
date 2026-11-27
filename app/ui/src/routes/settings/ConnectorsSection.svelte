<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../../lib/api';
  import type { Connector } from '../../lib/types';
  import { relDay } from '../../lib/format';
  import { toastError } from '../../lib/ui.svelte';
  import Icon from '../../lib/components/Icon.svelte';
  import { t } from '../../lib/i18n.svelte';

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
  <p class="muted grow">{t('Serveurs MCP configurés dans l’agent choisi. Sténo ne les utilise qu’en')} <strong>{t('lecture seule')}</strong>. {t('Enregistré automatiquement.')}</p>
  <button class="btn" onclick={refresh} disabled={refreshing}><Icon name="refresh" size={14} />{refreshing ? t('Détection…') : t('Rafraîchir')}</button>
</div>

<table class="tbl">
  <thead>
    <tr><th>{t('Connecteur')}</th><th>{t('État')}</th><th class="c">{t('En direct')}</th><th class="c">{t('Corrélation')}</th></tr>
  </thead>
  <tbody>
    {#each shown as c (c.id)}
      <tr class:blocked={!c.write_blockable}>
        <td>
          <strong>{c.name}</strong>
          {#if !c.write_blockable}
            <p class="why small">{t('L’agent ne sait pas bloquer les outils d’écriture de ce connecteur : il ne peut pas être activé.')}</p>
          {/if}
        </td>
        <td><span class="st small"><span class="sdot" class:on={/connect|ok/i.test(c.status)}></span>{c.status} · {relDay(c.detected_at)}</span></td>
        <td class="c">
          <input class="switch" type="checkbox" role="switch" checked={c.use_live} disabled={!c.write_blockable} onchange={(e) => update(c, e.currentTarget.checked, c.use_correlation)} aria-label={t('{name} en direct', { name: c.name })} />
        </td>
        <td class="c">
          <input class="switch" type="checkbox" role="switch" checked={c.use_correlation} disabled={!c.write_blockable} onchange={(e) => update(c, c.use_live, e.currentTarget.checked)} aria-label={t('{name} pour la corrélation', { name: c.name })} />
        </td>
      </tr>
    {:else}
      <tr><td colspan="4" class="faint">{t('Aucun connecteur détecté. Ajoutez des serveurs MCP dans votre agent puis cliquez sur Rafraîchir.')}</td></tr>
    {/each}
  </tbody>
</table>

<style>
  .top { gap: 16px; margin: 20px 0 16px; }
  .top p { line-height: 1.5; }
  .tbl { width: 100%; border-collapse: separate; border-spacing: 0; background: var(--surface); border: 1px solid var(--line); border-radius: var(--radius-lg); overflow: hidden; box-shadow: var(--shadow-sm); }
  th { text-align: left; font-weight: 600; font-size: var(--fs-micro); text-transform: uppercase; letter-spacing: 0.06em; color: var(--text-3); padding: 10px 16px; border-bottom: 1px solid var(--line); background: var(--surface-2); }
  td { padding: 12px 16px; border-bottom: 1px solid var(--line); vertical-align: top; }
  .st { display: inline-flex; align-items: center; gap: 7px; color: var(--text-2); }
  .sdot { width: 7px; height: 7px; border-radius: 50%; background: var(--text-3); }
  .sdot.on { background: var(--ok); }
  tr:last-child td { border-bottom: 0; }
  .c { text-align: center; width: 100px; }
  .blocked strong { color: var(--text-3); }
  .why { color: var(--text-3); margin-top: 2px; max-width: 380px; }
</style>
