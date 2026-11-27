// Plateforme d'exécution : classes posées sur <html> dès le démarrage (avant le premier rendu).
import { isTauri } from './api';

/** L'app tourne dans Tauri sur macOS : fenêtre transparente avec la matière native (NSVisualEffectView). */
export const nativeVibrancy: boolean =
  isTauri && typeof navigator !== 'undefined' && /Mac/i.test(navigator.userAgent);

/** API View Transitions disponible (WebKit récent, Chromium). */
export const hasViewTransitions: boolean =
  typeof document !== 'undefined' && typeof (document as Document & { startViewTransition?: unknown }).startViewTransition === 'function';

export function applyPlatformClasses(root: HTMLElement = document.documentElement): void {
  root.classList.toggle('native-vibrancy', nativeVibrancy);
  root.classList.toggle('vt', hasViewTransitions);
}
