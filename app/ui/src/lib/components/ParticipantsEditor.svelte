<script lang="ts">
  // Participants d'une réunion : invités du calendrier ou saisis. Ils aident l'agent du compte
  // rendu à nommer les intervenants.
  import { onMount } from 'svelte';
  import { api } from '../api';
  import type { Participant } from '../types';
  import { toastError } from '../ui.svelte';
  import { t } from '../i18n.svelte';
  import Icon from './Icon.svelte';

  let { meetingId, onchange }: { meetingId: string; onchange?: (p: Participant[]) => void } = $props();

  let list = $state<Participant[]>([]);
  let name = $state('');
  let email = $state('');

  onMount(async () => {
    try {
      list = await api.getParticipants(meetingId);
    } catch (e) {
      toastError(e);
    }
  });

  async function save(next: Participant[]) {
    try {
      list = await api.setParticipants(meetingId, $state.snapshot(next) as Participant[]);
      onchange?.(list);
    } catch (e) {
      toastError(e);
    }
  }

  function add() {
    const n = name.trim();
    if (!n) return;
    save([...list, { name: n, email: email.trim() || null, source: 'manual' }]);
    name = '';
    email = '';
  }
</script>

<div class="people">
  {#if list.length}
    <ul>
      {#each list as p, i (i)}
        <li>
          <Icon name="user" size={13} />
          <span class="grow ellipsis">{p.name}{#if p.email}<span class="faint small"> · {p.email}</span>{/if}</span>
          {#if p.source === 'calendar'}<span class="badge outline" title={t('Invité dans le calendrier')}>{t('Calendrier')}</span>{/if}
          <button class="btn ghost sm icon" onclick={() => save(list.filter((_, j) => j !== i))} title={t('Retirer')} aria-label={t('Retirer {name}', { name: p.name })}><Icon name="x" size={13} /></button>
        </li>
      {/each}
    </ul>
  {:else}
    <p class="faint small">{t('Aucun participant. Ajoutez-les pour que le compte rendu nomme les intervenants.')}</p>
  {/if}
  <form class="row add" onsubmit={(e) => { e.preventDefault(); add(); }}>
    <input class="input" bind:value={name} placeholder={t('Nom')} aria-label={t('Nom du participant')} />
    <input class="input" bind:value={email} placeholder={t('E-mail (facultatif)')} aria-label={t('E-mail du participant')} type="email" />
    <button class="btn sm" type="submit" disabled={!name.trim()}><Icon name="plus" size={13} />{t('Ajouter')}</button>
  </form>
</div>

<style>
  .people { display: flex; flex-direction: column; gap: 8px; }
  ul { list-style: none; margin: 0; padding: 0; border: 1px solid var(--line); border-radius: 10px; background: var(--surface); }
  li { display: flex; align-items: center; gap: 8px; padding: 6px 8px 6px 10px; border-top: 1px solid var(--line); color: var(--text-2); }
  li:first-child { border-top: 0; }
  li .grow { color: var(--text); }
  .add { gap: 6px; }
  .add .input { flex: 1; min-width: 0; }
</style>
