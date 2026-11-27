<script lang="ts">
  import Icon from './Icon.svelte';
  import { t } from '../i18n.svelte';
  // Niveau en dBFS (≈ -60 → 0) affiché sur une rangée de barres arrondies.
  let { db, label, icon, active = true }: { db: number; label: string; icon: string; active?: boolean } = $props();
  const N = 18;
  const lit = $derived(active ? Math.round(Math.min(1, Math.max(0, (db + 60) / 55)) * N) : 0);
  // Hauteurs légèrement croissantes : lecture « égaliseur » sans bruit visuel.
  const H = Array.from({ length: N }, (_, i) => 45 + Math.round((i / (N - 1)) * 55));
</script>

<div class="meter" role="meter" aria-label={label} aria-valuemin={-60} aria-valuemax={0} aria-valuenow={Math.round(db)} title={t('{label} : {db} dB', { label, db: Math.round(db) })}>
  <span class="ic"><Icon name={icon} size={14} /></span>
  <span class="lbl">{label}</span>
  <span class="bars" aria-hidden="true">
    {#each H as h, i (i)}
      <span class="b" class:on={i < lit} class:hot={i >= N - 3} style="--h: {h}%"></span>
    {/each}
  </span>
</div>

<style>
  .meter { display: flex; align-items: center; gap: 8px; color: var(--text-2); font-size: var(--fs-meta); min-width: 0; }
  .ic { width: 24px; height: 24px; border-radius: 7px; display: grid; place-items: center; background: var(--surface-2); color: var(--text-2); flex: none; }
  .lbl { width: 40px; flex: none; font-weight: 500; }
  .bars { display: flex; align-items: center; gap: 3px; flex: 1; min-width: 60px; height: 18px; }
  .b {
    flex: 1;
    max-width: 6px;
    height: var(--h);
    border-radius: 3px;
    background: color-mix(in srgb, var(--text) 13%, transparent);
    transform: scaleY(0.8);
    transform-origin: center;
    transition: background 90ms linear, transform 120ms var(--ease);
  }
  .b.on { background: var(--accent); transform: scaleY(1); }
  .b.on.hot { background: var(--warn); }
</style>
