<script lang="ts">
  import { api, openExternal } from '../../lib/api';
  import type { MeetingDetail, Report } from '../../lib/types';
  import { time, shortDate } from '../../lib/format';
  import { copyText, downloadText, slug, toast, toastError } from '../../lib/ui.svelte';
  import { initial, speakerColor, speakerName } from '../../lib/components/speakers';
  import { isEmptyBody, splitReport } from '../../lib/report';
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

  // Mise en page : « En bref » en tête, Décisions et Actions en cartes, le reste en document.
  const parts = $derived(report ? splitReport(report.markdown) : null);
  const brief = $derived(parts?.brief && !isEmptyBody(parts.brief.body) ? parts.brief : null);
  const decisions = $derived(parts?.decisions && !isEmptyBody(parts.decisions.body) ? parts.decisions : null);
  const mdActions = $derived(parts?.actions && !isEmptyBody(parts.actions.body) ? parts.actions : null);
  /** Actions extraites (cases à cocher) : seulement pour la version courante du compte rendu. */
  const liveActions = $derived(!version && detail.actions.length ? detail.actions : null);
  const openCount = $derived(detail.actions.filter((a) => a.status !== 'done').length);
  const countItems = (md: string) => md.split('\n').filter((l) => /^\s*([-*+]|\d+\.)\s+\S/.test(l)).length;

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
      {#if version}<span class="old">Ancienne version</span>{/if}
      <span class="grow"></span>
      <button class="btn sm" onclick={correlate} disabled={!canCorrelate || !!pending} title={correlateHint}>
        {#if pending?.op === 'correlate'}<span class="spin" aria-hidden="true"></span>{:else}<Icon name="link2" size={14} />{/if}{pending?.op === 'correlate' ? 'Corrélation…' : 'Corréler'}
      </button>
      <button class="btn sm" onclick={regenerate} disabled={m.confidential || !!pending} title="Relancer le compte rendu (nouvelle version)">
        {#if pending?.op === 'regenerate'}<span class="spin" aria-hidden="true"></span>{:else}<Icon name="refresh" size={14} />{/if}{pending?.op === 'regenerate' ? 'Régénération…' : 'Régénérer'}
      </button>
      <span class="vsep" aria-hidden="true"></span>
      <button class="btn sm icon ghost" onclick={copy} title="Copier le markdown" aria-label="Copier le markdown"><Icon name="copy" size={15} /></button>
      <button class="btn sm icon ghost" onclick={exportMd} title="Exporter en markdown (copie + fichier .md)" aria-label="Exporter en markdown"><Icon name="download" size={15} /></button>
    </div>

    <p class="faint small gen"><Icon name="ask" size={12} /><span class="ellipsis">Généré {report.provider ? `par ${report.provider}` : ''} à <span class="tnum">{time(report.generated_at)}</span>{report.model ? ` · ${report.model}` : ''}</span></p>

    <article class="report" class:stale={!!pending}>
      {#if brief}
        <section class="brief" aria-labelledby="rp-brief">
          <h2 class="brief-h" id="rp-brief"><Icon name="ask" size={14} stroke={1.8} />{brief.heading}</h2>
          <div class="brief-body"><Markdown source={brief.body} /></div>
        </section>
      {/if}

      {#if decisions}
        <section class="rcard dec" aria-labelledby="rp-dec">
          <h2 class="rc-h" id="rp-dec">
            <span class="rc-ic"><Icon name="flag" size={14} stroke={1.8} /></span>
            Décisions
            {#if countItems(decisions.body)}<span class="bc">{countItems(decisions.body)}</span>{/if}
          </h2>
          <div class="rc-body"><Markdown source={decisions.body} /></div>
        </section>
      {/if}

      {#if liveActions}
        <section class="rcard act" aria-labelledby="rp-act">
          <h2 class="rc-h" id="rp-act">
            <span class="rc-ic"><Icon name="checkcircle" size={14} stroke={1.8} /></span>
            Actions
            <span class="bc" title="{openCount} à faire sur {liveActions.length}">{liveActions.length - openCount}/{liveActions.length}</span>
          </h2>
          <ul class="actions">
            {#each liveActions as a (a.id)}
              <li class:done={a.status === 'done'}>
                <label class="check">
                  <input class="tick" type="checkbox" checked={a.status === 'done'} onchange={(e) => toggleAction(a.id, e.currentTarget.checked)} />
                  <span class="grow">{a.text}</span>
                </label>
                <span class="who small">{#if a.owner}<span class="avatar xs" style="--c: {speakerColor(a.owner)}" aria-hidden="true">{initial(speakerName(a.owner, detail.speakers))}</span>{/if}{a.owner ? speakerName(a.owner, detail.speakers) : '—'}{#if a.due}<span class="due tnum">{shortDate(a.due)}</span>{/if}</span>
              </li>
            {/each}
          </ul>
        </section>
      {:else if mdActions}
        <section class="rcard act" aria-labelledby="rp-act">
          <h2 class="rc-h" id="rp-act">
            <span class="rc-ic"><Icon name="checkcircle" size={14} stroke={1.8} /></span>
            Actions
          </h2>
          <div class="rc-body md-actions"><Markdown source={mdActions.body} /></div>
        </section>
      {/if}

      {#if parts?.rest}
        <div class="doc"><Markdown source={parts.rest} /></div>
      {/if}
    </article>

    {#if detail.refs.length}
      <section class="block">
        <h2 class="bh">Liens avec le code et les outils <span class="bc">{detail.refs.length}</span></h2>
        <ul class="refs card">
          {#each detail.refs as r (r.id)}
            <li>
              <span class="kind">{KIND[r.kind] ?? r.kind}</span>
              <div class="grow">
                {#if r.url}
                  <button class="reflink" onclick={() => openExternal(r.url!)} title={r.url}><code>{r.ref}</code><Icon name="external" size={11} /></button>
                {:else}
                  <code>{r.ref}</code>
                {/if}
                {#if r.title}<span class="muted">{' — '}{r.title}</span>{/if}
                {#if r.note}<p class="faint small note">{r.note}</p>{/if}
              </div>
              {#if r.verified}<span class="badge ok" title="Vérifié par l’agent"><Icon name="check" size={11} stroke={2.2} />Vérifié</span>
              {:else}<span class="badge warn" title="Non vérifié : à confirmer">Supposé</span>{/if}
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if detail.drafts.length}
      <section class="block">
        <h2 class="bh">Brouillons</h2>
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
      <Icon name="note" size={24} stroke={1.5} />
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
  .wrap { padding: 0 32px 56px; max-width: calc(72ch + 64px); font-size: var(--fs-body); }
  .toolbar {
    gap: 6px;
    padding: 14px 0 12px;
    margin-bottom: 22px;
    border-bottom: 1px solid var(--line);
    position: sticky;
    top: 0;
    background: var(--surface);
    z-index: 2;
    font-size: var(--fs-ui);
  }
  .ver { width: auto; height: 28px; font-size: var(--fs-meta); max-width: 260px; font-weight: 500; }
  .gen { display: flex; align-items: center; gap: 6px; margin: -8px 0 18px; }
  .old { display: inline-flex; height: 20px; align-items: center; padding: 0 8px; margin-left: 4px; border-radius: var(--r-pill); background: var(--warn-soft); color: var(--warn); font-weight: 500; font-size: var(--fs-micro); }
  .vsep { width: 1px; height: 18px; background: var(--line-strong); margin: 0 2px; }
  .report { transition: opacity var(--t) var(--ease); }
  .report.stale { opacity: 0.55; }
  .report :global(.md > h1:first-child) { display: none; }

  /* « En bref » : mis en avant, un cran au-dessus. */
  .brief {
    position: relative;
    padding: 18px 22px 16px;
    margin-bottom: 18px;
    border-radius: 14px;
    background:
      radial-gradient(120% 140% at 0% 0%, color-mix(in srgb, var(--accent) 11%, transparent), transparent 60%),
      color-mix(in srgb, var(--accent) 4%, var(--surface));
    box-shadow: inset 0 0 0 1px var(--accent-line), var(--shadow-sm);
    overflow: hidden;
  }
  .brief-h {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-bottom: 8px;
    font-size: var(--fs-micro);
    font-weight: 650;
    letter-spacing: 0.07em;
    text-transform: uppercase;
    color: var(--accent-text);
  }
  .brief-body :global(.md) { font-size: 15.5px; line-height: 1.62; letter-spacing: -0.006em; }
  .brief-body :global(.md ul) { padding-left: 20px; margin-bottom: 0; }
  .brief-body :global(.md li) { margin: 4px 0; }
  .brief-body :global(.md p:last-child) { margin-bottom: 0; }

  /* Décisions et Actions : cartes */
  .rcard {
    margin-bottom: 14px;
    border-radius: 14px;
    background: var(--surface);
    border: 1px solid var(--line);
    box-shadow: var(--shadow-sm);
    overflow: hidden;
  }
  .rc-h {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 12px 16px 10px;
    font-size: var(--fs-ui);
    font-weight: 600;
    letter-spacing: -0.005em;
    border-bottom: 1px solid var(--line);
  }
  .rc-ic { width: 24px; height: 24px; border-radius: 7px; display: grid; place-items: center; flex: none; }
  .dec .rc-ic { color: var(--accent-text); background: var(--accent-soft); }
  .act .rc-ic { color: var(--ok); background: var(--ok-soft); }
  .rc-body :global(.md) { font-size: var(--fs-body); }
  .rc-body :global(.md > ul), .rc-body :global(.md > ol) { list-style: none; margin: 0; padding: 0; }
  .rc-body :global(.md > ul > li), .rc-body :global(.md > ol > li) {
    position: relative;
    margin: 0;
    padding: 10px 16px 10px 40px;
    border-top: 1px solid var(--line);
    line-height: 1.5;
  }
  .rc-body :global(.md > ul > li:first-child), .rc-body :global(.md > ol > li:first-child) { border-top: 0; }
  .dec .rc-body :global(.md > ul > li::before) {
    content: '';
    position: absolute;
    left: 19px;
    top: 17px;
    width: 7px;
    height: 7px;
    border-radius: 2px;
    transform: rotate(45deg);
    background: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft-2);
  }
  .md-actions :global(.md > ul > li::before) {
    content: '';
    position: absolute;
    left: 16px;
    top: 12px;
    width: 14px;
    height: 14px;
    border-radius: 5px;
    box-shadow: inset 0 0 0 1.5px color-mix(in srgb, var(--text) 28%, transparent);
  }
  .rc-body :global(.md > p) { padding: 12px 16px; margin: 0; }
  .rc-body :global(.md > table) { margin: 0; border: 0; border-radius: 0; }
  .doc { margin-top: 30px; }
  .block { margin-top: 34px; }
  .bh { display: flex; align-items: center; gap: 8px; font-size: var(--fs-h3); margin-bottom: 10px; }
  .bc { font-size: var(--fs-micro); font-weight: 600; color: var(--text-3); background: var(--surface-2); border-radius: var(--r-pill); padding: 1px 7px; font-variant-numeric: tabular-nums; }
  .actions, .refs { list-style: none; margin: 0; padding: 4px 0; overflow: hidden; }
  .actions { padding: 0; }
  .actions li { display: flex; align-items: center; gap: 12px; padding: 10px 16px; border-top: 1px solid var(--line); transition: background var(--t-fast) var(--ease); }
  .actions li:first-child, .refs li:first-child { border-top: 0; }
  .actions li:hover { background: var(--hover); }
  .actions li .check { flex: 1; gap: 12px; align-items: center; }
  .actions li.done .check span { color: var(--text-3); text-decoration: line-through; text-decoration-color: var(--line-strong); }
  .who { display: inline-flex; align-items: center; gap: 6px; color: var(--text-2); flex: none; }
  .avatar.xs { width: 20px; height: 20px; font-size: 10px; }
  .due { color: var(--text-3); margin-left: 4px; padding-left: 8px; border-left: 1px solid var(--line-strong); }
  .refs li { display: flex; align-items: flex-start; gap: 12px; padding: 10px 14px; border-top: 1px solid var(--line); }
  .kind {
    flex: none;
    min-width: 76px;
    height: 20px;
    display: inline-grid;
    place-items: center;
    padding: 0 8px;
    border-radius: var(--r-pill);
    font-size: var(--fs-micro);
    font-weight: 600;
    color: var(--text-2);
    background: var(--surface-2);
    box-shadow: inset 0 0 0 1px var(--line);
  }
  code { padding: 1px 6px; border-radius: 6px; background: var(--surface-2); box-shadow: inset 0 0 0 1px var(--line); font-size: 12px; }
  .reflink { all: unset; cursor: pointer; display: inline-flex; align-items: center; gap: 4px; color: var(--accent-text); }
  .reflink:hover code { box-shadow: inset 0 0 0 1px var(--accent-line); }
  .reflink:focus-visible { outline: 2px solid var(--focus); border-radius: 4px; }
  .note { margin-top: 3px; }
  .draft { padding: 12px 14px; margin-top: 10px; }
  .draft .body { margin-top: 10px; white-space: pre-wrap; color: var(--text-2); line-height: 1.55; }
  .spin {
    width: 12px;
    height: 12px;
    border: 1.5px solid currentColor;
    border-right-color: transparent;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
