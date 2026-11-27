// Exports côté interface : copie mise en forme (pour Teams, Slack, un mail), impression / PDF,
// enregistrement de fichier (fenêtre « Enregistrer sous… » dans l'app, téléchargement sinon).

import { api, isTauri } from './api';
import { renderMarkdown } from './markdown';
import { downloadText } from './ui.svelte';

export type ExportFormat = 'markdown' | 'txt' | 'srt' | 'vtt';

const MIME: Record<ExportFormat, string> = {
  markdown: 'text/markdown',
  txt: 'text/plain',
  srt: 'application/x-subrip',
  vtt: 'text/vtt',
};
const EXT: Record<ExportFormat, string> = { markdown: 'md', txt: 'txt', srt: 'srt', vtt: 'vtt' };

function escapeHtml(s: string): string {
  return s.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]!);
}

/** HTML autonome d'un compte rendu : titre, date, corps rendu (assaini). */
export function reportHtml(title: string, date: string, markdown: string): string {
  return `<h1>${escapeHtml(title)}</h1><p><em>${escapeHtml(date)}</em></p>${renderMarkdown(markdown)}`;
}

/** Texte brut lisible d'un markdown (repli de la copie mise en forme). */
export function plainText(markdown: string): string {
  return markdown
    .replace(/^#{1,6}\s+/gm, '')
    .replace(/\*\*(.+?)\*\*/g, '$1')
    .replace(/__(.+?)__/g, '$1')
    .replace(/`([^`]+)`/g, '$1')
    .replace(/\[([^\]]+)\]\((https?:[^)]+)\)/g, '$1 ($2)');
}

/**
 * Copie mise en forme : HTML (titres, listes, gras conservés au collage) + texte brut.
 * Doit être appelée pendant le geste de l'utilisateur (clic).
 */
export async function copyRich(html: string, text: string): Promise<boolean> {
  try {
    if (typeof ClipboardItem !== 'undefined' && navigator.clipboard?.write) {
      await navigator.clipboard.write([
        new ClipboardItem({
          'text/html': new Blob([html], { type: 'text/html' }),
          'text/plain': new Blob([text], { type: 'text/plain' }),
        }),
      ]);
      return true;
    }
  } catch {
    // Repli ci-dessous.
  }
  // Repli : sélection d'un bloc hors écran puis copie (garde la mise en forme).
  const el = document.createElement('div');
  el.contentEditable = 'true';
  el.innerHTML = html;
  el.style.cssText = 'position:fixed;left:-9999px;top:0;opacity:0;';
  document.body.appendChild(el);
  const range = document.createRange();
  range.selectNodeContents(el);
  const sel = window.getSelection();
  sel?.removeAllRanges();
  sel?.addRange(range);
  const ok = document.execCommand('copy');
  sel?.removeAllRanges();
  el.remove();
  return ok;
}

/**
 * Impression d'un document (la fenêtre système propose « Enregistrer au format PDF »).
 * Le document est placé dans un bloc dédié, seul visible à l'impression.
 */
export async function printDocument(html: string): Promise<void> {
  document.getElementById('print-doc')?.remove();
  const el = document.createElement('div');
  el.id = 'print-doc';
  el.className = 'print-doc md';
  el.innerHTML = html;
  document.body.appendChild(el);
  document.documentElement.classList.add('printing');
  const cleanup = () => {
    document.documentElement.classList.remove('printing');
    el.remove();
    window.removeEventListener('afterprint', cleanup);
  };
  window.addEventListener('afterprint', cleanup);
  try {
    if (isTauri) await api.printWindow();
    else window.print();
  } finally {
    // Sans événement `afterprint` (impression native asynchrone) : nettoyage différé.
    setTimeout(() => {
      if (document.getElementById('print-doc') === el) cleanup();
    }, 120_000);
  }
}

/**
 * Enregistre un export. Dans l'app : fenêtre « Enregistrer sous… » (renvoie le chemin, ou
 * null si annulé). Hors app : téléchargement du fichier.
 */
export async function saveExport(meetingId: string, format: ExportFormat, fallbackName: string): Promise<string | null> {
  if (isTauri) return api.saveExport(meetingId, format);
  const content = await api.exportMeeting(meetingId, format);
  downloadText(`${fallbackName}.${EXT[format]}`, content, MIME[format]);
  return `${fallbackName}.${EXT[format]}`;
}
