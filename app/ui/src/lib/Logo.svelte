<script lang="ts">
  // Logo Scribe : bec de plume au trait, onde sonore au centre (d'après assets/logo.jpg).
  // `tile` ajoute le fond bleu nuit dégradé de l'icône d'app.
  let { size = 24, tile = false, title = 'Scribe' }: { size?: number; tile?: boolean; title?: string } = $props();
  const uid = Math.random().toString(36).slice(2, 8);
</script>

<svg
  width={size}
  height={size}
  viewBox={tile ? '0 0 100 100' : '24 16 52 68'}
  role="img"
  aria-label={title}
  class="logo"
  style={tile ? '' : 'color: currentColor'}
>
  {#if tile}
    <defs>
      <linearGradient id="lg-{uid}" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stop-color="#2b3140" />
        <stop offset="1" stop-color="#1c202b" />
      </linearGradient>
    </defs>
    <rect x="0" y="0" width="100" height="100" rx="22" fill="url(#lg-{uid})" />
  {/if}
  <g
    fill="none"
    stroke={tile ? '#f1f0ec' : 'currentColor'}
    stroke-width={tile ? 1.4 : 2.4}
    stroke-linecap="round"
    stroke-linejoin="round"
  >
    <path d="M50 20.4 L68.5 51.8 C64 58 59.5 68 59.5 79.7 L40.5 79.7 C40.5 68 36 58 31.5 51.8 Z" />
    <path d="M50 20.4 V47.9 M50 52.6 V69.4" />
    <circle cx="50" cy="50.25" r="2.35" />
    <path d="M38.5 48.4 V53.8 M41.8 43.7 V58.5 M45.2 38 V64.2 M54.8 38 V64.2 M58.2 43.7 V58.5 M61.5 48.4 V53.8" />
  </g>
</svg>

<style>
  .logo {
    display: block;
    flex: none;
  }
</style>
