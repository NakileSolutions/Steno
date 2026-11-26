import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';
import { isTauri } from './lib/api';

// Les erreurs de l'interface sont consignées dans le journal de l'app (logs/scribe.log).
if (isTauri) {
  const report = (message: string) => {
    import('@tauri-apps/api/core')
      .then(({ invoke }) => invoke('log_ui_error', { message: message.slice(0, 4000) }))
      .catch(() => {});
  };
  window.addEventListener('error', (e) => report(`${e.message} (${e.filename}:${e.lineno})\n${e.error?.stack ?? ''}`));
  window.addEventListener('unhandledrejection', (e) => report(`promesse rejetée : ${String(e.reason?.stack ?? e.reason)}`));
}

const app = mount(App, { target: document.getElementById('app')! });

export default app;
