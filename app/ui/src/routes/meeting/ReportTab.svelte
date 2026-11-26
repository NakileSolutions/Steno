<script lang="ts">
  import { api, openExternal } from '../../lib/api';
  import type { MeetingDetail, Report } from '../../lib/types';
  import { time, shortDate } from '../../lib/format';
  import { copyText, downloadText, slug, toast, toastError } from '../../lib/ui.svelte';
  import { speakerName } from '../../lib/components/speakers';
  import Markdown from '../../lib/components/Markdown.svelte';
  import Icon from '../../lib/components/Icon.svelte';

  let { detail, reload }: { detail: MeetingDetail; reload: () => Promise<void> } = $props();

  const m = $derived(detail.meeting);
  let version = $state<number | null>(null);
  let shown = $state<Report | null>(null);
  let pending = $state<null | { op: 'correlate' | 'regenerate'; from: number }>(null);

  const current = $derived(detail.report);
  const report = $derived(version && shown && shown.version === version ? shown : current);

  $effect(() => {
    // Nouvelle version reçue : on revient sur la version courante.
    const v = current?.version ?? 0;
    if (pending && v > pending.from) pending = null;
  });

  async function pickVersion(v: number) {
    if (!current || v === current.version) {
      version = null;
      return;
    }
    try {
      shown = await api.getReportVersion(m.id, v);
      version = v;
    } catch (e) {
      toastError(e);
    }
  }

  const canCorrelate = $derived(!m.confidential && m.correlation !== 'off' && !!current && m.status !== 'live');
  const correlateHint = $derived(
    m.confidential ? 'Réunion confidentielle : aucun appel à l’IA.'
      : m.correlation === 'off' ? 'Corrélation désactivée pour cette réunion.'
      : !current ? 'Il faut d’abord un compte rendu.'
      : 'Croiser le compte rendu avec le code et les connecteurs cochés'
  );

  async function correlate() {
    try {
      pending = { op: 'correlate', from: current?.version ?? 0 };
      version = null;
      await api.correlate(m.id);
      toast('Corrélation lancée : le compte rendu passera en version suivante.');
    } catch (e) {
      pending = null;
      toastError(e);
    }
  }
  async function regenerate() {
    try {
      pending = { op: 'regenerate', from: current?.version ?? 0 };
      version = null;
      await api.regenerateReport(m.id);
      toast('Régénération lancée.');
    } catch (e) {
      pending = null;
      toastError(e);
    }
  }
  async function copy() {
    if (report && (await copyText(report.markdown))) toast('Compte rendu copié.');
  }
  async function exportMd() {
    try {
      const md = await api.exportMeeting(m.id);
      await copyText(md);
      downloadText(`${m.started_at.slice(0, 10)}-${slug(m.title ?? m.id)}.md`, md);
      toast('Export markdown copié et téléchargé.');
    } catch (e) {
      toastError(e);
    }
  }

  async function toggleAction(id: number, done: boolean) {
    try {
      await api.setActionStatus(id, done ? 'done' : 'open');
      await reload();
    } catch (e) {
      toastError(e);
    }
  }

  const KIND: Record<string, string> = { code: 'Code', jira: 'Jira', confluence: 'Confluence', datadog: 'Datadog', gitlab: 'GitLab' };
</script>

<div class="wrap">
  {#if report}
    <div class="toolbar row">
      {#if detail.report_versions.length > 1}
        <select class="input ver" value={report.version} onchange={(e) => pickVersion(Number(e.currentTarget.value))} aria-label="Version du compte rendu">
          {#each [...detail.report_versions].reverse() as v (v.version)}
            <option value={v.version}>Version {v.version}{v.correlated ? ' · corrélée' : ''} · {shortDate(v.generated_at)} {time(v.generated_at)}</option>
          {/each}
        </select>
      {/if}
      <span class="faint small grow ellipsis">
        {#if version}Ancienne version — {/if}Généré {report.provider ? `par ${report.provider}` : ''} à {time(report.generated_at)}{report.model ? ` · ${report.model}` : ''}
      </span>
      <button class="btn sm" onclick={correlate} disabled={!canCorrelate || !!pending} title={correlateHint}>
        <Icon name="link2" size={14} />{pending?.op === 'correlate' ? 'Corrélation…' : 'Corréler'}
      </button>
      <button class="btn sm" onclick={regenerate} disabled={m.confidential || !!pending} title="Relancer le compte rendu (nouvelle version)">
        <Icon name="refresh" size={14} />{pending?.op === 'regenerate' ? 'Régénération…' : 'Régénérer'}
      </button>
      <button class="btn sm icon" onclick={copy} title="Copier le markdown" aria-label="Copier le markdown"><Icon name="copy" size={14} /></button>
      <button class="btn sm icon" onclick={exportMd} title="Exporter en markdown (copie + fichier .md)" aria-label="Exporter en markdown"><Icon name="download" size={14} /></button>
    </div>

    <article class="report">
      <Markdown source={report.markdown} />
    </article>

    {#if detail.actions.length}
      <section class="block">
        <h2 class="label">Actions extraites</h2>
        <ul class="actions">
          {#each detail.actions as a (a.id)}
            <li class:done={a.status === 'done'}>
              <label class="check">
                <input type="checkbox" checked={a.status === 'done'} onchange={(e) => toggleAction(a.id, e.currentTarget.checked)} />
                <span class="grow">{a.text}</span>
              </label>
              <span class="faint small">{a.owner ? speakerName(a.owner, detail.speakers) : '—'}{a.due ? ` · ${shortDate(a.due)}` : ''}</span>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if detail.refs.length}
      <section class="block">
        <h2 class="label">Liens avec le code et les outils</h2>
        <ul class="refs">
          {#each detail.refs as r (r.id)}
            <li>
              <span class="kind">{KIND[r.kind] ?? r.kind}</span>
              <div class="grow">
                {#if r.url}
                  <button class="reflink" onclick={() => openExternal(r.url!)} title={r.url}><code>{r.ref}</code></button>
                {:else}
                  <code>{r.ref}</code>
                {/if}
                {#if r.title}<span class="muted">{' — '}{r.title}</span>{/if}
                {#if r.note}<p class="faint small">{r.note}</p>{/if}
              </div>
              {#if r.verified}<span class="badge ok" title="Vérifié par l’agent"><Icon name="check" size={11} />Vérifié</span>
              {:else}<span class="badge warn" title="Non vérifié : à confirmer">Supposé</span>{/if}
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if detail.drafts.length}
      <section class="block">
        <h2 class="label">Brouillons</h2>
        <p class="faint small">Jamais publiés par le Scribe : copiez-les si vous voulez les utiliser.</p>
        {#each detail.drafts as d (d.id)}
          <div class="draft card">
            <div class="row">
              <span class="badge outline">{d.kind === 'jira_comment' ? 'Commentaire Jira' : d.kind}</span>
              {#if d.target}<code>{d.target}</code>{/if}
              <span class="grow"></span>
              <button class="btn sm ghost" onclick={async () => (await copyText(d.body)) && toast('Brouillon copié.')}><Icon name="copy" size={13} />Copier</button>
            </div>
            <p class="body">{d.body}</p>
          </div>
        {/each}
      </section>
    {/if}
  {:else}
    <div class="empty">
      <Icon name="note" size={30} stroke={1.2} />
      {#if m.confidential}
        <h2>Réunion confidentielle</h2>
        <p>Aucun compte rendu n’est généré : aucune donnée n’est envoyée à l’IA.</p>
      {:else if m.status === 'live'}
        <h2>Réunion en cours</h2>
        <p>Le compte rendu sera généré à la fin de la réunion.</p>
      {:else if m.status === 'processing' || pending}
        <h2>Compte rendu en préparation…</h2>
        <p>Il apparaîtra ici dès qu’il sera prêt.</p>
      {:else}
        <h2>Pas encore de compte rendu</h2>
        <button class="btn primary" onclick={regenerate}>Générer le compte rendu</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .wrap { padding: 16px 28px 40px; max-width: 820px; }
  .toolbar { gap: 6px; padding-bottom: 14px; margin-bottom: 18px; border-bottom: 1px solid var(--line); position: sticky; top: 0; background: var(--surface); z-index: 1; padding-top: 2px; }
  .ver { width: auto; height: 24px; font-size: 12px; max-width: 280px; }
  .report :global(.md > h1:first-child) { display: none; }
  .block { margin-top: 28px; }
  .block .label { margin-bottom: 8px; }
  .actions, .refs { list-style: none; margin: 0; padding: 0; }
  .actions li { display: flex; align-items: center; gap: 12px; padding: 6px 0; border-bottom: 1px solid var(--line); }
  .actions li .check { flex: 1; }
  .actions li.done .check span { color: var(--text-3); text-decoration: line-through; }
  .refs li { display: flex; align-items: flex-start; gap: 10px; padding: 8px 0; border-bottom: 1px solid var(--line); }
  .kind { width: 74px; flex: none; font-size: 11.5px; color: var(--text-3); padding-top: 1px; }
  .reflink { all: unset; cursor: pointer; }
  .reflink:hover code { text-decoration: underline; }
  .reflink:focus-visible { outline: 2px solid var(--focus); border-radius: 3px; }
  .draft { padding: 10px 12px; margin-top: 8px; }
  .draft .body { margin-top: 8px; white-space: pre-wrap; color: var(--text-2); line-height: 1.5; }
</style>
