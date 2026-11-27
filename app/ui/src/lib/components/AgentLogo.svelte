<script lang="ts" module>
  // Logos des CLI d'agent (src/assets/agents, lobe-icons, MIT). SVG du dépôt, inclus dans le
  // paquet : aucune requête réseau. Repli pour `custom` (icône terminal) et les identifiants
  // inconnus (pastille avec l'initiale).
  import claudeCode from '../../assets/agents/claude-code.svg?raw';
  import codex from '../../assets/agents/codex.svg?raw';
  import antigravity from '../../assets/agents/antigravity.svg?raw';
  import grok from '../../assets/agents/grok.svg?raw';
  import gemini from '../../assets/agents/gemini.svg?raw';
  import opencode from '../../assets/agents/opencode.svg?raw';

  const LOGOS: Record<string, string> = { 'claude-code': claudeCode, codex, antigravity, grok, gemini, opencode };
  /** Logos en `currentColor` : suivent la couleur du texte. */
  const MONO = new Set(['grok', 'opencode']);

  /** L'agent a un logo dans le dépôt. */
  export function hasAgentLogo(id: string | null | undefined): boolean {
    return !!id && Object.prototype.hasOwnProperty.call(LOGOS, id);
  }

  /** Initiale de repli : première lettre ou chiffre du nom (ou de l'identifiant), en majuscule. */
  export function agentInitial(label: string | null | undefined): string {
    const m = (label ?? '').normalize('NFD').replace(/[̀-ͯ]/g, '').match(/[A-Za-z0-9]/);
    return m ? m[0].toUpperCase() : '?';
  }

  let seq = 0;
</script>

<script lang="ts">
  import Icon from './Icon.svelte';

  let { id, size = 16, name = '' }: { id: string; size?: number; name?: string } = $props();

  // Identifiants internes (masques, dégradés) rendus uniques : un même logo peut apparaître
  // plusieurs fois dans la page.
  const uid = `agl${++seq}`;
  const svg = $derived(
    hasAgentLogo(id)
      ? LOGOS[id].replaceAll('lobe-icons-', `${uid}-`).replace(/<title>[^<]*<\/title>/, '')
      : '',
  );
</script>

{#if svg}
  <span class="alogo" class:mono={MONO.has(id)} class:color={!MONO.has(id)} style:--al-size="{size}px" aria-hidden="true" data-agent={id}>
    <!-- SVG du dépôt, vérifié : aucun script ni lien externe. -->
    {@html svg}
  </span>
{:else}
  <span class="alogo fallback" style:--al-size="{size}px" aria-hidden="true" data-agent={id} data-fallback={id === 'custom' ? 'terminal' : 'initial'}>
    {#if id === 'custom'}<Icon name="terminal" size={Math.round(size * 0.8)} stroke={1.8} />{:else}{agentInitial(name || id)}{/if}
  </span>
{/if}

<style>
  .alogo {
    display: inline-grid;
    place-items: center;
    flex: none;
    width: var(--al-size);
    height: var(--al-size);
    vertical-align: -0.22em;
    line-height: 1;
  }
  .alogo :global(svg) { width: 100%; height: 100%; display: block; }
  .mono { color: var(--text); }
  .fallback {
    border-radius: 5px;
    background: var(--surface-2);
    color: var(--text-2);
    box-shadow: inset 0 0 0 1px var(--line);
    font-size: calc(var(--al-size) * 0.6);
    font-weight: 650;
  }
  /* Sombre : les logos en couleur gardent un léger fond arrondi pour ne pas se perdre. */
  @media (prefers-color-scheme: dark) {
    .color {
      border-radius: 5px;
      background: rgba(255, 255, 255, 0.07);
      box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.08);
      padding: 1px;
      box-sizing: border-box;
    }
  }
</style>
