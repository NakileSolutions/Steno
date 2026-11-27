// Routage minimal par hash : `#/meetings/<id>?tab=report`.
import { tick } from 'svelte';

export type Route = { path: string; parts: string[]; query: Record<string, string> };

/** Analyse un hash (`#/meetings/abc?tab=report`) ; vide → `/meetings`. */
export function parseHash(hash: string): Route {
  let raw = hash.replace(/^#/, '');
  try {
    raw = decodeURI(raw);
  } catch {
    // Hash mal encodé : on le garde tel quel.
  }
  raw ||= '/meetings';
  const [p, q = ''] = raw.split('?');
  const path = p.startsWith('/') ? p : '/' + p;
  const query: Record<string, string> = {};
  new URLSearchParams(q).forEach((v, k) => (query[k] = v));
  return { path, parts: path.split('/').filter(Boolean), query };
}

const parse = () => parseHash(location.hash);

export const router = $state<{ route: Route }>({ route: parse() });

/** Route en attente d'une transition : le `hashchange` qui suit ne la relance pas. */
let pending: Route | null = null;

type VTDocument = Document & { startViewTransition?: (cb: () => Promise<void> | void) => unknown };

/** Transition entre vues (View Transitions API) quand la section principale change. */
function transitionTo(next: Route): boolean {
  const doc = document as VTDocument;
  if (typeof doc.startViewTransition !== 'function') return false;
  if (next.parts[0] === router.route.parts[0]) return false;
  if (doc.visibilityState === 'hidden' || matchMedia('(prefers-reduced-motion: reduce)').matches) return false;
  try {
    pending = next;
    doc.startViewTransition(async () => {
      // Une navigation plus récente l'a emporté entre-temps : on ne revient pas en arrière.
      if (pending !== next) return;
      pending = null;
      router.route = next;
      await tick();
    });
    return true;
  } catch {
    pending = null;
    return false;
  }
}

function apply(next: Route, animate: boolean) {
  if (animate && transitionTo(next)) return;
  pending = null;
  router.route = next;
}

const same = (a: Route, b: Route) =>
  a.path === b.path && JSON.stringify(a.query) === JSON.stringify(b.query);

window.addEventListener('hashchange', () => {
  const next = parse();
  if (!same(next, pending ?? router.route)) apply(next, true);
});

/**
 * Navigation synchrone par défaut (le DOM peut être interrogé après `await tick()`).
 * `transition: true` anime le changement de section (la mise à jour devient alors asynchrone).
 */
export function navigate(to: string, opts: { replace?: boolean; transition?: boolean } = {}): void {
  const hash = '#' + (to.startsWith('/') ? to : '/' + to);
  if (opts.replace) history.replaceState(null, '', hash);
  else if (location.hash !== hash) location.hash = hash;
  apply(parse(), !!opts.transition);
}

export function href(to: string): string {
  return '#' + (to.startsWith('/') ? to : '/' + to);
}

/** Met à jour un paramètre de la route courante sans ajouter d'entrée d'historique. */
export function setQuery(patch: Record<string, string | null>): void {
  const q = { ...router.route.query };
  for (const [k, v] of Object.entries(patch)) {
    if (v === null) delete q[k];
    else q[k] = v;
  }
  const s = new URLSearchParams(q).toString();
  navigate(router.route.path + (s ? '?' + s : ''), { replace: true });
}
