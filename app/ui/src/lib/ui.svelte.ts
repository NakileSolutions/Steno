// Notifications éphémères et boîtes de dialogue (confirmation, saisie).

import { t } from './i18n.svelte';

export type Toast = { id: number; text: string; kind: 'info' | 'ok' | 'err' };
export const toasts = $state<{ list: Toast[] }>({ list: [] });
let tid = 0;

export function toast(text: string, kind: Toast['kind'] = 'info'): void {
  const id = ++tid;
  toasts.list.push({ id, text, kind });
  setTimeout(() => {
    toasts.list = toasts.list.filter((t) => t.id !== id);
  }, kind === 'err' ? 6000 : 3200);
}

export function toastError(e: unknown): void {
  toast(e instanceof Error ? e.message : String(e), 'err');
}

export type DialogReq = {
  title: string;
  message?: string;
  confirm: string;
  danger?: boolean;
  input?: { value: string; placeholder?: string; label?: string };
  /** Texte à retaper pour confirmer (double confirmation). */
  typeToConfirm?: string;
  resolve: (v: string | boolean | null) => void;
};

export const dialogs = $state<{ current: DialogReq | null }>({ current: null });

export function confirmDialog(o: { title: string; message?: string; confirm?: string; danger?: boolean; typeToConfirm?: string }): Promise<boolean> {
  return new Promise((resolve) => {
    dialogs.current = { confirm: t('Confirmer'), ...o, resolve: (v) => resolve(v === true) };
  });
}

export function promptDialog(o: { title: string; message?: string; value?: string; placeholder?: string; confirm?: string; label?: string }): Promise<string | null> {
  return new Promise((resolve) => {
    dialogs.current = {
      title: o.title,
      message: o.message,
      confirm: o.confirm ?? t('Enregistrer'),
      input: { value: o.value ?? '', placeholder: o.placeholder, label: o.label },
      resolve: (v) => resolve(typeof v === 'string' ? v : null),
    };
  });
}

export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    const ta = document.createElement('textarea');
    ta.value = text;
    ta.style.position = 'fixed';
    ta.style.opacity = '0';
    document.body.appendChild(ta);
    ta.select();
    const ok = document.execCommand('copy');
    ta.remove();
    return ok;
  }
}

export function downloadText(filename: string, text: string, type = 'text/markdown'): void {
  const url = URL.createObjectURL(new Blob([text], { type: `${type};charset=utf-8` }));
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export function slug(s: string): string {
  return s.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'reunion';
}
