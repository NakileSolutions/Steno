<script lang="ts">
  // Carte « Micro » : micro par défaut du système ou un micro précis (audio.input_device).
  import { onMount } from 'svelte';
  import { api } from '../../lib/api';
  import { TRANSPORT_LABEL, defaultInputLabel, isDeviceMissing, missingInputLabel, transportIcon } from '../../lib/audio';
  import type { AudioDevice, Settings } from '../../lib/types';
  import Icon from '../../lib/components/Icon.svelte';

  let { settings = $bindable() }: { settings: Settings } = $props();
  let inputs = $state<AudioDevice[]>([]);
  let loading = $state(true);
  let error = $state('');

  async function refresh() {
    loading = true;
    error = '';
    try {
      inputs = (await api.listAudioDevices()).inputs;
    } catch (e) {
      inputs = [];
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }
  onMount(refresh);

  const empty = $derived(!loading && !inputs.length);
  const missing = $derived(isDeviceMissing(settings.audio.input_device, inputs));

  function choose(d: AudioDevice | null) {
    settings.audio.input_device = d?.id ?? null;
    settings.audio.input_device_name = d?.name ?? null;
  }
</script>

<h2 class="group-title">Micro</h2>
<div class="mic" role="radiogroup" aria-label="Micro" aria-disabled={empty}>
  <label class="mrow" class:sel={!settings.audio.input_device}>
    <input type="radio" name="mic" checked={!settings.audio.input_device} onchange={() => choose(null)} disabled={empty} />
    <span class="mic-ic" aria-hidden="true"><Icon name="mic" size={14} /></span>
    <span class="grow">{defaultInputLabel(inputs)}</span>
    <span class="tiny faint">suit les changements du système</span>
  </label>
  {#if missing}
    <label class="mrow sel gone">
      <input type="radio" name="mic" checked disabled />
      <span class="mic-ic" aria-hidden="true"><Icon name="warn" size={14} /></span>
      <span class="grow" role="status">{missingInputLabel(settings.audio.input_device_name, settings.audio.input_device)}</span>
    </label>
  {/if}
  {#each inputs as d (d.id)}
    <label class="mrow" class:sel={settings.audio.input_device === d.id} data-device={d.id}>
      <input type="radio" name="mic" checked={settings.audio.input_device === d.id} onchange={() => choose(d)} />
      <span class="mic-ic" aria-hidden="true"><Icon name={transportIcon(d.transport)} size={14} /></span>
      <span class="grow">{d.name}</span>
      <span class="tiny faint">{TRANSPORT_LABEL[d.transport] ?? d.transport}{d.default ? ' · par défaut' : ''}</span>
    </label>
  {/each}
  {#if empty}
    <p class="mrow faint small">{error || 'Liste disponible dans l’app sur Mac.'}</p>
  {/if}
  <div class="mrow foot">
    <span class="grow tiny faint">Appliqué dès l’enregistrement, même pendant l’écoute, sans couper la transcription.</span>
    <button class="btn sm" type="button" onclick={refresh} disabled={loading}><Icon name="refresh" size={13} />Rafraîchir</button>
  </div>
</div>

<style>
  .group-title { margin: 24px 0 8px 4px; font-size: var(--fs-meta); font-weight: 600; color: var(--text-2); letter-spacing: 0.01em; }
  .mic { background: var(--surface); border: 1px solid var(--line); border-radius: var(--radius-lg); box-shadow: var(--shadow-sm); overflow: hidden; }
  .mrow { display: flex; align-items: center; gap: 12px; padding: 10px 16px; position: relative; margin: 0; }
  .mrow + .mrow::before { content: ''; position: absolute; top: 0; left: 16px; right: 16px; border-top: 1px solid var(--line); }
  .mrow.sel { background: var(--selected); }
  .mic-ic { width: 26px; height: 26px; border-radius: 7px; display: grid; place-items: center; flex: none; background: var(--surface-2); color: var(--text-2); }
  .sel .mic-ic { background: var(--accent-soft); color: var(--accent-text); }
  .gone { color: var(--warn); background: color-mix(in srgb, var(--warn-soft) 55%, transparent); }
  .gone .mic-ic { background: var(--warn-soft); color: var(--warn); }
  .foot { padding: 8px 16px; }
</style>
