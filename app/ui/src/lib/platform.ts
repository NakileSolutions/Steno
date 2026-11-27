// Plateforme d'exécution : système (macOS, Linux, autre) et classes posées sur <html> dès le
// démarrage (avant le premier rendu).
//
// Le système se lit dans l'agent utilisateur (WKWebView sur macOS, WebKitGTK sous Linux). Un
// navigateur ailleurs (Windows, maquette, tests) donne `other`, qui garde les textes macOS
// d'origine. `?platform=linux` (ou `macos`, `other`) dans l'adresse force la valeur (développement
// et tests de bout en bout).

/** Même test que `api.ts` (pas d'import : `listening.ts`, importé par `api.ts`, dépend de ce module). */
const isTauri: boolean = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export type Platform = 'macos' | 'linux' | 'other';

/** Système d'après l'agent utilisateur, ou la valeur forcée si elle est valide. */
export function detectPlatform(userAgent: string, override?: string | null): Platform {
  if (override === 'macos' || override === 'linux' || override === 'other') return override;
  if (/Macintosh|Mac OS X/i.test(userAgent)) return 'macos';
  if (/Linux|X11/i.test(userAgent) && !/Android/i.test(userAgent)) return 'linux';
  return 'other';
}

function override(): string | null {
  try {
    return typeof location !== 'undefined' ? new URLSearchParams(location.search).get('platform') : null;
  } catch {
    return null;
  }
}

export const platform: Platform = detectPlatform(typeof navigator !== 'undefined' ? navigator.userAgent : '', override());
export const isMac = platform === 'macos';
export const isLinux = platform === 'linux';

/** Touche de commande des raccourcis de l'interface (« ⌘ » sur Mac, « Ctrl+ » ailleurs). */
export const modKey: string = typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform) ? '⌘' : 'Ctrl+';

/** L'app tourne dans Tauri sur macOS : fenêtre transparente avec la matière native (NSVisualEffectView). */
export const nativeVibrancy: boolean =
  isTauri && typeof navigator !== 'undefined' && /Mac/i.test(navigator.userAgent);

/**
 * L'app tourne dans Tauri hors macOS : le panneau rapide est une fenêtre transparente sans
 * matière native (le popover dessine lui-même son fond, sans faux bureau derrière).
 */
export const transparentPanel: boolean = isTauri && !nativeVibrancy;

/** API View Transitions disponible (WebKit récent, Chromium). */
export const hasViewTransitions: boolean =
  typeof document !== 'undefined' && typeof (document as Document & { startViewTransition?: unknown }).startViewTransition === 'function';

export function applyPlatformClasses(root: HTMLElement = document.documentElement): void {
  root.classList.toggle('native-vibrancy', nativeVibrancy);
  root.classList.toggle('transparent-panel', transparentPanel);
  root.classList.toggle('vt', hasViewTransitions);
  root.dataset.platform = platform;
}
