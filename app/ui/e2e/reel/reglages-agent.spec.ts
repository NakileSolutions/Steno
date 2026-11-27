// Réglages contre le cœur réel : modèles et réflexion par usage (agent.tiers, list_models),
// périphériques audio (list_audio_devices). Les réglages d'origine sont rétablis à la fin :
// le serveur est partagé avec le scénario complet.
import { expect, test } from '@playwright/test';
import { go } from './helpers';

test('agent.tiers, list_models et périphériques audio : contrat et interface', async ({ page }) => {
  const invoke = async (cmd: string, data: Record<string, unknown> = {}) => {
    const r = await page.request.post(`/api/invoke/${cmd}`, { data });
    expect(r.ok(), `${cmd} : ${await r.text()}`).toBe(true);
    return r.json();
  };
  const original = await invoke('get_settings');
  expect(original.agent.tiers).toEqual({
    search: { agent: '', model: '', effort: '' },
    summary: { agent: '', model: '', effort: '' },
    quick: { agent: '', model: '', effort: '' },
  });

  try {
    // Contrat : list_models (claude-code : alias et identifiants complets).
    const models = await invoke('list_models', { provider: 'claude-code' });
    expect(models.length).toBeGreaterThan(0);
    expect(models.map((m: { id: string }) => m.id)).toContain('opus');
    for (const m of models) expect(Object.keys(m).sort()).toEqual(['id', 'name', 'note']);

    // Contrat : detect_agents porte efforts et lists_models.
    const agents = await invoke('detect_agents');
    const grok = agents.find((a: { id: string }) => a.id === 'grok');
    expect(grok.efforts).toEqual(['low', 'medium', 'high', 'xhigh']);
    expect(grok.lists_models).toBe(true);

    // Contrat : périphériques du simulateur (3 micros, 2 sorties).
    const dev = await invoke('list_audio_devices');
    expect(dev.inputs).toHaveLength(3);
    expect(dev.outputs).toHaveLength(2);
    expect(dev.inputs.find((d: { default: boolean }) => d.default).name).toBe('Micro du MacBook Pro');

    // Aller-retour save_settings / get_settings.
    const next = structuredClone(original);
    next.agent.tiers = {
      search: { agent: 'grok', model: 'grok-4.7', effort: 'medium' },
      summary: { agent: 'claude-code', model: 'opus', effort: 'high' },
      quick: { agent: '', model: 'modele-libre', effort: '' },
    };
    next.audio.input_device = 'AirPodsPro-UID:input';
    next.audio.input_device_name = 'AirPods Pro';
    next.voice.feedback_device = 'BuiltInSpeakerDevice';
    await invoke('save_settings', { settings: next });
    const back = await invoke('get_settings');
    expect(back.agent.tiers).toEqual(next.agent.tiers);
    expect(back.agent.models).toEqual(original.agent.models);
    expect(back.audio.input_device).toBe('AirPodsPro-UID:input');
    expect(back.audio.input_device_name).toBe('AirPods Pro');
    expect(back.voice.feedback_device).toBe('BuiltInSpeakerDevice');

    // Interface : les valeurs enregistrées sont affichées.
    await go(page, '/settings?section=agent');
    const card = page.getByRole('group', { name: 'Modèles et réflexion' });
    const search = card.getByRole('group', { name: 'Recherches et corrélation' });
    await expect(search.getByRole('button', { name: 'Agent : Grok Build' })).toBeVisible();
    await expect(search.getByLabel('Modèle', { exact: true }).locator('option', { hasText: 'grok-4.7' })).toHaveCount(1);
    await expect(search.getByLabel('Modèle', { exact: true })).toHaveValue('grok-4.7');
    await expect(search.getByLabel('Réflexion')).toHaveValue('medium');
    const summary = card.getByRole('group', { name: 'Compte rendu' });
    await expect(summary.getByLabel('Modèle', { exact: true })).toHaveValue('opus');
    await expect(summary.getByLabel('Réflexion')).toHaveValue('high');
    await expect(card.getByRole('group', { name: 'Notes et récapitulatifs' }).getByLabel('Modèle', { exact: true })).toHaveValue('modele-libre');

    await go(page, '/settings?section=audio');
    const mic = page.getByRole('radiogroup', { name: 'Micro' });
    await expect(mic.getByText('Micro par défaut du système (Micro du MacBook Pro)')).toBeVisible();
    await expect(mic.locator('[data-device="AirPodsPro-UID:input"] input')).toBeChecked();
  } finally {
    await invoke('save_settings', { settings: original });
  }
});
