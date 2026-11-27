<script lang="ts">
  // Droits macOS : état de chaque droit, bouton pour le demander (invite système) ou pour
  // ouvrir directement la bonne page des Réglages Système. Utilisé par l'assistant de premier
  // lancement et par Réglages → Permissions. L'état est relu régulièrement, pour refléter un
  // changement fait dans les Réglages Système.
  import { onMount } from 'svelte';
  import { api } from '../api';
  import type { PermissionKind, Permissions } from '../types';
  import { toastError } from '../ui.svelte';
  import Icon from './Icon.svelte';

  const ROWS: { kind: PermissionKind; label: string; why: string; pane: string; optional?: boolean }[] = [
    { kind: 'microphone', label: 'Micro', why: 'Transcrire votre voix et détecter le mot d’appel.', pane: 'Micro' },
    {
      kind: 'system_audio',
      label: 'Audio du système',
      why: 'Entendre les autres participants dans l’app de l’appel (Teams, Slack, Zoom…).',
      pane: 'Enregistrement de l’écran et de l’audio système',
    },
    { kind: 'accessibility', label: 'Accessibilité', why: 'Lire le titre de l’appel en cours.', pane: 'Accessibilité', optional: true },
  ];

  let perms = $state<Permissions | null>(null);
  let asked = $state<Partial<Record<PermissionKind, boolean>>>({});

  async function refresh() {
    try {
      perms = await api.getPermissions();
    } catch {
      // Le prochain passage réessaiera.
    }
  }

  onMount(() => {
    refresh();
    const t = setInterval(refresh, 1500);
    const onFocus = () => refresh();
    window.addEventListener('focus', onFocus);
    return () => {
      clearInterval(t);
      window.removeEventListener('focus', onFocus);
    };
  });

  async function request(kind: PermissionKind) {
    try {
      await api.requestPermission(kind);
      asked[kind] = true;
      setTimeout(refresh, 800);
    } catch (e) {
      toastError(e);
    }
  }
  async function openSettings(kind: PermissionKind) {
    try {
      await api.openPermissionSettings(kind);
    } catch (e) {
      toastError(e);
    }
  }

  const LABEL = { granted: 'Autorisé', denied: 'Refusé', unknown: 'Pas encore demandé' } as const;
  const BADGE = { granted: 'ok', denied: 'err', unknown: 'warn' } as const;
  const ICON = { granted: 'check', denied: 'x', unknown: 'warn' } as const;
</script>

<ul class="perms">
  {#each ROWS as r (r.kind)}
    {@const v = perms?.[r.kind] ?? 'unknown'}
    <li class={v}>
      <span class="st"><Icon name={ICON[v]} size={14} stroke={2} /></span>
      <div class="grow">
        <p>
          <strong>{r.label}</strong>
          {#if r.optional}<span class="faint small">facultatif</span>{/if}
          <span class="badge {BADGE[v]}">{perms ? LABEL[v] : '…'}</span>
        </p>
        <p class="small muted">{r.why}</p>
        {#if v === 'denied'}
          <p class="small hint">macOS ne redemandera pas : cochez Sténo dans Réglages Système → Confidentialité et sécurité → {r.pane}.</p>
        {:else if v === 'unknown' && asked[r.kind]}
          <p class="small hint">Aucune fenêtre ? Ouvrez les Réglages Système et cochez Sténo dans « {r.pane} ».</p>
        {/if}
      </div>
      <div class="acts">
        {#if v === 'unknown' && perms?.available}
          <button class="btn primary sm" onclick={() => request(r.kind)}>Autoriser</button>
          <button class="btn ghost sm" onclick={() => openSettings(r.kind)}>Réglages Système</button>
        {:else if v === 'granted'}
          <button class="btn ghost sm" onclick={() => openSettings(r.kind)} title="Ouvrir la page {r.pane} des Réglages Système">Réglages Système</button>
        {:else}
          <button class="btn sm" class:primary={v === 'denied'} onclick={() => openSettings(r.kind)}>Ouvrir les Réglages Système</button>
        {/if}
      </div>
    </li>
  {/each}
</ul>
{#if perms && !perms.available}
  <p class="small warnc"><Icon name="warn" size={13} /> Capture audio non démarrée : l’état affiché peut être ancien et les demandes directes sont indisponibles.</p>
{/if}

<style>
  .perms { list-style: none; margin: 0; padding: 4px 0; border: 1px solid var(--line); border-radius: var(--radius-lg); background: var(--surface); box-shadow: var(--shadow-sm); }
  .perms li { display: flex; align-items: flex-start; gap: 12px; padding: 12px 14px; border-top: 1px solid var(--line); }
  .perms li:first-child { border-top: 0; }
  .perms p { line-height: 1.45; display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .perms p + p { margin-top: 2px; }
  .st { width: 22px; height: 22px; border-radius: 50%; display: grid; place-items: center; flex: none; margin-top: 1px; }
  .granted .st { background: var(--ok-soft); color: var(--ok); }
  .unknown .st { background: var(--warn-soft); color: var(--warn); }
  .denied .st { background: var(--err-soft); color: var(--err); }
  .hint { color: var(--text-2); }
  .acts { display: flex; gap: 6px; flex: none; align-items: center; }
  .warnc { color: var(--warn); display: flex; align-items: center; gap: 6px; margin-top: 8px; }
</style>
