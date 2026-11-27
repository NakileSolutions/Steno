// Découpe d'un compte rendu markdown en sections (`## …`) pour la mise en page :
// « En bref » mis en avant, Décisions et Actions en cartes, le reste en document.

export type ReportSection = { heading: string; body: string };

export type ReportParts = {
  brief: ReportSection | null;
  /** « ⚠️ Changements importants » : ce qui change par rapport au contexte fourni. */
  changes: ReportSection | null;
  decisions: ReportSection | null;
  actions: ReportSection | null;
  /** Le reste du document (titre `#` retiré), sections dans l'ordre d'origine. */
  rest: string;
};

const fold = (s: string) =>
  s.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase().trim();

function kind(heading: string): 'brief' | 'changes' | 'decisions' | 'actions' | null {
  const h = fold(heading).replace(/^[^a-z0-9]+/, '');
  // Titres français ou anglais (compte rendu rédigé dans l'une ou l'autre langue).
  if (/^(changements? importants?|important changes?|key changes?|what changed)\b/.test(h)) return 'changes';
  if (/^(en bref|resume|tl;?dr|summary|in brief|at a glance)\b/.test(h)) return 'brief';
  if (/^decisions?\b/.test(h)) return 'decisions';
  if (/^(actions?|action items?|next steps)\b/.test(h)) return 'actions';
  return null;
}

/** Une section dont le corps est vide ou « — » ne mérite pas de carte. */
export function isEmptyBody(body: string): boolean {
  return !body.replace(/[\s\-*—–•|]/g, '');
}

export function splitReport(md: string): ReportParts {
  const parts: ReportParts = { brief: null, changes: null, decisions: null, actions: null, rest: '' };
  const lines = (md ?? '').replace(/\r\n?/g, '\n').split('\n');
  const rest: string[] = [];
  let cur: { k: ReturnType<typeof kind>; heading: string; body: string[] } | null = null;
  let inFence = false;
  let sawTitle = false;

  const flush = () => {
    if (!cur) return;
    const body = cur.body.join('\n').trim();
    if (cur.k && !parts[cur.k]) parts[cur.k] = { heading: cur.heading, body };
    else rest.push(`## ${cur.heading}`, ...cur.body);
    cur = null;
  };

  for (const line of lines) {
    if (/^\s*(```|~~~)/.test(line)) inFence = !inFence;
    if (!inFence) {
      const h1 = /^#\s+(.+?)\s*#*\s*$/.exec(line);
      if (h1 && !sawTitle && !cur && !rest.some((l) => l.trim())) {
        sawTitle = true;
        continue;
      }
      const h2 = /^##\s+(.+?)\s*#*\s*$/.exec(line);
      if (h2) {
        flush();
        cur = { k: kind(h2[1]), heading: h2[1], body: [] };
        continue;
      }
    }
    if (cur) cur.body.push(line);
    else rest.push(line);
  }
  flush();
  parts.rest = rest.join('\n').trim();
  return parts;
}
