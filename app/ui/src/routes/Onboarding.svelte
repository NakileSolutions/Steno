<script lang="ts">
  // Assistant de premier lancement (PLAN §14) : 6 étapes.
  import { onMount } from 'svelte';
  import { api } from '../lib/api';
  import { setPrefs } from '../lib/prefs.svelte';
  import { navigate } from '../lib/router.svelte';
  import type { AgentInfo, Connector, DoctorItem, Repo, Settings, TestAgentResult, WakeCheck } from '../lib/types';
  import { debounce } from '../lib/format';
  import { toast, toastError } from '../lib/ui.svelte';
  import Logo from '../lib/Logo.svelte';
  import Icon from '../lib/components/Icon.svelte';
  import WakeTest from './settings/WakeTest.svelte';

  const STEPS = [
    { id: 'permissions', label: 'Permissions macOS' },
    { id: 'agent', label: 'Agent IA' },
    { id: 'repos', label: 'Dépôts' },
    { id: 'connectors', label: 'Connecteurs' },
    { id: 'wake', label: 'Mot d’appel' },
    { id: 'correlation', label: 'Corrélation' },
  ] as const;

  let step = $state(0);
  let settings = $state<Settings | null>(null);
  let doctor = $state<DoctorItem[]>([]);
  let doctoring = $state(false);
  let agents = $state<AgentInfo[]>([]);
  let testResult = $state<TestAgentResult | null>(null);
  let testing = $state(false);
  let repos = $state<Repo[]>([]);
  let connectors = $state<Connector[]>([]);
  let refreshing = $state(false);
  let wake = $state<WakeCheck | null>(null);
  let wakeTesting = $state(false);
  let saving = $state(false);

  onMount(async () => {
    try {
      settings = await api.getSettings();
      runDoctor();
      agents = await api.detectAgents();
      repos = await api.listRepos();
    } catch (e) {
      toastError(e);
    }
  });

  async function runDoctor() {
    doctoring = true;
    try {
      doctor = await api.doctor();
    } catch (e) {
      toastError(e);
    } finally {
      doctoring = false;
    }
  }
  async function testAgent() {
    if (!settings) return;
    testing = true;
    testResult = null;
    try {
      testResult = await api.testAgent(settings.agent.provider);
    } catch (e) {
      testResult = { ok: false, message: e instanceof Error ? e.message : String(e), duration_ms: 0 };
    } finally {
      testing = false;
    }
  }
  async function addRepo() {
    try {
      const p = await api.pickFolder();
      if (p) repos.push(await api.addRepo(p));
    } catch (e) {
      toastError(e);
    }
  }
  async function removeRepo(r: Repo) {
    try {
      await api.removeRepo(r.id);
      repos = repos.filter((x) => x.id !== r.id);
    } catch (e) {
      toastError(e);
    }
  }
  async function refreshConnectors() {
    refreshing = true;
    try {
      connectors = await api.refreshConnectors();
    } catch (e) {
      toastError(e);
    } finally {
      refreshing = false;
    }
  }
  async function setConnector(c: Connector, on: boolean) {
    try {
      Object.assign(c, await api.updateConnector(c.id, on, on));
    } catch (e) {
      toastError(e);
    }
  }

  const checkWake = debounce(async (w: string) => {
    try {
      wake = await api.checkWakeWord(w, settings?.voice.strict ?? false);
    } catch {}
  }, 350);
  $effect(() => {
    if (settings && STEPS[step].id === 'wake') checkWake(settings.voice.wake_word);
  });
  $effect(() => {
    if (STEPS[step].id === 'connectors' && !connectors.length) refreshConnectors();
  });

  async function persist() {
    if (!settings) return;
    settings = await api.saveSettings($state.snapshot(settings) as Settings);
    setPrefs(settings);
  }

  async function next() {
    saving = true;
    try {
      await persist();
      if (step < STEPS.length - 1) step++;
      else {
        toast('Scribe est prêt.');
        navigate('/meetings');
      }
    } catch (e) {
      toastError(e);
    } finally {
      saving = false;
    }
  }

  async function openWakeTest() {
    try {
      await persist();
      wakeTesting = true;
    } catch (e) {
      toastError(e);
    }
  }

  const blockable = $derived(connectors.filter((c) => c.write_blockable));
  const hiddenCount = $derived(connectors.length - blockable.length);
  const STATUS_ICON: Record<string, string> = { ok: 'check', warn: 'warn', error: 'x' };
</script>

<div class="ob">
  <aside class="rail">
    <div class="brand"><Logo size={40} tile /><div><strong>Scribe</strong><p class="tiny">Premier lancement</p></div></div>
    <ol class="steps">
      {#each STEPS as s, i (s.id)}
        <li class:cur={i === step} class:done={i < step}>
          <button onclick={() => (i <= step ? (step = i) : null)} disabled={i > step} aria-current={i === step ? 'step' : undefined}>
            <span class="num">{#if i < step}<Icon name="check" size={12} stroke={2} />{:else}{i + 1}{/if}</span>
            {s.label}
          </button>
        </li>
      {/each}
    </ol>
    <p class="tiny note">Tout reste sur ce Mac. Vous pourrez tout modifier dans les Réglages.</p>
  </aside>

  <main class="pane scroll">
    {#if settings}
      <div class="inner">
        <p class="label">Étape {step + 1} sur {STEPS.length}</p>

        {#if STEPS[step].id === 'permissions'}
          <h1>Permissions macOS</h1>
          <p class="lead">Le Scribe a besoin du micro, de l’audio du système (pour entendre Teams) et de l’accessibilité (pour lire le titre de la réunion). Il n’écoute que lorsque vous démarrez l’écoute.</p>
          <ul class="doctor">
            {#each doctor as d (d.name)}
              <li class={d.status}>
                <span class="st"><Icon name={STATUS_ICON[d.status]} size={14} stroke={2} /></span>
                <div class="grow">
                  <p><strong>{d.name}</strong> <span class="muted">— {d.message}</span></p>
                  {#if d.fix}<p class="fix small">{d.fix}</p>{/if}
                </div>
              </li>
            {:else}
              <li class="faint">{doctoring ? 'Vérification…' : 'Aucun résultat.'}</li>
            {/each}
          </ul>
          <button class="btn" onclick={runDoctor} disabled={doctoring}><Icon name="refresh" size={14} />{doctoring ? 'Vérification…' : 'Vérifier à nouveau'}</button>

        {:else if STEPS[step].id === 'agent'}
          <h1>Agent IA</h1>
          <p class="lead">Choisissez le CLI d’agent déjà connecté à votre abonnement. Le Scribe ne l’appelle que sur demande et en fin de réunion.</p>
          <div class="choices" role="radiogroup" aria-label="Agent IA">
            {#each agents as a (a.id)}
              <label class="choice" class:sel={settings.agent.provider === a.id} class:off={!a.installed}>
                <input type="radio" name="ob-agent" value={a.id} bind:group={settings.agent.provider} disabled={!a.installed} />
                <span class="grow"><strong>{a.name}</strong> <span class="faint small">{a.installed ? (a.version ?? 'installé') : (a.error ?? 'non installé')}</span></span>
                <span class="badge {a.support === 'supported' ? 'ok' : a.support === 'experimental' ? 'warn' : ''}">{a.support === 'supported' ? 'Supporté' : a.support === 'experimental' ? 'Expérimental' : 'Générique'}</span>
              </label>
            {/each}
          </div>
          <div class="row">
            <button class="btn" onclick={testAgent} disabled={testing}>{testing ? 'Test en cours…' : 'Tester l’agent'}</button>
            {#if testResult}<span class="small" class:okc={testResult.ok} class:errc={!testResult.ok}>{testResult.ok ? '✓' : '✕'} {testResult.message}</span>{/if}
          </div>

        {:else if STEPS[step].id === 'repos'}
          <h1>Dépôts lisibles</h1>
          <p class="lead">Les dossiers de code que l’agent peut lire (jamais modifier) pour répondre à vos questions.</p>
          <ul class="repos">
            {#each repos as r (r.id)}
              <li><Icon name="folder" size={15} /><strong>{r.label}</strong><span class="faint small mono grow ellipsis">{r.path}</span><button class="btn ghost sm icon" onclick={() => removeRepo(r)} aria-label="Retirer {r.label}"><Icon name="x" size={13} /></button></li>
            {:else}
              <li class="faint">Aucun dossier pour l’instant (facultatif).</li>
            {/each}
          </ul>
          <button class="btn" onclick={addRepo}><Icon name="plus" size={14} />Ajouter un dossier…</button>

        {:else if STEPS[step].id === 'connectors'}
          <h1>Connecteurs</h1>
          <p class="lead">Serveurs MCP de votre agent (Jira, Datadog…). Seuls ceux dont l’écriture peut être bloquée sont proposés.</p>
          <ul class="repos">
            {#each blockable as c (c.id)}
              <li>
                <label class="check grow"><input type="checkbox" checked={c.use_live || c.use_correlation} onchange={(e) => setConnector(c, e.currentTarget.checked)} /><strong>{c.name}</strong><span class="faint small">{c.status}</span></label>
              </li>
            {:else}
              <li class="faint">{refreshing ? 'Détection…' : 'Aucun connecteur activable.'}</li>
            {/each}
          </ul>
          {#if hiddenCount}<p class="faint small">{hiddenCount} connecteur{hiddenCount > 1 ? 's' : ''} masqué{hiddenCount > 1 ? 's' : ''} : l’agent ne sait pas bloquer leurs outils d’écriture.</p>{/if}
          <button class="btn" onclick={refreshConnectors} disabled={refreshing}><Icon name="refresh" size={14} />Rafraîchir</button>

        {:else if STEPS[step].id === 'wake'}
          <h1>Mot d’appel</h1>
          <p class="lead">Pendant l’écoute, dites « <strong>{settings.voice.wake_word || '…'}</strong>, cherche… » pour lancer une recherche, et « <strong>{settings.voice.wake_word || '…'}</strong>, arrête l’écoute » pour terminer la réunion. Seul votre micro peut le déclencher.</p>
          <div class="row">
            <input class="input lg wake" bind:value={settings.voice.wake_word} aria-label="Mot d’appel" />
            <button class="btn" onclick={openWakeTest} disabled={!!wake?.errors.length}><Icon name="mic" size={14} />Tester…</button>
          </div>
          {#if wake}
            <div class="msgs">
              {#each wake.errors as e}<p class="errc small">✕ {e}</p>{/each}
              {#each wake.warnings as w}<p class="warnc small">⚠ {w}</p>{/each}
              {#if !wake.errors.length && !wake.warnings.length}<p class="okc small">✓ Aucun risque repéré.</p>{/if}
            </div>
          {/if}
          <label class="check"><input type="checkbox" bind:checked={settings.voice.strict} /> Mode strict (exiger « OK {settings.voice.wake_word} »)</label>

        {:else}
          <h1>Corrélation</h1>
          <p class="lead">Après le compte rendu, le Scribe peut le croiser avec le code et les connecteurs pour repérer les contradictions.</p>
          <div class="choices" role="radiogroup" aria-label="Mode de corrélation">
            {#each [['off', 'Désactivée', 'Jamais.'], ['on_demand', 'À la demande', 'Bouton « Corréler » sur chaque compte rendu. Recommandé.'], ['auto', 'Automatique', 'Après chaque réunion. Consomme davantage de quota.']] as [v, l, h]}
              <label class="choice" class:sel={settings.correlation.mode === v}>
                <input type="radio" name="ob-corr" value={v} bind:group={settings.correlation.mode} />
                <span class="grow"><strong>{l}</strong><span class="faint small block">{h}</span></span>
              </label>
            {/each}
          </div>
        {/if}

        <div class="nav row">
          {#if step > 0}<button class="btn" onclick={() => step--}>Précédent</button>{/if}
          <span class="grow"></span>
          <button class="btn ghost" onclick={() => navigate('/meetings')}>Passer</button>
          <button class="btn primary" onclick={next} disabled={saving}>{step === STEPS.length - 1 ? 'Terminer' : 'Suivant'}</button>
        </div>
      </div>
    {/if}
  </main>
</div>

{#if wakeTesting && settings}
  <WakeTest bind:settings onclose={() => (wakeTesting = false)} />
{/if}

<style>
  .ob { display: grid; grid-template-columns: 250px 1fr; height: 100%; }
  .rail { background: var(--side-bg); color: var(--side-text); padding: 28px 18px 20px; display: flex; flex-direction: column; gap: 26px; }
  .brand { display: flex; align-items: center; gap: 12px; }
  .brand strong { font-size: 16px; }
  .brand p { color: var(--side-text-2); }
  .steps { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 2px; flex: 1; }
  .steps button {
    all: unset;
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    box-sizing: border-box;
    padding: 7px 8px;
    border-radius: 7px;
    color: var(--side-text-2);
    font-weight: 500;
  }
  .steps button:not(:disabled):hover { background: var(--side-hover); }
  .steps button:focus-visible { outline: 2px solid #9fb4ea; }
  .steps .cur button { background: var(--side-active); color: #fff; }
  .steps .done button { color: var(--side-text); }
  .num { width: 20px; height: 20px; border-radius: 50%; display: grid; place-items: center; font-size: 11px; border: 1px solid rgba(255, 255, 255, 0.25); flex: none; }
  .cur .num { background: #f1f0ec; color: #1c202b; border-color: #f1f0ec; }
  .done .num { background: rgba(255, 255, 255, 0.14); border-color: transparent; }
  .note { color: var(--side-text-2); }
  .pane { background: var(--surface); }
  .inner { max-width: 600px; padding: 56px 48px 40px; display: flex; flex-direction: column; gap: 16px; }
  .inner h1 { font-size: 24px; margin-top: -8px; }
  .lead { color: var(--text-2); font-size: 14px; line-height: 1.55; }
  .doctor, .repos { list-style: none; margin: 0; padding: 0; border: 1px solid var(--line); border-radius: var(--radius-lg); }
  .doctor li, .repos li { display: flex; align-items: flex-start; gap: 10px; padding: 9px 12px; border-top: 1px solid var(--line); }
  .repos li { align-items: center; }
  .doctor li:first-child, .repos li:first-child { border-top: 0; }
  .st { width: 20px; height: 20px; border-radius: 50%; display: grid; place-items: center; flex: none; }
  .ok .st { background: var(--ok-soft); color: var(--ok); }
  .warn .st { background: var(--warn-soft); color: var(--warn); }
  .error .st { background: var(--err-soft); color: var(--err); }
  .fix { margin-top: 3px; color: var(--text-2); }
  .choices { display: flex; flex-direction: column; border: 1px solid var(--line); border-radius: var(--radius-lg); overflow: hidden; }
  .choice { display: flex; align-items: center; gap: 10px; padding: 10px 12px; border-top: 1px solid var(--line); }
  .choice:first-child { border-top: 0; }
  .choice.sel { background: var(--selected); }
  .choice.off { opacity: 0.55; }
  .block { display: block; }
  .wake { width: 240px; font-weight: 600; }
  .msgs { display: flex; flex-direction: column; gap: 3px; }
  .okc { color: var(--ok); }
  .errc { color: var(--err); }
  .warnc { color: var(--warn); }
  .nav { margin-top: 18px; padding-top: 16px; border-top: 1px solid var(--line); gap: 8px; }
</style>
