<script lang="ts">
  // Forme d'onde décorative de l'en-tête « En direct » : barres fines qui suivent les niveaux audio
  // (micro et visio, dBFS). Au repos, une onde calme. Sans animation si l'utilisateur la refuse.
  import { onMount } from 'svelte';

  let { me, them, active = true, bars = 120 }: { me: number; them: number; active?: boolean; bars?: number } = $props();

  const N = $derived(Math.max(12, bars));
  let els: HTMLSpanElement[] = $state([]);
  const lvl = (db: number) => Math.min(1, Math.max(0, (db + 60) / 52));
  const clamp = (v: number) => Math.min(1, Math.max(0.08, v));
  /** Enveloppe : bords effilés. */
  const env = (x: number) => Math.pow(Math.sin(Math.PI * (0.04 + x * 0.92)), 0.7);

  let reduced = $state(false);

  /** Hauteurs statiques (mouvement réduit) : proportionnelles au niveau, sans animation. */
  const still = $derived.by(() => {
    const a = active ? Math.max(lvl(me), lvl(them)) : 0;
    return Array.from({ length: N }, (_, i) => {
      const x = i / (N - 1);
      return clamp(env(x) * (0.26 + 0.15 * Math.sin(x * 7.5) + a * 0.7 * (0.6 + 0.4 * Math.sin(i * 1.7))));
    });
  });

  onMount(() => {
    const mq = matchMedia('(prefers-reduced-motion: reduce)');
    reduced = mq.matches;
    const onmq = () => (reduced = mq.matches);
    mq.addEventListener?.('change', onmq);

    let raf = 0;
    let last = 0;
    let amp = 0;
    let ampMe = 0;
    let ampThem = 0;
    const t0 = performance.now();
    const frame = (now: number) => {
      raf = requestAnimationFrame(frame);
      if (reduced || now - last < 33) return;
      last = now;
      const tm = active ? lvl(me) : 0;
      const tt = active ? lvl(them) : 0;
      // Attaque rapide, relâchement doux.
      ampMe += (tm - ampMe) * (tm > ampMe ? 0.45 : 0.1);
      ampThem += (tt - ampThem) * (tt > ampThem ? 0.45 : 0.1);
      amp = Math.max(ampMe, ampThem);
      const t = (now - t0) / 1000;
      const n = els.length;
      for (let i = 0; i < n; i++) {
        const el = els[i];
        if (!el) continue;
        const x = i / (n - 1);
        // Onde calme : deux sinusoïdes lentes qui glissent.
        const calm = 0.26 + 0.15 * Math.sin(t * 1.3 + x * 7.5) + 0.07 * Math.sin(t * 0.8 - x * 15);
        // Parole : agitation rapide, différente pour chaque barre ; « eux » à droite, « moi » à gauche.
        const jitter = 0.5 + 0.5 * Math.sin(t * 9.1 + i * 1.93) * Math.sin(t * 4.3 + i * 0.71);
        const side = ampMe * (1 - x) + ampThem * x;
        const speech = (0.55 * amp + 0.45 * side) * (0.3 + 0.7 * jitter);
        el.style.transform = `scaleY(${clamp(env(x) * (calm + speech * 0.95)).toFixed(3)})`;
      }
    };
    raf = requestAnimationFrame(frame);
    return () => {
      cancelAnimationFrame(raf);
      mq.removeEventListener?.('change', onmq);
    };
  });
</script>

<div class="wave" class:active aria-hidden="true">
  {#each still as h, i (i)}
    <span bind:this={els[i]} style={reduced ? `transform: scaleY(${h})` : undefined}></span>
  {/each}
</div>

<style>
  .wave {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 2px;
    height: 44px;
    min-width: 0;
    flex: 1;
    -webkit-mask-image: linear-gradient(90deg, transparent, #000 6%, #000 94%, transparent);
    mask-image: linear-gradient(90deg, transparent, #000 6%, #000 94%, transparent);
  }
  span {
    flex: 0 1 2.5px;
    min-width: 1.5px;
    height: 100%;
    border-radius: 3px;
    transform: scaleY(0.12);
    transform-origin: center;
    background: color-mix(in srgb, var(--text) 22%, transparent);
    will-change: transform;
  }
  .active span {
    background: linear-gradient(180deg, #8f99ff 0%, var(--accent) 50%, #8f99ff 100%);
  }
</style>
