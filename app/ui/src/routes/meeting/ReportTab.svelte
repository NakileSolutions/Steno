<script lang="ts">
  import { api, openExternal } from '../../lib/api';
  import type { MeetingDetail, Report } from '../../lib/types';
  import { time, shortDate } from '../../lib/format';
  import { copyText, slug, toast, toastError } from '../../lib/ui.svelte';
  import { copyRich, plainText, printDocument, reportHtml, saveExport, type ExportFormat } from '../../lib/exporting';
  import Menu, { type MenuItem } from '../../lib/components/Menu.svelte';
  import { initial, speakerColor, speakerName } from '../../lib/components/speakers';
  import { isEmptyBody, splitReport } from '../../lib/report';
  import Markdown from '../../lib/components/Markdown.svelte';
  import Icon from '../../lib/components/Icon.svelte';
  import AgentLogo from '../../lib/components/AgentLogo.svelte';
  import { agentName } from '../../lib/agents';
  import ContextEditor from '../../lib/components/ContextEditor.svelte';
  import ParticipantsEditor from '../../lib/components/ParticipantsEditor.svelte';
  import type { MeetingContext } from '../../lib/types';
  import { t, tk, tn } from '../../lib/i18n.svelte';

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
    m.confidential ? t('Réunion confidentielle : aucun appel à l’IA.')
      : m.correlation === 'off' ? t('Corrélation désactivée pour cette réunion.')
      : !current ? t('Il faut d’abord un compte rendu.')
      : t('Croiser le compte rendu avec le code et les connecteurs cochés')
  );

  async function correlate() {
    try {
      pending = { op: 'correlate', from: current?.version ?? 0 };
      version = null;
      await api.correlate(m.id);
      toast(t('Corrélation lancée : le compte rendu passera en version suivante.'));
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
      toast(t('Régénération lancée.'));
    } catch (e) {
      pending = null;
      toastError(e);
    }
  }
  const docTitle = $derived(m.title ?? t('Réunion'));
  const docDate = $derived(t('{date} à {time}', { date: shortDate(m.started_at), time: time(m.started_at) }));
  const baseName = $derived(`${m.started_at.slice(0, 10)}-${slug(m.title ?? m.id)}`);

  /** Copie mise en forme : se colle telle quelle dans Teams, Slack, Outlook ou Gmail. */
  async function copy() {
    if (!report) return;
    const ok = await copyRich(reportHtml(docTitle, docDate, report.markdown), `${docTitle} — ${docDate}\n\n${plainText(report.markdown)}`);
    if (ok) toast(t('Compte rendu copié : collez-le dans Teams, Slack ou un mail.'));
    else toastError(t('Copie impossible.'));
  }
  async function copyMarkdown() {
    if (report && (await copyText(report.markdown))) toast(t('Markdown copié.'));
  }
  async function save(format: ExportFormat) {
    try {
      const where = await saveExport(m.id, format, format === 'markdown' ? baseName : `${baseName}-transcription`);
      if (where) toast(t('Enregistré : {file}', { file: where.split('/').pop() ?? '' }));
    } catch (e) {
      toastError(e);
    }
  }
  async function print() {
    if (!report) return;
    try {
      await printDocument(reportHtml(docTitle, docDate, report.markdown));
    } catch (e) {
      toastError(e);
    }
  }
  const exportItems: MenuItem[] = $derived([
    { label: t('Copier pour Teams, Slack ou un mail'), icon: 'copy', hint: t('mis en forme'), action: copy },
    { label: t('Copier le markdown'), icon: 'copy', action: copyMarkdown },
    { label: t('Imprimer ou enregistrer en PDF…'), icon: 'printer', action: print, separatorBefore: true },
    { label: t('Enregistrer en Markdown…'), icon: 'download', hint: t('.md, avec la transcription'), action: () => save('markdown') },
    { label: t('Transcription en texte…'), icon: 'download', hint: '.txt', action: () => save('txt'), separatorBefore: true },
    { label: t('Sous-titres SRT…'), icon: 'download', hint: '.srt', action: () => save('srt') },
    { label: t('Sous-titres WebVTT…'), icon: 'download', hint: '.vtt', action: () => save('vtt') },
  ]);

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
  const changes = $derived(parts?.changes && !isEmptyBody(parts.changes.body) ? parts.changes : null);

  // Contexte : résumé dans l'en-tête du panneau ; un changement invite à régénérer.
  let ctx = $state<MeetingContext | null>(null);
  const ctxSummary = $derived.by(() => {
    if (!ctx) return '';
    const bits: string[] = [];
    if (ctx.text.trim()) bits.push(t('contexte'));
    if (ctx.files.length) bits.push(tn(ctx.files.length, '{n} document', '{n} documents'));
    if (ctx.template) bits.push(t('format choisi'));
    return bits.join(' · ');
  });
  let ctxDirty = $state(false);
  // Délai avant la rédaction (pour ajouter du contexte) : compte à rebours et « Rédiger maintenant ».
  let reportAt = $state<number | null>(null);
  let nowMs = $state(Date.now());
  const remaining = $derived(reportAt ? Math.max(0, Math.ceil((reportAt - nowMs) / 1000)) : 0);
  const countdown = $derived(`${Math.floor(remaining / 60)}:${String(remaining % 60).padStart(2, '0')}`);
  async function refreshSchedule() {
    try {
      const c = await api.getMeetingContext(m.id);
      reportAt = c.report_at ? Date.parse(c.report_at) : null;
    } catch {
      reportAt = null;
    }
  }
  $effect(() => {
    if (!reportAt) return;
    const timer = setInterval(() => {
      nowMs = Date.now();
      if (reportAt && nowMs >= reportAt) refreshSchedule();
    }, 1000);
    return () => clearInterval(timer);
  });
  async function writeNow() {
    try {
      await api.reportNow(m.id);
      reportAt = null;
    } catch (e) {
      toastError(e);
    }
  }
  function onContext(c: MeetingContext, user = true) {
    ctx = c;
    if (user) {
      ctxDirty = !!current;
      refreshSchedule();
    }
  }
  $effect(() => {
    api.getMeetingContext(m.id).then((c) => {
      onContext(c, false);
      reportAt = c.report_at ? Date.parse(c.report_at) : null;
      nowMs = Date.now();
    }).catch(() => {});
  });
  const decisions = $derived(parts?.decisions && !isEmptyBody(parts.decisions.body) ? parts.decisions : null);
  const mdActions = $derived(parts?.actions && !isEmptyBody(parts.actions.body) ? parts.actions : null);
  /** Actions extraites (cases à cocher) : seulement pour la version courante du compte rendu. */
  const liveActions = $derived(!version && detail.actions.length ? detail.actions : null);
  const openCount = $derived(detail.actions.filter((a) => a.status !== 'done').length);
  const countItems = (md: string) => md.split('\n').filter((l) => /^\s*([-*+]|\d+\.)\s+\S/.test(l)).length;

  const KIND: Record<string, string> = { code: tk('Code'), jira: 'Jira', confluence: 'Confluence', datadog: 'Datadog', gitlab: 'GitLab' };
</script>

<div class="wrap">
  {#if report}
    <div class="toolbar row">
      {#if detail.report_versions.length > 1}
        <select class="input ver" value={report.version} onchange={(e) => pickVersion(Number(e.currentTarget.value))} aria-label={t('Version du compte rendu')}>
          {#each [...detail.report_versions].reverse() as v (v.version)}
            <option value={v.version}>{t('Version {v}', { v: v.version })}{v.correlated ? ` · ${t('corrélée')}` : ''} · {shortDate(v.generated_at)} {time(v.generated_at)}</option>
          {/each}
        </select>
      {/if}
      {#if version}<span class="old">{t('Ancienne version')}</span>{/if}
      <span class="grow"></span>
      <button class="btn sm" onclick={correlate} disabled={!canCorrelate || !!pending} title={correlateHint}>
        {#if pending?.op === 'correlate'}<span class="spin" aria-hidden="true"></span>{:else}<Icon name="link2" size={14} />{/if}{pending?.op === 'correlate' ? t('Corrélation…') : t('Corréler')}
      </button>
      <button class="btn sm" onclick={regenerate} disabled={m.confidential || !!pending} title={t('Relancer le compte rendu (nouvelle version)')}>
        {#if pending?.op === 'regenerate'}<span class="spin" aria-hidden="true"></span>{:else}<Icon name="refresh" size={14} />{/if}{pending?.op === 'regenerate' ? t('Régénération…') : t('Régénérer')}
      </button>
      <span class="vsep" aria-hidden="true"></span>
      <button class="btn sm icon ghost" onclick={copy} title={t('Copier le compte rendu mis en forme (Teams, Slack, mail)')} aria-label={t('Copier le compte rendu')}><Icon name="copy" size={15} /></button>
      <Menu icon="share" items={exportItems} title={t('Exporter')} />
    </div>

    <p class="faint small gen">{#if report.provider}<AgentLogo id={report.provider} name={agentName(report.provider)} size={14} />{:else}<Icon name="ask" size={12} />{/if}<span class="ellipsis">{report.provider ? t('Généré par {agent} à', { agent: agentName(report.provider) }) : t('Généré à')} <span class="tnum">{time(report.generated_at)}</span>{report.model ? ` · ${report.model}` : ''}</span></p>

    {#if !m.confidential}
      <details class="ctxbox">
        <summary><Icon name="paperclip" size={14} />{t('Contexte, documents et format')}{#if ctxSummary}<span class="faint small">{' — '}{ctxSummary}</span>{/if}</summary>
        <div class="ctxbody">
          <ContextEditor meetingId={m.id} onchange={(c) => onContext(c)} />
          <p class="lbl-p">{t('Participants')}</p>
          <ParticipantsEditor meetingId={m.id} onchange={() => (ctxDirty = !!current)} />
          {#if ctxDirty}
            <div class="row redo">
              <p class="small muted grow">{t('Régénérez le compte rendu pour tenir compte de ces changements (l’ancienne version reste consultable).')}</p>
              <button class="btn sm primary" onclick={() => { ctxDirty = false; regenerate(); }} disabled={!!pending}><Icon name="refresh" size={14} />{t('Régénérer')}</button>
            </div>
          {/if}
        </div>
      </details>
    {/if}

    <article class="report" class:stale={!!pending}>
      {#if changes}
        <section class="rcard chg" aria-labelledby="rp-chg">
          <h2 class="rc-h" id="rp-chg"><span class="rc-ic"><Icon name="warn" size={14} stroke={1.8} /></span>{t('Changements importants')}</h2>
          <div class="rc-body"><Markdown source={changes.body} /></div>
        </section>
      {/if}
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
            {t('Décisions')}
            {#if countItems(decisions.body)}<span class="bc">{countItems(decisions.body)}</span>{/if}
          </h2>
          <div class="rc-body"><Markdown source={decisions.body} /></div>
        </section>
      {/if}

      {#if liveActions}
        <section class="rcard act" aria-labelledby="rp-act">
          <h2 class="rc-h" id="rp-act">
            <span class="rc-ic"><Icon name="checkcircle" size={14} stroke={1.8} /></span>
            {t('Actions')}
            <span class="bc" title={t('{open} à faire sur {total}', { open: openCount, total: liveActions.length })}>{liveActions.length - openCount}/{liveActions.length}</span>
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
            {t('Actions')}
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
        <h2 class="bh">{t('Liens avec le code et les outils')} <span class="bc">{detail.refs.length}</span></h2>
        <ul class="refs card">
          {#each detail.refs as r (r.id)}
            <li>
              <span class="kind">{KIND[r.kind] ? t(KIND[r.kind]) : r.kind}</span>
              <div class="grow">
                {#if r.url}
                  <button class="reflink" onclick={() => openExternal(r.url!)} title={r.url}><code>{r.ref}</code><Icon name="external" size={11} /></button>
                {:else}
                  <code>{r.ref}</code>
                {/if}
                {#if r.title}<span class="muted">{' — '}{r.title}</span>{/if}
                {#if r.note}<p class="faint small note">{r.note}</p>{/if}
              </div>
              {#if r.verified}<span class="badge ok" title={t('Vérifié par l’agent')}><Icon name="check" size={11} stroke={2.2} />{t('Vérifié')}</span>
              {:else}<span class="badge warn" title={t('Non vérifié : à confirmer')}>{t('Supposé')}</span>{/if}
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if detail.drafts.length}
      <section class="block">
        <h2 class="bh">{t('Brouillons')}</h2>
        <p class="faint small">{t('Jamais publiés par Sténo : copiez-les si vous voulez les utiliser.')}</p>
        {#each detail.drafts as d (d.id)}
          <div class="draft card">
            <div class="row">
              <span class="badge outline">{d.kind === 'jira_comment' ? t('Commentaire Jira') : d.kind}</span>
              {#if d.target}<code>{d.target}</code>{/if}
              <span class="grow"></span>
              <button class="btn sm ghost" onclick={async () => (await copyText(d.body)) && toast(t('Brouillon copié.'))}><Icon name="copy" size={13} />{t('Copier')}</button>
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
        <h2>{t('Réunion confidentielle')}</h2>
        <p>{t('Aucun compte rendu n’est généré : aucune donnée n’est envoyée à l’IA.')}</p>
      {:else if m.status === 'live'}
        <h2>{t('Réunion en cours')}</h2>
        <p>{t('Le compte rendu sera généré à la fin de la réunion.')}</p>
      {:else if remaining > 0}
        <h2>{t('Rédaction du compte rendu dans')} <span class="tnum">{countdown}</span></h2>
        <p>{t('Vous pouvez encore ajouter du contexte, des documents ou choisir le format ci-dessous. Chaque modification laisse au moins 30 s de plus.')}</p>
        <button class="btn primary" onclick={writeNow}>{t('Rédiger maintenant')}</button>
      {:else if m.status === 'processing' || pending}
        <h2>{t('Compte rendu en préparation…')}</h2>
        <p>{t('Il apparaîtra ici dès qu’il sera prêt.')}</p>
      {:else}
        <h2>{t('Pas encore de compte rendu')}</h2>
        <button class="btn primary" onclick={regenerate}>{t('Générer le compte rendu')}</button>
      {/if}
    </div>
    {#if !m.confidential}
      <section class="ctxempty">
        <h2 class="bh"><Icon name="paperclip" size={15} />{t('Contexte, documents et format')} <span class="faint small">{t('facultatif')}</span></h2>
        {#if m.status === 'processing' && remaining === 0}<p class="faint small hint">{t('Ajouté avant la fin de la préparation, il est pris en compte ; sinon, régénérez le compte rendu ensuite.')}</p>{/if}
        <ContextEditor meetingId={m.id} onchange={(c) => onContext(c, false)} />
      </section>
    {/if}
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
  .chg { border-color: color-mix(in srgb, var(--warn) 40%, var(--line)); }
  .chg .rc-ic { color: var(--warn); background: var(--warn-soft); }
  .ctxbox { margin: -6px 0 20px; border: 1px solid var(--line); border-radius: 12px; background: var(--surface); }
  .ctxbox summary { display: flex; align-items: center; gap: 8px; padding: 10px 14px; cursor: pointer; font-weight: 600; font-size: var(--fs-ui); color: var(--text-2); list-style: none; }
  .ctxbox summary::-webkit-details-marker { display: none; }
  .ctxbox[open] summary { border-bottom: 1px solid var(--line); }
  .ctxbody { padding: 14px; }
  .redo { margin-top: 12px; gap: 10px; align-items: center; padding: 10px 12px; border-radius: 10px; background: var(--accent-soft); }
  .ctxempty { margin-top: 28px; }
  .lbl-p { font-weight: 600; font-size: var(--fs-ui); margin: 14px 0 8px; }
  .ctxempty .hint { margin: -4px 0 10px; }
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
