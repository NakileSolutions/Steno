<script lang="ts">
  // Préparation guidée : l'agent pose quelques questions, puis rédige le contexte de la
  // réunion à partir des réponses.
  import { api } from '../api';
  import type { MeetingContext } from '../types';
  import { toast, toastError } from '../ui.svelte';
  import { t } from '../i18n.svelte';
  import Icon from './Icon.svelte';

  let { meetingId, ondone }: { meetingId: string; ondone?: (c: MeetingContext) => void } = $props();

  let step = $state<'idle' | 'asking' | 'answering' | 'writing'>('idle');
  let questions = $state<string[]>([]);
  let answers = $state<string[]>([]);

  async function start() {
    step = 'asking';
    try {
      questions = await api.prepareQuestions(meetingId);
      answers = questions.map(() => '');
      step = 'answering';
    } catch (e) {
      step = 'idle';
      toastError(e);
    }
  }

  async function finish() {
    step = 'writing';
    try {
      const c = await api.prepareApply(meetingId, questions.map((q, i) => ({ question: q, answer: answers[i] ?? '' })));
      step = 'idle';
      questions = [];
      toast(t('Contexte rédigé : il servira au compte rendu.'));
      ondone?.(c);
    } catch (e) {
      step = 'answering';
      toastError(e);
    }
  }
</script>

<div class="prep">
  {#if step === 'idle'}
    <button class="btn primary" onclick={start}><Icon name="ask" size={14} />{t('Préparer avec l’IA')}</button>
    <p class="faint small">{t('Quelques questions pour cerner l’objectif, les décisions attendues et les points à surveiller.')}</p>
  {:else if step === 'asking'}
    <p class="muted row gap"><span class="spin" aria-hidden="true"></span>{t('L’IA prépare ses questions…')}</p>
  {:else}
    <ol class="qs">
      {#each questions as q, i (i)}
        <li>
          <label for="prep-{i}">{q}</label>
          <textarea id="prep-{i}" class="input" rows="2" bind:value={answers[i]} disabled={step === 'writing'}></textarea>
        </li>
      {/each}
    </ol>
    <div class="row end">
      <button class="btn ghost" onclick={() => (step = 'idle')} disabled={step === 'writing'}>{t('Annuler')}</button>
      <button class="btn primary" onclick={finish} disabled={step === 'writing' || !answers.some((a) => a.trim())}>
        {#if step === 'writing'}<span class="spin" aria-hidden="true"></span>{t('Rédaction…')}{:else}{t('Rédiger le contexte')}{/if}
      </button>
    </div>
  {/if}
</div>

<style>
  .prep { display: flex; flex-direction: column; gap: 10px; align-items: flex-start; }
  .qs { margin: 0; padding-left: 20px; display: flex; flex-direction: column; gap: 12px; width: 100%; }
  .qs li { display: flex; flex-direction: column; gap: 6px; }
  .qs label { font-weight: 550; }
  .qs textarea { resize: vertical; font: inherit; line-height: 1.5; }
  .end { justify-content: flex-end; gap: 6px; width: 100%; }
  .gap { gap: 8px; align-items: center; }
  .spin { width: 12px; height: 12px; border: 1.5px solid currentColor; border-right-color: transparent; border-radius: 50%; animation: spin 0.8s linear infinite; display: inline-block; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
