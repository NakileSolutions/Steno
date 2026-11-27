<script lang="ts">
  // Réunion prévue : démarrage de l'écoute, participants, préparation guidée et contexte.
  import { api } from '../../lib/api';
  import type { MeetingDetail } from '../../lib/types';
  import { fullDate, time } from '../../lib/format';
  import { toastError } from '../../lib/ui.svelte';
  import { navigate } from '../../lib/router.svelte';
  import { t } from '../../lib/i18n.svelte';
  import Icon from '../../lib/components/Icon.svelte';
  import ContextEditor from '../../lib/components/ContextEditor.svelte';
  import ParticipantsEditor from '../../lib/components/ParticipantsEditor.svelte';
  import PrepAssistant from '../../lib/components/PrepAssistant.svelte';

  let { detail }: { detail: MeetingDetail } = $props();
  const m = $derived(detail.meeting);
  let busy = $state(false);
  let ctxKey = $state(0);

  async function start(inPerson: boolean) {
    busy = true;
    try {
      await api.startPlanned(m.id, inPerson);
      navigate('/live');
    } catch (e) {
      toastError(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="planned">
  <section class="card hero">
    <div class="grow">
      <p class="label">{t('Réunion prévue')}</p>
      <p class="when"><Icon name="calendar" size={14} />{fullDate(m.started_at)} · <span class="tnum">{time(m.started_at)}</span></p>
      <p class="faint small">{t('Démarrer l’écoute à l’heure prévue reprend automatiquement cette réunion, avec son contexte et ses participants.')}</p>
    </div>
    <div class="col acts">
      <button class="btn primary" onclick={() => start(false)} disabled={busy}><Icon name="play" size={14} />{t('Démarrer l’écoute')}</button>
      <button class="btn ghost sm" onclick={() => start(true)} disabled={busy}><Icon name="user" size={13} />{t('En présentiel')}</button>
    </div>
  </section>

  <section>
    <h2 class="bh"><Icon name="ask" size={15} />{t('Préparation')}</h2>
    <PrepAssistant meetingId={m.id} ondone={() => ctxKey++} />
  </section>

  <section>
    <h2 class="bh"><Icon name="users" size={15} />{t('Participants')}</h2>
    <ParticipantsEditor meetingId={m.id} />
  </section>

  <section>
    <h2 class="bh"><Icon name="paperclip" size={15} />{t('Contexte, documents et format')}</h2>
    {#key ctxKey}<ContextEditor meetingId={m.id} />{/key}
  </section>
</div>

<style>
  .planned { padding: 8px 32px 56px; max-width: calc(72ch + 64px); display: flex; flex-direction: column; gap: 26px; }
  .hero { display: flex; gap: 16px; align-items: center; padding: 16px 18px; }
  .when { display: flex; align-items: center; gap: 7px; font-size: var(--fs-h3); font-weight: 600; margin: 4px 0 6px; }
  .col { display: flex; flex-direction: column; gap: 6px; align-items: stretch; }
  .bh { display: flex; align-items: center; gap: 8px; font-size: var(--fs-h3); margin-bottom: 10px; }
</style>
