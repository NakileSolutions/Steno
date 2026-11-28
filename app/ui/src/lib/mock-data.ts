// Données d'exemple du faux backend (mode navigateur). Fictives, en français.

import type {
  Action, AgentInfo, AudioDevices, Connector, Decision, Draft, Meeting, ModelInfo, Note, Repo, Report, ReportRef,
  Request, Segment, Settings, Speaker,
} from './types';
import { DEFAULT_CONSENT_MESSAGE, defaultCallApps } from './listening';

export type MeetingStore = {
  meeting: Meeting;
  segments: Segment[];
  speakers: Speaker[];
  notes: Note[];
  requests: Request[];
  reports: Report[]; // toutes les versions, la dernière est la courante
  refs: ReportRef[];
  decisions: Decision[];
  actions: Action[];
  drafts: Draft[];
};

// ---------- Dates ----------

export function iso(d: Date): string {
  const pad = (n: number) => String(Math.abs(Math.trunc(n))).padStart(2, '0');
  const off = -d.getTimezoneOffset();
  const sign = off >= 0 ? '+' : '-';
  return (
    `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}` +
    `T${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}` +
    `${sign}${pad(off / 60)}:${pad(off % 60)}`
  );
}

export function dayAt(daysAgo: number, h: number, m: number): Date {
  const d = new Date();
  d.setDate(d.getDate() - daysAgo);
  d.setHours(h, m, 0, 0);
  return d;
}

const plus = (d: Date, s: number) => new Date(d.getTime() + s * 1000);

let segId = 1;
let noteId = 1;
let refId = 1;
let decisionId = 1;
let actionId = 1;
let draftId = 1;
export const nextIds = {
  seg: () => segId++,
  note: () => noteId++,
  ref: () => refId++,
  decision: () => decisionId++,
  action: () => actionId++,
  draft: () => draftId++,
};

type Line = [t0: number, speaker: string, text: string, opts?: { priv?: boolean; echo?: boolean; req?: string }];

function buildSegments(meetingId: string, start: Date, lines: Line[]): Segment[] {
  return lines.map(([t0, speaker, text, o], i) => ({
    id: nextIds.seg(),
    meeting_id: meetingId,
    seq: i + 1,
    ts: iso(plus(start, t0)),
    t0,
    t1: t0 + Math.max(2, Math.round(text.length / 14)),
    channel: speaker === 'Moi' ? 'me' : 'them',
    speaker,
    text,
    private: !!o?.priv,
    is_echo: !!o?.echo,
    request_id: o?.req ?? null,
  }));
}

const GITLAB = 'https://gitlab.example.com/orion/ms-booking/-/blob/main/src/main/java/fr/orion/booking';
const JIRA = 'https://jira.example.com/browse';
const DD = 'https://datadog.example.com';

// ---------- Daily ORION ----------

function daily(): MeetingStore {
  const id = 'daily-orion';
  const start = dayAt(0, 9, 30);
  const meeting: Meeting = {
    id, title: 'Daily ORION', title_source: 'call_window',
    started_at: iso(start), ended_at: iso(plus(start, 17 * 60 + 12)),
    status: 'done', confidential: false, agent_provider: 'claude-code', correlation: 'on_demand', call_app: 'teams',
  };
  const segments = buildSegments(id, start, [
    [5, 'Eux-1', 'Bonjour à tous, on commence le daily. Je partage le tableau du sprint.'],
    [18, 'Moi', "Salut. Hier j'ai terminé la migration de ms-booking vers le nouveau client HTTP, c'est en revue sur APP-1234."],
    [32, 'Eux-2', 'De mon côté je suis toujours bloqué sur APP-1251 : le script de migration des stations plante en recette.'],
    [47, 'Eux-1', 'Tu as regardé les logs ? Il y avait des timeouts sur ms-booking hier soir.'],
    [58, 'Eux-2', "Pas encore, la description du ticket ne donne pas l'heure exacte."],
    [70, 'Moi', 'Sténo, cherche les erreurs Datadog sur ms-booking depuis hier soir.', { req: 'req-daily-1' }],
    [84, 'Eux-1', "Merci. Pendant ce temps : Thomas s'est inscrit à la revue d'archi de jeudi."],
    [95, 'Moi', 'Il faut que je pense à relancer Thomas pour mon entretien annuel.', { priv: true }],
    [110, 'Eux-2', "Sur APP-1251, je propose qu'on passe le batch en pagination par 500 au lieu de tout charger."],
    [126, 'Moi', 'Ça me paraît raisonnable. On garde la pagination par 500 et on ajoute une reprise sur erreur.'],
    [140, 'Eux-1', 'Donc décision : pagination par 500 pour le batch de stations, avec reprise. Je le note dans le ticket.'],
    [141.5, 'Moi', 'Je le note dans le ticket.', { echo: true }],
    [210, 'Moi', 'Sténo a trouvé 37 timeouts sur ms-booking entre 22 h et 23 h, tous sur l’appel au service des stations.'],
    [224, 'Eux-2', 'Ça colle avec mon plantage. Je regarde ça ce matin.'],
    [238, 'Moi', "Je prends la reprise sur erreur dans ms-booking, je m'en occupe d'ici vendredi."],
    [255, 'Eux-1', 'Parfait. Tu mets à jour APP-1251 avec les résultats Datadog ?'],
    [266, 'Eux-2', 'Oui, je le fais aujourd’hui.'],
    [280, 'Moi', "Sténo, note : vérifier le quota de l'API des stations avec l'équipe plateforme.", { req: 'req-daily-2' }],
    [300, 'Eux-1', 'Dernier point : la mise en production de ms-booking est prévue mardi, sauf blocage.'],
    [320, 'Eux-1', 'Merci tout le monde, bonne journée.'],
  ]);

  const answer1 = `**37 timeouts** sur \`ms-booking\` hier entre **22:04 et 22:58**, tous sur l'appel sortant vers \`ms-stations\` (\`GET /stations/batch\`).

- Pic à 22:31 (14 erreurs/min), retour à la normale vers 23:00.
- Délai d'attente configuré à **2 s** dans \`StationClient.java:48\`, alors que le p99 de \`ms-stations\` était de 2,7 s sur la période.
- Le plantage de **APP-1251** en recette a eu lieu à 22:33 : très probablement la même cause.

**Sources**
- [Datadog — logs \`service:ms-booking status:error\`](${DD}/logs?query=service%3Ams-booking%20status%3Aerror)
- [StationClient.java:48](${GITLAB}/station/StationClient.java#L48)
- [APP-1251 — Batch de migration des stations](${JIRA}/APP-1251)`;

  const requests: Request[] = [
    {
      id: 'req-daily-1', meeting_id: id, created_at: iso(plus(start, 74)), origin: 'voice', intent: 'search',
      query: 'les erreurs Datadog sur ms-booking depuis hier soir', context_seconds: 120, tier: 'search',
      status: 'done', started_at: iso(plus(start, 75)), finished_at: iso(plus(start, 118)), error: null,
      summary: '37 timeouts sur ms-booking hier soir (22:04–22:58), tous vers ms-stations. Délai de 2 s trop court ?',
      answer_md: answer1, tools_used: ['datadog.search_logs', 'datadog.get_metrics', 'code.grep'],
    },
    {
      id: 'req-daily-2', meeting_id: id, created_at: iso(plus(start, 284)), origin: 'voice', intent: 'note',
      query: "vérifier le quota de l'API des stations avec l'équipe plateforme", context_seconds: 30, tier: 'quick',
      status: 'done', started_at: iso(plus(start, 284)), finished_at: iso(plus(start, 287)), error: null,
      summary: 'Action notée : vérifier le quota de l’API des stations.',
      answer_md: "Action ajoutée à vos notes : **vérifier le quota de l'API des stations avec l'équipe plateforme**.",
      tools_used: [],
    },
  ];

  const base = `# Daily ORION — ${start.toLocaleDateString('fr-FR', { day: 'numeric', month: 'long', year: 'numeric' })}

## En bref
- Migration de \`ms-booking\` vers le nouveau client HTTP terminée, en revue (APP-1234).
- APP-1251 bloqué : le script de migration des stations plante en recette, lié à des timeouts vers \`ms-stations\`.
- Mise en production de \`ms-booking\` prévue mardi.

## Décisions
- Le batch de stations (APP-1251) passe en **pagination par 500**, avec reprise sur erreur.

## Actions — qui / quoi / échéance
| Qui | Quoi | Échéance |
|---|---|---|
| Moi | Ajouter la reprise sur erreur dans ms-booking | vendredi |
| Eux-2 | Mettre à jour APP-1251 avec les résultats Datadog | aujourd'hui |
| Moi | Vérifier le quota de l'API des stations avec l'équipe plateforme | — |

## Questions ouvertes
- Le délai d'attente de 2 s vers \`ms-stations\` est-il suffisant en production ?

## Points techniques évoqués
- Nouveau client HTTP dans \`ms-booking\`.
- Pagination du batch de stations et reprise sur erreur.
- Timeouts \`ms-booking → ms-stations\` hier soir.

## Recherches faites pendant la réunion
- **Erreurs Datadog sur ms-booking depuis hier soir** : 37 timeouts entre 22:04 et 22:58, tous vers \`ms-stations\`.

## Mes notes
- Vérifier le quota de l'API des stations avec l'équipe plateforme.
- Prévenir le support avant la mise en production de mardi.`;

  const correlatedMd = `${base}

## Liens avec le code et les outils
- ⚠ La décision « pagination par 500 » demande de modifier \`StationBatchJob.java:112\`, qui charge aujourd'hui **toutes** les stations en mémoire (\`findAll()\`).
- ⚠ Le délai de 2 s (\`StationClient.java:48\`) est inférieur au p99 de \`ms-stations\` observé hier soir (2,7 s).
- APP-1234 est en revue (MR !482), 2 approbations sur 2 : rien ne bloque la mise en production de mardi.`;

  const reports: Report[] = [
    { meeting_id: id, markdown: base, generated_at: iso(plus(start, 19 * 60)), provider: 'claude-code', model: null, correlated: false, version: 1 },
    { meeting_id: id, markdown: correlatedMd, generated_at: iso(plus(start, 26 * 60)), provider: 'claude-code', model: null, correlated: true, version: 2 },
  ];

  const refs: ReportRef[] = [
    { id: nextIds.ref(), kind: 'code', ref: 'ms-booking/…/station/StationClient.java:48', title: "Délai d'attente du client ms-stations (2 s)", url: `${GITLAB}/station/StationClient.java#L48`, verified: true, note: null },
    { id: nextIds.ref(), kind: 'code', ref: 'ms-booking/…/batch/StationBatchJob.java:112', title: 'Chargement complet des stations (findAll)', url: `${GITLAB}/batch/StationBatchJob.java#L112`, verified: true, note: null },
    { id: nextIds.ref(), kind: 'jira', ref: 'APP-1251', title: 'Batch de migration des stations en échec en recette', url: `${JIRA}/APP-1251`, verified: true, note: null },
    { id: nextIds.ref(), kind: 'jira', ref: 'APP-1234', title: 'Migration de ms-booking vers le nouveau client HTTP', url: `${JIRA}/APP-1234`, verified: true, note: null },
    { id: nextIds.ref(), kind: 'datadog', ref: 'ms-stations · latence p99', title: 'p99 à 2,7 s hier vers 22:30', url: `${DD}/dashboard/ms-stations`, verified: false, note: 'Lu sur un tableau de bord partagé, non recoupé avec les traces.' },
  ];

  return {
    meeting, segments,
    speakers: [
      { label: 'Moi', display_name: null },
      { label: 'Eux-1', display_name: 'Marie' },
      { label: 'Eux-2', display_name: null },
    ],
    notes: [
      { id: nextIds.note(), meeting_id: id, ts: iso(plus(start, 287)), text: "Vérifier le quota de l'API des stations avec l'équipe plateforme.", source: 'voice' },
      { id: nextIds.note(), meeting_id: id, ts: iso(plus(start, 305)), text: 'Prévenir le support avant la mise en production de mardi.', source: 'user' },
    ],
    requests, reports, refs,
    decisions: [
      { id: nextIds.decision(), meeting_id: id, text: "Le batch de stations (APP-1251) passe en pagination par 500, avec reprise sur erreur.", created_at: iso(plus(start, 19 * 60)) },
    ],
    actions: [
      { id: nextIds.action(), meeting_id: id, owner: 'Moi', text: 'Ajouter la reprise sur erreur dans ms-booking', due: iso(dayAt(-2, 18, 0)).slice(0, 10), status: 'open' },
      { id: nextIds.action(), meeting_id: id, owner: 'Eux-2', text: 'Mettre à jour APP-1251 avec les résultats Datadog', due: iso(dayAt(0, 18, 0)).slice(0, 10), status: 'done' },
      { id: nextIds.action(), meeting_id: id, owner: 'Moi', text: "Vérifier le quota de l'API des stations avec l'équipe plateforme", due: null, status: 'open' },
    ],
    drafts: [
      { id: nextIds.draft(), kind: 'jira_comment', target: 'APP-1251', created_at: iso(plus(start, 26 * 60)),
        body: "Analyse Datadog d'hier soir : 37 timeouts ms-booking → ms-stations entre 22:04 et 22:58 (p99 ms-stations 2,7 s, délai client 2 s). Décision du daily : pagination par 500 avec reprise sur erreur." },
    ],
  };
}

// ---------- Revue d'architecture ----------

function archi(): MeetingStore {
  const id = 'revue-archi-brindille';
  const start = dayAt(1, 14, 0);
  const meeting: Meeting = {
    id, title: "Revue d'architecture — export Brindille", title_source: 'user',
    started_at: iso(start), ended_at: iso(plus(start, 62 * 60)),
    status: 'done', confidential: false, agent_provider: 'claude-code', correlation: 'on_demand', call_app: 'zoom',
  };
  const segments = buildSegments(id, start, [
    [12, 'Eux-1', "L'objectif, c'est de décider comment on alimente Brindille avec les données collaborateurs."],
    [40, 'Eux-2', "L'API temps réel de Brindille est limitée à 10 appels par seconde, ça ne tiendra pas la reprise initiale."],
    [75, 'Moi', 'On pourrait faire un fichier SFTP quotidien pour le stock, et l’API seulement pour les créations.'],
    [130, 'Eux-1', "Ça me va. On part sur le fichier SFTP quotidien, et on garde l'API pour plus tard."],
    [190, 'Eux-3', 'Il faudra chiffrer le fichier, la sécurité va le demander.'],
    [260, 'Moi', 'Je peux estimer le connecteur SFTP pour la semaine prochaine.'],
    [330, 'Eux-1', "Je rédige l'ADR d'ici mercredi prochain."],
    [3600, 'Eux-1', 'Merci à tous.'],
  ]);
  const md = `# Revue d'architecture — export Brindille — ${start.toLocaleDateString('fr-FR', { day: 'numeric', month: 'long', year: 'numeric' })}

## En bref
- L'API temps réel de Brindille (10 appels/s) ne tient pas la reprise initiale.
- Choix d'un **fichier SFTP quotidien** chiffré pour le stock.

## Décisions
- L'export Brindille passe par un fichier SFTP quotidien plutôt que par l'API temps réel.

## Actions — qui / quoi / échéance
| Qui | Quoi | Échéance |
|---|---|---|
| Sophie | Rédiger l'ADR export Brindille | mercredi prochain |
| Moi | Estimer le connecteur SFTP | semaine prochaine |

## Questions ouvertes
- Quel chiffrement exige l'équipe sécurité (PGP, AES) ?

## Points techniques évoqués
- Limite de débit de l'API Brindille.
- Format et chiffrement du fichier SFTP.`;
  return {
    meeting, segments,
    speakers: [
      { label: 'Moi', display_name: null },
      { label: 'Eux-1', display_name: 'Sophie' },
      { label: 'Eux-2', display_name: null },
      { label: 'Eux-3', display_name: null },
    ],
    notes: [],
    requests: [],
    reports: [{ meeting_id: id, markdown: md, generated_at: iso(plus(start, 64 * 60)), provider: 'claude-code', model: null, correlated: false, version: 1 }],
    refs: [],
    decisions: [{ id: nextIds.decision(), meeting_id: id, text: "L'export Brindille passe par un fichier SFTP quotidien plutôt que par l'API temps réel.", created_at: iso(plus(start, 64 * 60)) }],
    actions: [
      { id: nextIds.action(), meeting_id: id, owner: 'Sophie', text: "Rédiger l'ADR export Brindille", due: iso(dayAt(-6, 18, 0)).slice(0, 10), status: 'open' },
      { id: nextIds.action(), meeting_id: id, owner: 'Moi', text: 'Estimer le connecteur SFTP', due: iso(dayAt(-5, 18, 0)).slice(0, 10), status: 'done' },
    ],
    drafts: [],
  };
}

// ---------- Huddle Slack ----------

function huddle(): MeetingStore {
  const id = 'huddle-equipe-flotte';
  const start = dayAt(1, 11, 15);
  const md = `# Huddle #equipe-flotte

## En bref
- Le déverrouillage des vélos cargo échoue depuis la v2.13.
- Le fournisseur des cadenas renvoie un code \`CARGO_UNLOCK_UNSUPPORTED\` : il faut déverrouiller en deux temps.

## Décisions
- Contournement : déverrouillage en deux temps (cadenas puis batterie), en attendant le correctif du fournisseur.

## Actions — qui / quoi / échéance
| Qui | Quoi | Échéance |
|---|---|---|
| Moi | Ajouter le déverrouillage en deux temps dans ms-fleet | vendredi |
| Inès | Ouvrir un ticket chez le fournisseur des cadenas | aujourd'hui |`;
  return {
    meeting: {
      id, title: 'Huddle #equipe-flotte', title_source: 'call_window', started_at: iso(start), ended_at: iso(plus(start, 11 * 60)),
      status: 'done', confidential: false, agent_provider: 'claude-code', correlation: 'on_demand', call_app: 'slack',
    },
    segments: buildSegments(id, start, [
      [4, 'Eux-1', 'Je vous appelle vite fait : le déverrouillage des vélos cargo plante.'],
      [21, 'Moi', 'Depuis quand ? On a livré la v2.13 de ms-fleet mardi.'],
      [36, 'Eux-1', 'Depuis mardi justement. Le fournisseur des cadenas répond CARGO_UNLOCK_UNSUPPORTED.'],
      [58, 'Moi', 'On peut déverrouiller en deux temps en attendant, c’est déjà prévu dans leur API.'],
      [80, 'Eux-1', 'D’accord, je leur ouvre un ticket aujourd’hui de mon côté.'],
      [96, 'Moi', 'Je m’occupe du contournement dans ms-fleet, pour vendredi.'],
    ]),
    speakers: [{ label: 'Moi', display_name: null }, { label: 'Eux-1', display_name: 'Inès' }],
    notes: [],
    requests: [],
    reports: [{ meeting_id: id, markdown: md, generated_at: iso(plus(start, 13 * 60)), provider: 'claude-code', model: null, correlated: false, version: 1 }],
    refs: [],
    decisions: [{ id: nextIds.decision(), meeting_id: id, text: 'Déverrouillage en deux temps en attendant le correctif du fournisseur des cadenas.', created_at: iso(plus(start, 13 * 60)) }],
    actions: [
      { id: nextIds.action(), meeting_id: id, owner: 'Moi', text: 'Ajouter le déverrouillage en deux temps dans ms-fleet', due: iso(dayAt(-3, 18, 0)).slice(0, 10), status: 'open' },
      { id: nextIds.action(), meeting_id: id, owner: 'Inès', text: 'Ouvrir un ticket chez le fournisseur des cadenas', due: iso(dayAt(1, 18, 0)).slice(0, 10), status: 'done' },
    ],
    drafts: [],
  };
}

// ---------- 1:1 (confidentielle) ----------

function oneOnOne(): MeetingStore {
  const id = 'one-on-one-thomas';
  const start = dayAt(2, 16, 30);
  return {
    meeting: {
      id, title: '1:1 Thomas', title_source: 'call_window', started_at: iso(start), ended_at: iso(plus(start, 28 * 60)),
      status: 'ended', confidential: true, agent_provider: null, correlation: 'off', call_app: 'teams',
    },
    segments: buildSegments(id, start, [
      [8, 'Eux', 'Comment ça se passe sur le projet en ce moment ?'],
      [20, 'Moi', 'Plutôt bien, la migration avance. Je voudrais parler de la suite.'],
      [44, 'Eux', 'On en reparle à ton entretien annuel, je bloque un créneau.'],
    ]),
    speakers: [{ label: 'Moi', display_name: null }, { label: 'Eux', display_name: 'Thomas' }],
    notes: [], requests: [], reports: [], refs: [], decisions: [], actions: [], drafts: [],
  };
}

// ---------- Incident ms-pricing ----------

function incident(): MeetingStore {
  const id = 'incident-ms-pricing';
  const start = dayAt(5, 11, 0);
  const segments = buildSegments(id, start, [
    [10, 'Eux-1', 'Les factures de novembre sont sorties en double pour 312 clients.'],
    [35, 'Moi', 'Sténo, vérifie si le correctif de APP-1198 est déployé en production.', { req: 'req-incident-1' }],
    [80, 'Eux-2', 'Le job de recalcul des tarifs a été relancé deux fois après le redémarrage du cluster.'],
    [140, 'Moi', "Le correctif n'est pas encore en production, il est seulement en recette."],
    [200, 'Eux-1', 'On déploie le correctif aujourd’hui et on prévient les clients concernés.'],
  ]);
  const md1 = `# Point incident ms-pricing

## En bref
- Factures de novembre émises en double pour 312 clients.
- Cause : relance du job de recalcul des tarifs après le redémarrage du cluster.

## Décisions
- Déploiement du correctif APP-1198 en production le jour même.

## Actions — qui / quoi / échéance
| Qui | Quoi | Échéance |
|---|---|---|
| Moi | Déployer APP-1198 en production | aujourd'hui |
| Eux-1 | Prévenir les clients concernés | aujourd'hui |`;
  const md2 = `${md1}

## Liens avec le code et les outils
- \`PricingJob.kt:57\` ne vérifie pas le verrou distribué avant de démarrer : le correctif APP-1198 l'ajoute.
- Aucune autre exécution en double trouvée dans les logs des 30 derniers jours.`;
  return {
    meeting: {
      id, title: 'Point incident ms-pricing', title_source: 'inferred', started_at: iso(start), ended_at: iso(plus(start, 25 * 60)),
      status: 'done', confidential: false, agent_provider: 'codex', correlation: 'on_demand', call_app: null,
    },
    segments,
    speakers: [{ label: 'Moi', display_name: null }, { label: 'Eux-1', display_name: 'Karim' }, { label: 'Eux-2', display_name: null }],
    notes: [],
    requests: [{
      id: 'req-incident-1', meeting_id: id, created_at: iso(plus(start, 38)), origin: 'voice', intent: 'check',
      query: 'si le correctif de APP-1198 est déployé en production', context_seconds: 120, tier: 'search', status: 'done',
      started_at: iso(plus(start, 38)), finished_at: iso(plus(start, 71)), error: null,
      summary: 'Non : APP-1198 est en recette (v2.14.0-rc2), pas en production (v2.13.4).',
      answer_md: `**Non.** Le correctif de APP-1198 est en **recette** (\`v2.14.0-rc2\`), la production tourne encore en \`v2.13.4\`.

**Sources**
- [APP-1198 — Double exécution du job de recalcul des tarifs](${JIRA}/APP-1198)
- [Pipeline de déploiement ms-pricing](https://gitlab.example.com/orion/ms-pricing/-/pipelines)`,
      tools_used: ['jira.get_issue', 'gitlab.list_pipelines'],
    }],
    reports: [
      { meeting_id: id, markdown: md1, generated_at: iso(plus(start, 27 * 60)), provider: 'codex', model: null, correlated: false, version: 1 },
      { meeting_id: id, markdown: md2, generated_at: iso(plus(start, 40 * 60)), provider: 'codex', model: null, correlated: true, version: 2 },
    ],
    refs: [
      { id: nextIds.ref(), kind: 'code', ref: 'ms-pricing/…/PricingJob.kt:57', title: 'Démarrage du job sans verrou distribué', url: null, verified: true, note: null },
      { id: nextIds.ref(), kind: 'jira', ref: 'APP-1198', title: 'Double exécution du job de recalcul des tarifs', url: `${JIRA}/APP-1198`, verified: true, note: null },
    ],
    decisions: [{ id: nextIds.decision(), meeting_id: id, text: 'Déploiement du correctif APP-1198 en production le jour même.', created_at: iso(plus(start, 27 * 60)) }],
    actions: [
      { id: nextIds.action(), meeting_id: id, owner: 'Moi', text: 'Déployer APP-1198 en production', due: iso(dayAt(5, 18, 0)).slice(0, 10), status: 'done' },
      { id: nextIds.action(), meeting_id: id, owner: 'Karim', text: 'Prévenir les clients concernés', due: iso(dayAt(5, 18, 0)).slice(0, 10), status: 'done' },
    ],
    drafts: [],
  };
}

// ---------- Réunion en direct ----------

/** Titre de l'appel Teams simulé au démarrage (proposé, jamais démarré automatiquement). */
export const TEAMS_TITLE = 'Refinement ORION — sprint 48';

/** Script joué en boucle pendant l'écoute simulée (après « Démarrer l'écoute »). */
export const LIVE_SCRIPT: [speaker: string, text: string][] = [
  ['Eux-1', 'On attaque le ticket APP-1260 : la validation des numéros de cadre dans ms-fleet.'],
  ['Moi', 'Sténo, cherche la règle de validation du numéro de cadre dans ms-fleet.'],
  ['Eux-2', 'Aujourd’hui on accepte n’importe quelle chaîne de cinq caractères.'],
  ['Moi', 'Il faudrait au moins gérer les cadres importés et les anciens modèles.'],
  ['Eux-1', 'La description du ticket parle aussi des vélos cargo.'],
  ['Eux-2', 'Pendant ce temps, je regarde le script de reprise des données.'],
  ['Eux-1', 'On l’estime à combien ? Je dirais trois points.'],
  ['Moi', 'Trois points me vont, à condition de réutiliser la table de référence des modèles de vélos.'],
  ['Eux-2', 'D’accord pour trois. Le ticket suivant, c’est APP-1262 sur les doublons de comptes.'],
  ['Eux-1', 'Celui-là il faut d’abord une analyse, on ne peut pas l’estimer aujourd’hui.'],
  ['Moi', 'Je peux faire l’analyse cette semaine si personne ne s’est inscrit dessus.'],
  ['Eux-2', 'Vas-y, je te l’assigne.'],
];

export function seedMeetings(): MeetingStore[] {
  return [daily(), archi(), huddle(), oneOnOne(), incident()];
}

/** Demandes faites hors réunion (historique). */
export function seedHistoryRequests(): Request[] {
  const d = dayAt(1, 17, 40);
  return [{
    id: 'req-hist-1', meeting_id: null, created_at: iso(d), origin: 'text', intent: 'history',
    query: "Qu'a-t-on décidé sur l'export Brindille ?", context_seconds: null, tier: 'search', status: 'done',
    started_at: iso(d), finished_at: iso(plus(d, 14)), error: null,
    summary: 'Fichier SFTP quotidien chiffré pour le stock ; API temps réel plus tard.',
    answer_md: `Lors de la **revue d'architecture d'hier**, l'équipe a décidé d'alimenter Brindille par un **fichier SFTP quotidien** (chiffré) plutôt que par l'API temps réel, limitée à 10 appels/s.

- ADR à rédiger par Sophie d'ici mercredi prochain.
- Estimation du connecteur SFTP : faite.`,
    tools_used: ['steno.search_meetings', 'steno.get_report'],
  }];
}

// ---------- Réglages, agents, connecteurs, dépôts ----------

export function defaultSettings(): Settings {
  return {
    agent: {
      provider: 'claude-code',
      tiers: {
        search: { agent: '', model: '', effort: '' },
        summary: { agent: '', model: '', effort: '' },
        quick: { agent: '', model: '', effort: '' },
      },
      models: { quick: '', search: '', summary: '' },
      custom_command: '',
      allow_api_key: false,
      max_parallel: 2,
      timeouts_s: { quick: 90, search: 240, summary: 300 },
      live_tool_budget: 15,
      live_context_seconds: 120,
      use_skills: true,
    },
    voice: {
      wake_word: 'Sténo',
      strict: false,
      prefixes: ['ok', 'dis', 'hey'],
      variants: ['steno', 'stenau', 'stenot'],
      verbs: {
        search: ['cherche', 'recherche', 'regarde', 'trouve'],
        check: ['vérifie', 'confirme'],
        note: ['note', 'ajoute une action', 'retiens'],
        recap: ['résume', 'récap', 'récapitule'],
        cancel: ['annule'],
        stop: ["arrête l'écoute", "arrête d'écouter", 'arrête la transcription', 'termine la réunion', 'fin de la réunion'],
      },
      feedback: 'system',
      feedback_device: null,
    },
    audio: {
      auto_detect_calls: true,
      input_device: null,
      input_device_name: null,
      call_apps: defaultCallApps(),
      silence_ms: 1000,
      final_transcription: 'fr',
      vocabulary: ['ms-booking', 'ms-stations', 'ms-pricing', 'ORION', 'Brindille'],
      shortcuts: { toggle: 'Ctrl+Alt+S', aside: 'Ctrl+Alt+Space', quick_note: 'Ctrl+Alt+N' },
    },
    correlation: { mode: 'on_demand', budget: 30, exclude_words: ['1:1', 'RH', 'entretien', 'perso'] },
    privacy: {
      audio_retention_days: 0,
      transcript_retention_days: null,
      auto_export_dir: null,
      excluded_titles: [],
      consent_reminder: true,
      consent_message: DEFAULT_CONSENT_MESSAGE,
    },
    profile: { name: 'Camille', role: 'Développeuse back-end', team: 'ORION', jira_key_pattern: '[A-Z][A-Z0-9]+-\\d+', theme: 'auto', language: 'fr' },
    updates: { check: true, auto_install: false },
    report: { default_template: 'standard', ask_context_on_stop: false, context_delay_s: 60, templates: [{ id: 'perso-note-comex', name: 'Note COMEX', instructions: 'Trois puces au plus, ton exécutif, chiffres en gras.' }] },
  };
}

const caps = (deny: boolean) => ({ resume_session: true, stream_json: true, read_only_files: true, tool_deny_list: deny, extra_dirs: true });

const EFFORTS_MAX = ['low', 'medium', 'high', 'xhigh', 'max'];
const none = { resume_session: false, stream_json: false, read_only_files: false, tool_deny_list: false, extra_dirs: false };

export function seedAgents(): AgentInfo[] {
  return [
    { id: 'claude-code', name: 'Claude Code', support: 'supported', installed: true, version: '2.3.1', error: null, capabilities: caps(true), efforts: [...EFFORTS_MAX], lists_models: false },
    { id: 'codex', name: 'Codex', support: 'supported', installed: true, version: '0.98.0', error: null, capabilities: caps(true), efforts: [...EFFORTS_MAX], lists_models: false },
    { id: 'antigravity', name: 'Antigravity', support: 'experimental', installed: true, version: '1.2.0', error: null, capabilities: caps(true), efforts: [...EFFORTS_MAX], lists_models: true },
    { id: 'grok', name: 'Grok Build', support: 'experimental', installed: true, version: '0.4.2', error: null, capabilities: caps(true), efforts: ['low', 'medium', 'high', 'xhigh'], lists_models: true },
    { id: 'gemini', name: 'Gemini CLI', support: 'experimental', installed: false, version: null, error: 'Introuvable dans le PATH', capabilities: { ...caps(false), resume_session: false }, efforts: [], lists_models: false },
    { id: 'opencode', name: 'OpenCode', support: 'experimental', installed: false, version: null, error: 'Introuvable dans le PATH', capabilities: caps(false), efforts: [], lists_models: false },
    { id: 'custom', name: 'Commande personnalisée', support: 'generic', installed: false, version: null, error: 'Aucune commande configurée', capabilities: none, efforts: [], lists_models: false },
  ];
}

/** Modèles proposés par agent (données d'exemple, pas une vérité). */
export function seedModels(provider: string): ModelInfo[] {
  const alias = 'alias : dernière version';
  switch (provider) {
    case 'claude-code':
      return [
        ...['fable', 'opus', 'sonnet', 'haiku'].map((id) => ({ id, name: id, note: alias })),
        ...['claude-opus-5-5', 'claude-sonnet-5-5', 'claude-haiku-5-5', 'claude-fable-5-2'].map((id) => ({ id, name: id, note: null })),
      ];
    case 'codex':
      return [{ id: 'gpt-5.5-codex', name: 'gpt-5.5-codex', note: 'lu dans ~/.codex/config.toml ; Codex ne liste pas ses modèles' }];
    case 'antigravity':
      return ['gemini-3.5-pro', 'gemini-3.5-flash', 'gemini-3-pro', 'gemini-3-flash'].map((id) => ({ id, name: id, note: null }));
    case 'grok':
      return ['grok-4.7', 'grok-4.6', 'grok-4.5'].map((id) => ({ id, name: id, note: null }));
    default:
      return [];
  }
}

/** Périphériques simulés (mêmes données que le simulateur du cœur). */
export function seedAudioDevices(): AudioDevices {
  return {
    inputs: [
      { id: 'BuiltInMicrophoneDevice', name: 'Micro du MacBook Pro', default: true, transport: 'builtin' },
      { id: 'AirPodsPro-UID:input', name: 'AirPods Pro', default: false, transport: 'bluetooth' },
      { id: 'ScarlettSolo-UID', name: 'Scarlett Solo USB', default: false, transport: 'usb' },
    ],
    outputs: [
      { id: 'AirPodsPro-UID:output', name: 'AirPods Pro', default: true, transport: 'bluetooth' },
      { id: 'BuiltInSpeakerDevice', name: 'Haut-parleurs du MacBook Pro', default: false, transport: 'builtin' },
    ],
  };
}

export function seedConnectors(): Connector[] {
  const now = iso(dayAt(0, 8, 55));
  return [
    { id: 1, provider: 'claude-code', name: 'atlassian', status: 'connecté', detected_at: now, use_live: true, use_correlation: true, write_blockable: true },
    { id: 2, provider: 'claude-code', name: 'datadog', status: 'connecté', detected_at: now, use_live: true, use_correlation: true, write_blockable: true },
    { id: 3, provider: 'claude-code', name: 'gitlab', status: 'connecté', detected_at: now, use_live: false, use_correlation: true, write_blockable: true },
    { id: 4, provider: 'claude-code', name: 'slack', status: 'connecté', detected_at: now, use_live: false, use_correlation: false, write_blockable: false },
    { id: 5, provider: 'claude-code', name: 'figma', status: 'authentification requise', detected_at: now, use_live: false, use_correlation: false, write_blockable: true },
  ];
}

export function seedRepos(): Repo[] {
  return [
    { id: 1, label: 'ms-booking', path: '/Users/camille/dev/orion/ms-booking', use_live: true, use_correlation: true, exclude_globs: '**/target/**\n**/*.min.js' },
    { id: 2, label: 'ms-pricing', path: '/Users/camille/dev/orion/ms-pricing', use_live: true, use_correlation: true, exclude_globs: '' },
    { id: 3, label: 'infra-cluster', path: '/Users/camille/dev/orion/infra-cluster', use_live: false, use_correlation: true, exclude_globs: 'secrets/**' },
  ];
}

/** Formats intégrés (miroir de `report::BUILTIN_TEMPLATES` côté cœur). */
export const MOCK_BUILTIN_TEMPLATES = [
  { id: 'standard', name: 'Standard', description: 'Décisions, actions, questions ouvertes, points techniques.' },
  { id: 'daily', name: "Daily / point d'équipe", description: 'Court : par personne, ce qui est fait, en cours, bloqué.' },
  { id: 'client', name: 'Point client', description: 'Présentable au client : demandes, engagements, prochaines étapes.' },
  { id: 'atelier', name: 'Atelier / spécification', description: 'Détaillé : besoins, règles de gestion, cas limites, points à trancher.' },
  { id: 'arbitrage', name: 'Comité / arbitrage', description: "Options étudiées, arguments, décision et qui l'a prise." },
  { id: 'mail', name: 'Mail de synthèse', description: 'Prêt à envoyer : objet, synthèse, décisions, actions.' },
  { id: 'slack', name: 'Message Slack / Teams', description: 'Court, à coller dans un canal : 10 lignes au plus.' },
];
