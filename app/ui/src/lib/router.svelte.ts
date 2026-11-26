// Routage minimal par hash : `#/meetings/<id>?tab=report`.

export type Route = { path: string; parts: string[]; query: Record<string, string> };

function parse(): Route {
  const raw = decodeURI(location.hash.replace(/^#/, '')) || '/meetings';
  const [p, q = ''] = raw.split('?');
  const path = p.startsWith('/') ? p : '/' + p;
  const query: Record<string, string> = {};
  new URLSearchParams(q).forEach((v, k) => (query[k] = v));
  return { path, parts: path.split('/').filter(Boolean), query };
}

export const router = $state<{ route: Route }>({ route: parse() });

window.addEventListener('hashchange', () => {
  router.route = parse();
});

export function navigate(to: string, opts: { replace?: boolean } = {}): void {
  const hash = '#' + (to.startsWith('/') ? to : '/' + to);
  if (opts.replace) history.replaceState(null, '', hash);
  else if (location.hash !== hash) location.hash = hash;
  router.route = parse();
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
