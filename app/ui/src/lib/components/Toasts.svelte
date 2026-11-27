<script lang="ts">
  import { toasts } from '../ui.svelte';
</script>

<div class="toasts" aria-live="polite">
  {#each toasts.list as t (t.id)}
    <div class="toast {t.kind}" role={t.kind === 'err' ? 'alert' : 'status'}><span class="ti" aria-hidden="true"></span>{t.text}</div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    bottom: 20px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 100;
    pointer-events: none;
    align-items: center;
  }
  .toast {
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: 460px;
    padding: 9px 16px 9px 12px;
    border-radius: 12px;
    background: rgba(27, 31, 42, 0.94);
    color: #f1f2f6;
    box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.06) inset, 0 10px 30px rgba(0, 0, 0, 0.25), 0 2px 6px rgba(0, 0, 0, 0.15);
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    font-size: 13px;
    font-weight: 500;
    line-height: 1.4;
    animation: up 180ms cubic-bezier(0.2, 0, 0, 1);
  }
  .ti {
    width: 18px;
    height: 18px;
    flex: none;
    border-radius: 50%;
    background: #4cc38a url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='%231b1f2a' stroke-width='3' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M6 12.5l4 4L18 8'/%3E%3C/svg%3E") center / 11px no-repeat;
  }
  .toast.err .ti {
    background: #ff6369 url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='white' stroke-width='3' stroke-linecap='round'%3E%3Cpath d='M12 6.5v7M12 17.5v.1'/%3E%3C/svg%3E") center / 12px no-repeat;
  }
  @keyframes up { from { opacity: 0; transform: translateY(8px) scale(0.98); } }
</style>
