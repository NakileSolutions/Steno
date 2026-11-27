# Sécurité

Sténo écoute votre micro et le son de vos appels : la sécurité et la confidentialité sont au
cœur du projet. Merci de signaler toute faille de façon responsable.

## Signaler une vulnérabilité

- **Ne publiez pas** la faille dans une issue publique.
- Utilisez le signalement privé de GitHub (onglet **Security → Report a vulnerability** du
  dépôt) ou écrivez à **dev@nakile.fr** avec l'objet « Sécurité Sténo ».
- Décrivez la faille, la version (`steno --version` ou Réglages), les étapes pour la
  reproduire et son impact.

Nous accusons réception sous 7 jours et visons un correctif sous 30 jours selon la gravité.
Vous serez crédité dans les notes de version si vous le souhaitez.

## Versions prises en charge

Seule la dernière version de la branche principale reçoit des correctifs de sécurité.

## Modèle de sécurité

### Ce qui reste sur le Mac

- **Audio** : capturé et transcrit localement (Neural Engine / GPU). Jamais envoyé sur le
  réseau. Les fichiers temporaires (`~/Library/Caches/Steno/<réunion>`) sont supprimés après
  le compte rendu, sauf rétention choisie (0 jour par défaut).
- **Données** : base SQLite locale `~/Library/Application Support/Steno/steno.db` (droits
  600), sauvegarde quotidienne locale (7 dernières). Aucun serveur Sténo, aucune télémétrie.
- **Documents joints** au contexte d'une réunion : leur texte est extrait localement et gardé
  dans la base ; le fichier d'origine n'est pas conservé.

### Ce qui sort du Mac, et vers qui

- Seul du **texte** (extraits de transcription, notes, contexte, texte des documents joints)
  est envoyé, et uniquement au **CLI d'agent IA que vous avez choisi**, qui le transmet à son
  éditeur selon ses conditions. Cela arrive : à votre demande (commande vocale ou tapée), à la
  fin de la réunion (compte rendu) et pour la corrélation si elle est activée.
- **Réunion confidentielle** : aucun appel à l'IA, ni recherche ni compte rendu.
- Les **modèles audio** sont téléchargés depuis Hugging Face au premier usage ; aucune donnée
  n'est envoyée lors de ce téléchargement.

### L'agent ne fait que lire

- L'agent tourne en lecture seule : outils d'écriture refusés, commandes shell limitées à une
  liste, lectures limitées aux dépôts que vous avez cochés.
- Pour Claude Code, chaque appel d'outil passe par un crochet (`steno policy-hook`) qui
  refuse les outils MCP d'écriture (`*send*`, `*create*`, `*comment*`, `executeWrite`…).
- Un agent qui ne sait pas bloquer ses outils MCP voit ses connecteurs désactivés.
- Les clés d'API présentes dans l'environnement ne sont pas transmises à l'agent (abonnement
  uniquement), sauf activation explicite dans les réglages.
- Le contenu des réunions est toujours présenté à l'agent comme des **données**, jamais comme
  des consignes (protection contre l'injection d'instructions par un participant).

### Permissions macOS

| Permission | Usage |
|---|---|
| Micro | Transcrire votre voix et détecter le mot d'appel |
| Audio du système (« Enregistrement de l'écran et de l'audio système ») | Entendre les autres participants, uniquement dans l'app de l'appel |
| Accessibilité (facultatif) | Lire le titre de l'appel en cours |

Pour proposer la permission « audio du système » dans l'assistant, Sténo utilise une API
privée de macOS (`TCC.framework`, chargée dynamiquement), comme d'autres outils de capture
audio. Elle sert uniquement à demander ou vérifier cette permission.

### Signature

L'application est compilée sur votre machine et signée **ad hoc** (sans compte développeur
Apple). Compilez-la depuis une source de confiance et vérifiez le dépôt avant `make install`.
