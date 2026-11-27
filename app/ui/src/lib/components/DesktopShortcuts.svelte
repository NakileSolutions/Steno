<script lang="ts">
  // Linux (Wayland) : les raccourcis globaux de l'app ne fonctionnent pas. Les mêmes actions
  // existent en ligne de commande, transmises à Sténo déjà lancé : on affiche les commandes à
  // lier dans les raccourcis personnalisés du bureau (GNOME : Paramètres › Clavier).
  // Affiché seulement quand les raccourcis globaux sont indisponibles, sauf `always`.
  import { onMount } from 'svelte';
  import { api } from '../api';
  import { formatShortcut } from '../listening';
  import type { Permissions, Settings } from '../types';
  import { copyText, toast } from '../ui.svelte';
  import Icon from './Icon.svelte';
  import { t, tk } from '../i18n.svelte';
  import { isLinux } from '../platform';

  let { shortcuts = null, always = false }: { shortcuts?: Settings['audio']['shortcuts'] | null; always?: boolean } = $props();

  let perms = $state<Permissions | null>(null);
  onMount(() => {
    api.getPermissions().then((p) => (perms = p)).catch(() => {});
  });

  const ACTIONS = [
    { flag: '--toggle', label: tk('Démarrer / arrêter l’écoute'), key: 'toggle' },
    { flag: '--aside', label: tk('Aparté (activer / désactiver)'), key: 'aside' },
    { flag: '--note', label: tk('Note rapide'), key: 'quick_note' },
    { flag: '--show', label: tk('Afficher Sténo'), key: null },
  ] as const;

  const visible = $derived(isLinux && !!perms && (always || perms.global_shortcuts === false || perms.session === 'wayland'));
  const cmd = $derived(perms?.app_command || 'steno-app');

  async function copy(text: string) {
    if (await copyText(text)) toast(t('Commande copiée.'));
  }
</script>

{#if visible}
  <div class="dsc" role="note">
    <p class="small">
      <Icon name="info" size={13} />
      {#if perms?.session === 'wayland'}
        {t('Session Wayland : les raccourcis globaux de Sténo ne fonctionnent pas. Créez-les dans Paramètres › Clavier › Raccourcis personnalisés, avec ces commandes (Sténo doit être lancé) :')}
      {:else}
        {t('Raccourcis globaux indisponibles. Créez-les dans les raccourcis personnalisés du bureau, avec ces commandes (Sténo doit être lancé) :')}
      {/if}
    </p>
    <ul>
      {#each ACTIONS as a (a.flag)}
        {@const line = `${cmd} ${a.flag}`}
        <li>
          <span class="lbl small">{t(a.label)}{#if a.key && shortcuts?.[a.key]}<span class="faint">&nbsp;· {formatShortcut(shortcuts[a.key])}</span>{/if}</span>
          <code class="mono">{line}</code>
          <button class="btn ghost sm icon" onclick={() => copy(line)} title={t('Copier la commande')} aria-label={t('Copier la commande')}><Icon name="copy" size={13} /></button>
        </li>
      {/each}
    </ul>
    <p class="tiny faint">{t('L’aparté se bascule (une pression pour l’activer, une autre pour le désactiver) : le maintien n’est pas possible avec un raccourci du bureau.')}</p>
  </div>
{/if}

<style>
  .dsc { display: flex; flex-direction: column; gap: 8px; padding: 12px 14px; border: 1px solid var(--line); border-radius: var(--radius-lg); background: var(--surface-2); }
  .dsc > p { display: flex; gap: 6px; align-items: flex-start; line-height: 1.45; }
  .dsc ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 4px; }
  .dsc li { display: grid; grid-template-columns: minmax(150px, 1fr) minmax(0, 2fr) auto; align-items: center; gap: 10px; }
  .dsc code { font-size: 11.5px; padding: 3px 6px; border-radius: 6px; background: var(--surface); border: 1px solid var(--line); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; user-select: all; }
  .lbl { color: var(--text-2); }
</style>
