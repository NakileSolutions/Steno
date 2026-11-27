<script lang="ts">
  import { onMount } from 'svelte';
  import { api, onEvent } from '../lib/api';
  import { href } from '../lib/router.svelte';
  import type { Action, ActionFilter, Decision, MeetingSummary } from '../lib/types';
  import { dayLabel, shortDate } from '../lib/format';
  import { toastError } from '../lib/ui.svelte';
  import Icon from '../lib/components/Icon.svelte';
  import { isOverdue } from '../lib/group';
  import { initial, speakerColor } from '../lib/components/speakers';

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
  onMount(() => onEvent('steno://meetings', () => { loadActions(); loadOthers(); }));

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
  const overdue = (a: Action) => isOverdue(a);
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
      <label class="row small faint dl">Du <input class="input" type="date" bind:value={from} /></label>
      <label class="row small faint dl">au <input class="input" type="date" bind:value={to} /></label>
      {#if owner || from || to}
        <button class="btn ghost sm" onclick={() => { owner = ''; from = ''; to = ''; }}><Icon name="x" size={12} />Effacer les filtres</button>
      {/if}
    </div>

    {#if view === 'actions'}
      {#if actions.length}
        <ul class="list">
          {#each actions as a (a.id)}
            <li class:done={a.status === 'done'}>
              <input class="tick" type="checkbox" checked={a.status === 'done'} onchange={(e) => toggle(a, e.currentTarget.checked)} aria-label="Fait : {a.text}" />
              <div class="grow">
                <p class="txt">{a.text}</p>
                <p class="sub small">
                  <a href={href(`/meetings/${a.meeting_id}`)}><Icon name="meetings" size={12} />{a.meeting_title ?? title(a.meeting_id)}</a>
                  {#if a.meeting_started_at}<span class="sep">·</span>{dayLabel(a.meeting_started_at)}{/if}
                </p>
              </div>
              <span class="owner">{#if a.owner}<span class="avatar xs" style="--c: {speakerColor(a.owner)}" aria-hidden="true">{initial(a.owner)}</span>{/if}{a.owner ?? '—'}</span>
              <span class="due" class:late={overdue(a)} class:none={!a.due}>{a.due ? shortDate(a.due) : ''}</span>
            </li>
          {/each}
        </ul>
      {:else}
        <div class="empty"><Icon name="check" size={24} stroke={1.5} /><p>{status === 'open' ? 'Aucune action en attente.' : 'Aucune action.'}</p></div>
      {/if}
    {:else}
      {#if decisions.length}
        <ul class="list">
          {#each decisions as d (d.id)}
            <li>
              <span class="bullet" aria-hidden="true"><Icon name="check" size={11} stroke={2.2} /></span>
              <div class="grow">
                <p class="txt">{d.text}</p>
                <p class="sub small"><a href={href(`/meetings/${d.meeting_id}`)}><Icon name="meetings" size={12} />{title(d.meeting_id)}</a><span class="sep">·</span>{dayLabel(d.created_at)}</p>
              </div>
            </li>
          {/each}
        </ul>
      {:else}
        <div class="empty"><Icon name="actions" size={24} stroke={1.5} /><p>Aucune décision sur la période.</p></div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .page { height: 100%; }
  .inner { max-width: 900px; padding: 32px 40px 56px; }
  .head { margin-bottom: 18px; }
  .filters { gap: 8px; margin-bottom: 16px; }
  .filters .input { width: auto; height: 30px; font-size: 12.5px; }
  .filters .dl { gap: 6px; }
  .list {
    list-style: none;
    margin: 0;
    padding: 4px 0;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-sm);
    overflow: hidden;
  }
  .list li { display: flex; align-items: flex-start; gap: 14px; padding: 12px 16px; border-top: 1px solid var(--line); transition: background var(--t-fast) var(--ease); animation: appear var(--t) var(--ease) both; }
  .list li:first-child { border-top: 0; }
  .list li:hover { background: var(--hover); }
  .list li > input { margin-top: 1px; }
  .txt { font-size: var(--fs-body); line-height: 1.45; }
  li.done .txt { color: var(--text-3); text-decoration: line-through; text-decoration-color: var(--line-strong); }
  .sub { color: var(--text-3); margin-top: 3px; display: flex; align-items: center; gap: 0; flex-wrap: wrap; }
  .sub a { color: var(--text-2); text-decoration: none; display: inline-flex; align-items: center; gap: 5px; }
  .sub a :global(.icon) { color: var(--text-3); }
  .sub a:hover { color: var(--accent-text); }
  .sep { margin: 0 6px; }
  .owner { display: inline-flex; align-items: center; gap: 7px; min-width: 110px; justify-content: flex-end; color: var(--text-2); font-size: 12.5px; padding-top: 1px; }
  .avatar.xs { width: 20px; height: 20px; font-size: 10px; }
  .due { min-width: 64px; height: 22px; display: inline-grid; place-items: center; padding: 0 8px; border-radius: var(--r-pill); background: var(--surface-2); color: var(--text-2); font-size: var(--fs-meta); font-variant-numeric: tabular-nums; }
  .due.none { background: transparent; }
  .due.late { background: var(--warn-soft); color: var(--warn); font-weight: 600; }
  .bullet { width: 20px; height: 20px; border-radius: 50%; display: grid; place-items: center; background: var(--ok-soft); color: var(--ok); flex: none; margin-top: 1px; }
</style>
