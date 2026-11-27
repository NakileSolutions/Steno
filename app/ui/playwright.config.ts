// Tests de bout en bout.
//  - « maquette » : `vite preview` + faux backend en mémoire (mock.ts) ;
//  - « reel »     : `steno serve` (cœur réel, base neuve, faux CLI `claude`, simulateur audio) ;
//  - « captures » : comme « reel », produit les captures de docs/screenshots (hors `npm test`).
import { chromium, defineConfig, devices } from '@playwright/test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { PORTS } from './e2e/ports';

const UI = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(UI, '../..');
const STENO = path.join(ROOT, 'target/debug/steno');
const FAKE_BIN = path.join(ROOT, 'fixtures/fake-bin');


// Projets demandés sur la ligne de commande (tous si aucun) : on ne lance que les serveurs utiles.
function selectedProjects(): Set<string> {
  const out = new Set<string>();
  const argv = process.argv;
  argv.forEach((a, i) => {
    if (a.startsWith('--project=')) out.add(a.slice('--project='.length));
    else if (a === '--project' && argv[i + 1]) out.add(argv[i + 1]);
  });
  return out.size ? out : new Set(['maquette', 'reel']);
}
const wanted = selectedProjects();

// Dossier STENO_HOME neuf par serveur, créé une seule fois (les workers héritent de l'environnement).
function freshHome(key: string): string {
  const v = `STENO_E2E_HOME_${key.toUpperCase()}`;
  if (!process.env[v]) process.env[v] = fs.mkdtempSync(path.join(os.tmpdir(), `steno-e2e-${key}-`));
  return process.env[v]!;
}

function stenoServer(key: keyof typeof PORTS, fixture: string, speed: number) {
  const env: Record<string, string> = {
    ...(process.env as Record<string, string>),
    STENO_HOME: freshHome(key),
    PATH: `${FAKE_BIN}${path.delimiter}${process.env.PATH ?? ''}`,
    STENO_SIDECAR: `${STENO} replay ${fixture} --speed ${speed}`,
  };
  return {
    command: `"${STENO}" serve --port ${PORTS[key]} --ui "${path.join(UI, 'dist')}"`,
    url: `http://127.0.0.1:${PORTS[key]}/`,
    env,
    reuseExistingServer: false,
    timeout: 60_000,
    stdout: 'ignore' as const,
    stderr: 'pipe' as const,
  };
}

const servers = [];
if (wanted.has('maquette')) {
  servers.push({
    command: `npx vite preview --port ${PORTS.maquette} --strictPort --host 127.0.0.1`,
    url: `http://127.0.0.1:${PORTS.maquette}/`,
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  });
}
if (wanted.has('reel')) {
  servers.push(stenoServer('reel', path.join(ROOT, 'fixtures/daily-45min.jsonl'), 40));
  servers.push(stenoServer('voix', path.join(UI, 'e2e/fixtures/arret-voix.jsonl'), 10));
  servers.push(stenoServer('slack', path.join(UI, 'e2e/fixtures/appel-slack.jsonl'), 10));
}
if (wanted.has('captures')) {
  servers.push(stenoServer('captures', path.join(ROOT, 'fixtures/daily-45min.jsonl'), 40));
}

// Chromium préinstallé (environnement d'intégration) si celui de Playwright est absent ;
// sur un poste de dev : `npx playwright install chromium`.
let executablePath: string | undefined;
try {
  if (!fs.existsSync(chromium.executablePath())) throw new Error('absent');
} catch {
  const pre = process.env.PW_CHROMIUM_PATH ?? '/opt/pw-browsers/chromium';
  if (fs.existsSync(pre)) executablePath = pre;
}

const desktop = {
  ...devices['Desktop Chrome'],
  viewport: { width: 1440, height: 900 },
  locale: 'fr-FR',
  timezoneId: 'Europe/Paris',
  launchOptions: { args: ['--lang=fr-FR'], ...(executablePath ? { executablePath } : {}) },
};

export default defineConfig({
  testDir: './e2e',
  outputDir: './test-results',
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: 0,
  workers: 1,
  reporter: process.env.CI ? 'line' : [['list']],
  expect: { timeout: 10_000 },
  use: { trace: 'off', screenshot: 'only-on-failure', actionTimeout: 10_000 },
  projects: [
    {
      name: 'maquette',
      testDir: './e2e/maquette',
      use: { ...desktop, baseURL: `http://127.0.0.1:${PORTS.maquette}` },
    },
    {
      name: 'reel',
      testDir: './e2e/reel',
      timeout: 240_000,
      use: { ...desktop, baseURL: `http://127.0.0.1:${PORTS.reel}` },
    },
    {
      name: 'captures',
      testDir: './e2e/captures',
      timeout: 360_000,
      use: { ...desktop, deviceScaleFactor: 2, baseURL: `http://127.0.0.1:${PORTS.captures}` },
    },
  ],
  webServer: servers,
});
