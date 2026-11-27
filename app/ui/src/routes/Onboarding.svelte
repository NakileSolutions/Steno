<script lang="ts">
  // Assistant de premier lancement (PLAN §14) : 6 étapes.
  import { onMount } from 'svelte';
  import { api } from '../lib/api';
  import { setPrefs } from '../lib/prefs.svelte';
  import { navigate } from '../lib/router.svelte';
  import type { AgentInfo, Connector, DoctorItem, Repo, Settings, TestAgentResult, WakeCheck } from '../lib/types';
  import { debounce, stripLeadingIcon } from '../lib/format';
  import { toast, toastError } from '../lib/ui.svelte';
  import Logo from '../lib/Logo.svelte';
  import Icon from '../lib/components/Icon.svelte';
  import AgentLogo from '../lib/components/AgentLogo.svelte';
  import { sortAgents } from '../lib/agents';
  import WakeTest from './settings/WakeTest.svelte';

  const STEPS = [
    { id: 'permissions', label: 'Permissions macOS', icon: 'shield' },
    { id: 'agent', label: 'Agent IA', icon: 'agent' },
    { id: 'repos', label: 'Dépôts', icon: 'folder' },
    { id: 'connectors', label: 'Connecteurs', icon: 'plug' },
    { id: 'wake', label: 'Mot d’appel', icon: 'mic' },
    { id: 'correlation', label: 'Corrélation', icon: 'link2' },
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

  // Claude Code et Codex toujours proposés ; Antigravity, Grok et les autres s'ils sont
  // détectés (ou déjà choisis). Ordre : agents mis en avant d'abord.
  const ALWAYS = ['claude-code', 'codex'];
  const obAgents = $derived(sortAgents(agents).filter((a) => ALWAYS.includes(a.id) || a.installed || a.id === settings?.agent.provider));

  async function modelsLater(e: MouseEvent) {
    e.preventDefault();
    try {
      await persist();
    } catch {
      // Rien de bloquant : on ouvre quand même les Réglages.
    }
    navigate('/settings?section=agent');
  }

  const blockable = $derived(connectors.filter((c) => c.write_blockable));
  const hiddenCount = $derived(connectors.length - blockable.length);
  const STATUS_ICON: Record<string, string> = { ok: 'check', warn: 'warn', error: 'x' };
</script>

<div class="ob">
  <aside class="rail">
    <div class="brand"><span class="btile"><Logo size={40} tile /></span><div><strong>Scribe</strong><p class="tiny">Premier lancement</p></div></div>
    <div class="prog" role="progressbar" aria-label="Progression" aria-valuemin={1} aria-valuemax={STEPS.length} aria-valuenow={step + 1}>
      <span style="width: {((step + 1) / STEPS.length) * 100}%"></span>
    </div>
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
        {#key step}<span class="illus" aria-hidden="true"><Icon name={STEPS[step].icon} size={24} stroke={1.6} /></span>{/key}
        <p class="label">Étape {step + 1} sur {STEPS.length}</p>

        {#if STEPS[step].id === 'permissions'}
          <h1>Permissions macOS</h1>
          <p class="lead">Le Scribe a besoin du micro, de l’audio du système (pour entendre l’app de l’appel : Teams, Slack, Zoom…) et de l’accessibilité (pour lire le titre de l’appel). Il n’écoute que lorsque vous démarrez l’écoute.</p>
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
              <li class="faint">{#if doctoring}<span class="skeleton" style="width: 60%"></span>{:else}Aucun résultat.{/if}</li>
            {/each}
          </ul>
          <button class="btn" onclick={runDoctor} disabled={doctoring}><Icon name="refresh" size={14} />{doctoring ? 'Vérification…' : 'Vérifier à nouveau'}</button>

        {:else if STEPS[step].id === 'agent'}
          <h1>Agent IA</h1>
          <p class="lead">Choisissez le CLI d’agent déjà connecté à votre abonnement. Le Scribe ne l’appelle que sur demande et en fin de réunion.</p>
          <div class="choices" role="radiogroup" aria-label="Agent IA">
            {#each obAgents as a (a.id)}
              <label class="choice" class:sel={settings.agent.provider === a.id} class:off={!a.installed}>
                <input type="radio" name="ob-agent" value={a.id} bind:group={settings.agent.provider} disabled={!a.installed} />
                <AgentLogo id={a.id} name={a.name} size={20} />
                <span class="grow"><strong>{a.name}</strong> <span class="faint small">{a.installed ? (a.version ?? 'installé') : (a.error ?? 'non installé')}</span></span>
                <span class="badge {a.support === 'supported' ? 'ok' : a.support === 'experimental' ? 'warn' : ''}">{a.support === 'supported' ? 'Supporté' : a.support === 'experimental' ? 'Expérimental' : 'Générique'}</span>
              </label>
            {/each}
          </div>
          <div class="row">
            <button class="btn" onclick={testAgent} disabled={testing}>{testing ? 'Test en cours…' : 'Tester l’agent'}</button>
            {#if testResult}<span class="small res" class:okc={testResult.ok} class:errc={!testResult.ok}><Icon name={testResult.ok ? 'check' : 'x'} size={13} stroke={2} />{testResult.message}</span>{/if}
          </div>
          <p class="small faint later"><Icon name="info" size={13} /><span>Modèle et niveau de réflexion se règlent pour les recherches, le compte rendu et les notes. <a href="#/settings?section=agent" onclick={modelsLater}>Choisir les modèles plus tard dans Réglages</a></span></p>

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
                <label class="check toggle grow"><span><Icon name="plug" size={14} /> <strong>{c.name}</strong> <span class="faint small">{c.status}</span></span><input class="switch" type="checkbox" role="switch" checked={c.use_live || c.use_correlation} onchange={(e) => setConnector(c, e.currentTarget.checked)} /></label>
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
              {#each wake.errors as e}<p class="errc small">✕ {stripLeadingIcon(e)}</p>{/each}
              {#each wake.warnings as w}<p class="warnc small">⚠ {stripLeadingIcon(w)}</p>{/each}
              {#if !wake.errors.length && !wake.warnings.length}<p class="okc small">✓ Aucun risque repéré.</p>{/if}
            </div>
          {/if}
          <label class="check strict"><input class="switch" type="checkbox" role="switch" bind:checked={settings.voice.strict} /> Mode strict (exiger « OK {settings.voice.wake_word} »)</label>

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
          <button class="btn primary nextbtn" onclick={next} disabled={saving}>{step === STEPS.length - 1 ? 'Terminer' : 'Suivant'}{#if step < STEPS.length - 1}<Icon name="chevron" size={14} stroke={2} />{/if}</button>
        </div>
      </div>
    {/if}
  </main>
</div>

{#if wakeTesting && settings}
  <WakeTest bind:settings onclose={() => (wakeTesting = false)} />
{/if}

<style>
  .ob { display: grid; grid-template-columns: 264px 1fr; height: 100%; background: var(--side-bg); }
  .rail { color: var(--side-text); padding: 32px 18px 22px; display: flex; flex-direction: column; gap: 22px; }
  .brand { display: flex; align-items: center; gap: 12px; padding: 0 6px; }
  .btile { display: grid; border-radius: 10px; box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.08), 0 4px 14px rgba(0, 0, 0, 0.3); }
  .brand strong { font-size: 16px; color: #f3f4f8; }
  .brand p { color: var(--side-text-2); }
  .prog { height: 4px; margin: 0 6px; border-radius: 2px; background: rgba(255, 255, 255, 0.08); overflow: hidden; }
  .prog span { display: block; height: 100%; border-radius: 2px; background: linear-gradient(90deg, #7d89ff, #a9b2ff); transition: width 240ms var(--ease); }
  .steps { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 2px; flex: 1; }
  .steps button {
    all: unset;
    display: flex;
    align-items: center;
    gap: 11px;
    width: 100%;
    box-sizing: border-box;
    height: 36px;
    padding: 0 10px;
    border-radius: 9px;
    color: var(--side-text-3);
    font-weight: 500;
    transition: background var(--t-fast) var(--ease), color var(--t-fast) var(--ease);
  }
  .steps button:not(:disabled):hover { background: var(--side-hover); }
  .steps button:focus-visible { outline: 2px solid var(--side-accent); }
  .steps .cur button { background: var(--side-active); color: #fff; box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.04); }
  .steps .done button { color: var(--side-text); }
  .num { width: 22px; height: 22px; border-radius: 50%; display: grid; place-items: center; font-size: 11px; font-weight: 600; box-shadow: inset 0 0 0 1.5px rgba(255, 255, 255, 0.18); flex: none; font-variant-numeric: tabular-nums; transition: background var(--t) var(--ease); }
  .cur .num { background: #7d89ff; color: #fff; box-shadow: 0 0 0 4px rgba(125, 137, 255, 0.2); }
  .done .num { background: rgba(125, 137, 255, 0.22); color: #c9ceff; box-shadow: none; }
  .note { color: var(--side-text-3); padding: 0 6px; line-height: 1.5; }
  .pane { background: var(--surface); margin: 8px 8px 8px 0; border-radius: 12px; box-shadow: var(--content-ring); }
  .inner { max-width: 640px; padding: 64px 56px 48px; display: flex; flex-direction: column; gap: 16px; }
  .illus {
    width: 52px;
    height: 52px;
    border-radius: 15px;
    display: grid;
    place-items: center;
    color: var(--accent-text);
    background: linear-gradient(180deg, var(--accent-soft-2), var(--accent-soft));
    box-shadow: inset 0 0 0 1px var(--accent-line);
    margin-bottom: 4px;
    animation: appear 200ms var(--ease);
  }
  .inner h1 { font-size: 26px; margin-top: -8px; }
  .lead { color: var(--text-2); font-size: 14.5px; line-height: 1.6; }
  .doctor, .repos { list-style: none; margin: 0; padding: 4px 0; border: 1px solid var(--line); border-radius: var(--radius-lg); background: var(--surface); box-shadow: var(--shadow-sm); }
  .doctor li, .repos li { display: flex; align-items: flex-start; gap: 12px; padding: 10px 14px; border-top: 1px solid var(--line); animation: appear var(--t) var(--ease) both; }
  .repos li { align-items: center; }
  .doctor li:first-child, .repos li:first-child { border-top: 0; }
  .doctor p { line-height: 1.45; }
  .st { width: 22px; height: 22px; border-radius: 50%; display: grid; place-items: center; flex: none; }
  .ok .st { background: var(--ok-soft); color: var(--ok); }
  .warn .st { background: var(--warn-soft); color: var(--warn); }
  .error .st { background: var(--err-soft); color: var(--err); }
  .fix { margin-top: 3px; color: var(--text-2); }
  .choices { display: flex; flex-direction: column; gap: 6px; }
  .choice { display: flex; align-items: center; gap: 12px; padding: 12px 14px; border: 1px solid var(--line); border-radius: 10px; transition: border-color var(--t-fast) var(--ease), background var(--t-fast) var(--ease); }
  .choice:hover:not(.off) { border-color: var(--line-strong); }
  .choice.sel { background: var(--accent-soft); border-color: var(--accent-line); box-shadow: 0 0 0 1px var(--accent-line); }
  .choice.off { opacity: 0.55; }
  .block { display: block; margin-top: 2px; }
  .wake { width: 260px; font-weight: 600; }
  .msgs { display: flex; flex-direction: column; gap: 3px; }
  .res { display: inline-flex; align-items: center; gap: 6px; }
  .okc { color: var(--ok); }
  .errc { color: var(--err); }
  .warnc { color: var(--warn); }
  .strict { gap: 10px; }
  .later { display: flex; align-items: flex-start; gap: 7px; line-height: 1.5; }
  .later :global(.icon) { margin-top: 3px; flex: none; }
  .later a { color: var(--accent-text); font-weight: 500; }
  .inner > :global(.btn) { align-self: flex-start; }
  .nav { margin-top: 20px; padding-top: 18px; border-top: 1px solid var(--line); gap: 8px; }
  .nextbtn { min-width: 112px; gap: 4px; }
</style>
