<script lang="ts">
  import Icon from './Icon.svelte';
  // Niveau en dBFS (≈ -60 → 0) affiché sur 12 pastilles.
  let { db, label, icon, active = true }: { db: number; label: string; icon: string; active?: boolean } = $props();
  const N = 12;
  const lit = $derived(active ? Math.round(Math.min(1, Math.max(0, (db + 60) / 55)) * N) : 0);
</script>

<div class="meter" role="meter" aria-label={label} aria-valuemin={-60} aria-valuemax={0} aria-valuenow={Math.round(db)} title="{label} : {Math.round(db)} dB">
  <Icon name={icon} size={14} />
  <span class="lbl">{label}</span>
  <span class="bars">
    {#each Array(N) as _, i}
      <span class="b" class:on={i < lit} class:hot={i >= N - 2}></span>
    {/each}
  </span>
</div>

<style>
  .meter { display: flex; align-items: center; gap: 7px; color: var(--text-2); font-size: 12px; min-width: 0; }
  .lbl { width: 42px; flex: none; }
  .bars { display: flex; gap: 2px; flex: 1; min-width: 60px; }
  .b { flex: 1; height: 6px; border-radius: 2px; background: var(--line-strong); transition: background 0.1s; }
  .b.on { background: var(--text-2); }
  .b.on.hot { background: var(--live); }
</style>
