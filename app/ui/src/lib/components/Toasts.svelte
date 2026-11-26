<script lang="ts">
  import { toasts } from '../ui.svelte';
</script>

<div class="toasts" aria-live="polite">
  {#each toasts.list as t (t.id)}
    <div class="toast {t.kind}" role={t.kind === 'err' ? 'alert' : 'status'}>{t.text}</div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    gap: 6px;
    z-index: 100;
    pointer-events: none;
    align-items: center;
  }
  .toast {
    max-width: 440px;
    padding: 7px 14px;
    border-radius: 8px;
    background: var(--navy-2);
    color: #f1f0ec;
    box-shadow: var(--shadow);
    font-size: 12.5px;
    animation: up 0.18s ease-out;
  }
  .toast.err { background: var(--err); color: #fff; }
  @keyframes up { from { opacity: 0; transform: translateY(6px); } }
</style>
