// Palette de commandes (⌘K) : filtrage, regroupement et navigation au clavier. Logique pure.

export type PaletteGroupId = 'recent' | 'meetings' | 'actions';

export type PaletteItem = {
  id: string;
  title: string;
  /** Texte secondaire (date, section…). */
  subtitle?: string;
  /** Mots-clés supplémentaires pour la recherche (non affichés). */
  keywords?: string;
  icon?: string;
  /** Raccourci affiché à droite (« ⌘1 », « ⌘, »…). */
  shortcut?: string;
  /** Texte affiché à droite à défaut de raccourci (heure, jour). */
  hint?: string;
};

export type PaletteGroup<T extends PaletteItem = PaletteItem> = { id: PaletteGroupId; label: string; items: T[] };

export const GROUP_LABEL: Record<PaletteGroupId, string> = {
  recent: 'Récemment ouvert',
  meetings: 'Réunions',
  actions: 'Actions',
};

/** Minuscules, sans accents ni ponctuation superflue : « Réglages » ≈ « reglages ». */
export function normalize(s: string): string {
  return s
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase()
    .replace(/[’']/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/**
 * Score d'un texte pour une requête : chaque mot de la requête doit y figurer.
 * Début du texte > début d'un mot > milieu d'un mot. 0 si un mot manque.
 */
export function matchScore(text: string, query: string): number {
  const q = normalize(query);
  if (!q) return 1;
  const t = normalize(text);
  let score = 0;
  for (const tok of q.split(' ')) {
    const i = t.indexOf(tok);
    if (i < 0) return 0;
    if (i === 0) score += 4;
    else if (/[\s\-—/(«›>·:,.]/.test(t[i - 1])) score += 3;
    else score += 1;
  }
  // Correspondance exacte du titre en tête : bonus.
  if (t.startsWith(q)) score += 2;
  return score;
}

/** Filtre et trie (score décroissant, ordre d'origine à égalité). Requête vide : tout, dans l'ordre. */
export function filterItems<T extends PaletteItem>(items: T[], query: string): T[] {
  if (!normalize(query)) return items.slice();
  return items
    .map((item, i) => {
      // Le titre compte double par rapport aux mots-clés et au sous-titre.
      const s = matchScore(item.title, query) * 2 || matchScore(`${item.title} ${item.subtitle ?? ''} ${item.keywords ?? ''}`, query);
      return { item, i, s };
    })
    .filter((x) => x.s > 0)
    .sort((a, b) => b.s - a.s || a.i - b.i)
    .map((x) => x.item);
}

export type BuildInput<T extends PaletteItem> = {
  query: string;
  /** Réunions récemment ouvertes, de la plus récente à la plus ancienne. */
  recent: T[];
  /** Réunions trouvées (titre connu localement et recherche plein texte du backend). */
  meetings: T[];
  actions: T[];
  /** Repli toujours proposé en fin de liste quand il y a une requête (« Rechercher partout »…). */
  fallback?: T[];
  limits?: Partial<Record<PaletteGroupId, number>>;
};

/**
 * Groupes affichés. Sans requête : « Récemment ouvert » puis « Actions ».
 * Avec une requête : récentes correspondantes, réunions trouvées (sans doublon), actions, puis le repli.
 * Les groupes vides sont omis.
 */
export function buildGroups<T extends PaletteItem>(input: BuildInput<T>): PaletteGroup<T>[] {
  const lim = { recent: 5, meetings: 8, actions: 20, ...input.limits };
  const q = normalize(input.query);
  const recent = filterItems(input.recent, q).slice(0, lim.recent);
  const seen = new Set(recent.map((r) => r.id));
  const meetings: T[] = [];
  if (q) {
    for (const m of input.meetings) {
      if (seen.has(m.id)) continue;
      seen.add(m.id);
      meetings.push(m);
      if (meetings.length >= lim.meetings) break;
    }
  }
  const actions = filterItems(input.actions, q).slice(0, lim.actions);
  if (q && input.fallback) actions.push(...input.fallback);
  const groups: PaletteGroup<T>[] = [
    { id: 'recent', label: GROUP_LABEL.recent, items: recent },
    { id: 'meetings', label: GROUP_LABEL.meetings, items: meetings },
    { id: 'actions', label: GROUP_LABEL.actions, items: actions },
  ];
  return groups.filter((g) => g.items.length);
}

/** Liste à plat, dans l'ordre d'affichage (navigation ↑↓). */
export function flatten<T extends PaletteItem>(groups: PaletteGroup<T>[]): T[] {
  return groups.flatMap((g) => g.items);
}

/** Déplacement de la sélection, en boucle. */
export function moveIndex(i: number, delta: number, length: number): number {
  if (length <= 0) return -1;
  if (i < 0) return delta > 0 ? 0 : length - 1;
  return (((i + delta) % length) + length) % length;
}

/** Ajoute un identifiant en tête des récents, sans doublon. */
export function pushRecent(ids: string[], id: string, max = 8): string[] {
  return [id, ...ids.filter((x) => x !== id)].slice(0, max);
}

/** Fusionne deux listes de réunions (titres d'abord, plein texte ensuite) sans doublon. */
export function mergeById<T extends { id: string }>(...lists: T[][]): T[] {
  const out: T[] = [];
  const seen = new Set<string>();
  for (const l of lists) for (const x of l) if (!seen.has(x.id)) (seen.add(x.id), out.push(x));
  return out;
}
