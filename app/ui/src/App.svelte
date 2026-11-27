<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { router, navigate } from './lib/router.svelte';
  import { initLive } from './lib/live.svelte';
  import { hideCurrent, onEvent } from './lib/api';
  import { MAIN_SECTIONS } from './lib/sections';
  import Dialogs from './lib/components/Dialogs.svelte';
  import Toasts from './lib/components/Toasts.svelte';
  import Tray from './routes/Tray.svelte';
  import Onboarding from './routes/Onboarding.svelte';
  import Shell from './routes/Shell.svelte';

  const isTray = $derived(router.route.path.startsWith('/tray'));

  onMount(() => {
    initLive();
    const un = onEvent('scribe://navigate', ({ route }) => navigate(route));
    return un;
  });

  $effect(() => {
    document.documentElement.classList.toggle('tray', isTray);
  });

  function editable(el: Element | null): boolean {
    if (!el) return false;
    const t = el.tagName;
    return t === 'INPUT' || t === 'TEXTAREA' || t === 'SELECT' || (el as HTMLElement).isContentEditable;
  }

  async function onkeydown(e: KeyboardEvent) {
    if (document.querySelector('dialog[open]')) return;
    const mod = e.metaKey || e.ctrlKey;
    if (e.key === '/' && !mod && !editable(document.activeElement)) {
      e.preventDefault();
      let el = document.querySelector<HTMLInputElement>('[data-search]');
      if (!el && !isTray) {
        navigate('/search');
        await tick();
        el = document.querySelector<HTMLInputElement>('[data-search]');
      }
      el?.focus();
      el?.select();
      return;
    }
    if (e.key === 'Escape') {
      if (editable(document.activeElement)) {
        (document.activeElement as HTMLElement).blur();
      } else if (isTray) {
        hideCurrent();
      }
      return;
    }
    if (mod && !e.shiftKey && !e.altKey && /^[1-5]$/.test(e.key) && !isTray) {
      e.preventDefault();
      navigate(MAIN_SECTIONS[Number(e.key) - 1].path);
    }
    if (mod && e.key === ',' && !isTray) {
      e.preventDefault();
      navigate('/settings');
    }
  }
</script>

<svelte:window {onkeydown} />

{#if isTray}
  <Tray />
{:else if router.route.path.startsWith('/onboarding')}
  <Onboarding />
{:else}
  <Shell />
{/if}

<Dialogs />
<Toasts />
