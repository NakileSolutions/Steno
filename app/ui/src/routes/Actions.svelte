<script lang="ts">
  import { onMount } from 'svelte';
  import { api, onEvent } from '../lib/api';
  import { href } from '../lib/router.svelte';
  import type { Action, ActionFilter, Decision, MeetingSummary } from '../lib/types';
  import { dayLabel, shortDate } from '../lib/format';
  import { toastError } from '../lib/ui.svelte';
  import Icon from '../lib/components/Icon.svelte';

  let view = $state<'actions' | 'decisions'>('actions');
  let status = $state<'open' | 'done' | 'all'>('open');
  let owner = $state('');
  let from = $state('');
  let to = $state('');
  let actions = $state<Action[]>([]);
  let allOwners = $state<string[]>([]);
  let decisions = $state<Decision[]>([]);
  let meetings = $state<MeetingSummary[]>([]);

  async function loadActions() {
    const f: ActionFilter = {};
    if (status !== 'all') f.status = status;
    if (owner) f.owner = owner;
    if (from) f.from = from;
    if (to) f.to = to;
    try {
      actions = await api.listActions(f);
    } catch (e) {
      toastError(e);
    }
  }
  async function loadOthers() {
    try {
      const [all, decs, ms] = await Promise.all([api.listActions({}), api.listDecisions(from || undefined), api.listMeetings({ limit: 500 })]);
      allOwners = [...new Set(all.map((a) => a.owner).filter(Boolean) as string[])].sort();
      decisions = decs.filter((d) => !to || d.created_at.slice(0, 10) <= to);
      meetings = ms;
    } catch (e) {
      toastError(e);
    }
  }

  $effect(() => {
    void [status, owner, from, to];
    loadActions();
    loadOthers();
  });
  onMount(() => onEvent('scribe://meetings', () => { loadActions(); loadOthers(); }));

  const title = (id: string) => meetings.find((m) => m.id === id)?.title ?? 'Réunion';

  async function toggle(a: Action, done: boolean) {
    try {
      await api.setActionStatus(a.id, done ? 'done' : 'open');
      a.status = done ? 'done' : 'open';
      if (status !== 'all') setTimeout(loadActions, 600);
    } catch (e) {
      toastError(e);
    }
  }
  const overdue = (a: Action) => a.status === 'open' && !!a.due && a.due < new Date().toISOString().slice(0, 10);
</script>

<div class="page scroll">
  <div class="inner">
    <div class="row head">
      <h1 class="grow">Décisions & actions</h1>
      <div class="seg-ctl" role="tablist" aria-label="Affichage">
        <button role="tab" aria-selected={view === 'actions'} onclick={() => (view = 'actions')}>Actions</button>
        <button role="tab" aria-selected={view === 'decisions'} onclick={() => (view = 'decisions')}>Décisions</button>
      </div>
    </div>

    <div class="filters row wrap">
      {#if view === 'actions'}
        <select class="input" bind:value={status} aria-label="Statut">
          <option value="open">À faire</option>
          <option value="done">Faites</option>
          <option value="all">Toutes</option>
        </select>
        <select class="input" bind:value={owner} aria-label="Personne">
          <option value="">Toutes les personnes</option>
          {#each allOwners as o}<option value={o}>{o}</option>{/each}
        </select>
      {/if}
      <label class="row small faint">Du <input class="input" type="date" bind:value={from} /></label>
      <label class="row small faint">au <input class="input" type="date" bind:value={to} /></label>
      {#if owner || from || to}
        <button class="btn ghost sm" onclick={() => { owner = ''; from = ''; to = ''; }}>Effacer les filtres</button>
      {/if}
    </div>

    {#if view === 'actions'}
      {#if actions.length}
        <ul class="list">
          {#each actions as a (a.id)}
            <li class:done={a.status === 'done'}>
              <input type="checkbox" checked={a.status === 'done'} onchange={(e) => toggle(a, e.currentTarget.checked)} aria-label="Fait : {a.text}" />
              <div class="grow">
                <p class="txt">{a.text}</p>
                <p class="sub small">
                  <a href={href(`/meetings/${a.meeting_id}`)}>{a.meeting_title ?? title(a.meeting_id)}</a>
                  {#if a.meeting_started_at}{' · '}{dayLabel(a.meeting_started_at)}{/if}
                </p>
              </div>
              <span class="owner">{a.owner ?? '—'}</span>
              <span class="due" class:late={overdue(a)}>{a.due ? shortDate(a.due) : ''}</span>
            </li>
          {/each}
        </ul>
      {:else}
        <div class="empty"><Icon name="check" size={28} /><p>{status === 'open' ? 'Aucune action en attente.' : 'Aucune action.'}</p></div>
      {/if}
    {:else}
      {#if decisions.length}
        <ul class="list">
          {#each decisions as d (d.id)}
            <li>
              <span class="bullet" aria-hidden="true"></span>
              <div class="grow">
                <p class="txt">{d.text}</p>
                <p class="sub small"><a href={href(`/meetings/${d.meeting_id}`)}>{title(d.meeting_id)}</a> · {dayLabel(d.created_at)}</p>
              </div>
            </li>
          {/each}
        </ul>
      {:else}
        <div class="empty"><p>Aucune décision sur la période.</p></div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .page { height: 100%; }
  .inner { max-width: 880px; padding: 24px 32px 48px; }
  .head { margin-bottom: 14px; }
  .filters { gap: 8px; margin-bottom: 14px; }
  .filters .input { width: auto; height: 26px; font-size: 12.5px; }
  .filters label { gap: 6px; }
  .list { list-style: none; margin: 0; padding: 0; border-top: 1px solid var(--line); }
  .list li { display: flex; align-items: flex-start; gap: 12px; padding: 10px 4px; border-bottom: 1px solid var(--line); }
  .list li input { margin-top: 3px; }
  .txt { font-size: 13.5px; line-height: 1.45; }
  li.done .txt { color: var(--text-3); text-decoration: line-through; }
  .sub { color: var(--text-3); margin-top: 1px; }
  .sub a { color: var(--text-2); text-decoration: none; }
  .sub a:hover { text-decoration: underline; }
  .owner { width: 90px; text-align: right; color: var(--text-2); font-size: 12.5px; padding-top: 1px; }
  .due { width: 60px; text-align: right; color: var(--text-3); font-size: 12px; padding-top: 2px; font-variant-numeric: tabular-nums; }
  .due.late { color: var(--live); font-weight: 500; }
  .bullet { width: 6px; height: 6px; border-radius: 50%; background: var(--text-2); margin: 7px 4px 0; flex: none; }
</style>
