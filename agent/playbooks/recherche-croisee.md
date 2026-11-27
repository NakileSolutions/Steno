# Procédure : recherche croisée

À utiliser pour les demandes `search` et `check`, et pour la corrélation après la réunion.

**En direct, la vitesse prime** (la réunion continue pendant que tu cherches) :
- commence par la source la plus probable et arrête-toi dès que tu as la réponse : une
  question sur le code ne demande ni Jira, ni Confluence, ni Datadog ;
- un projet est cité (« Atlas ») : travaille dans son dossier (liste « Projets ») ; ne
  lance jamais de recherche sur tout un dossier parent ;
- préfère un accès ciblé (`Read` d'un fichier connu, `Glob` sur un chemin précis) à une
  recherche large ; une question simple se règle en 1 à 3 appels.

1. **Extraire les entités** de la demande et du contexte récent : clés de tickets (motif
   `{{JIRA}}`), noms de services (`ms-*`…), fonctions, endpoints, environnements (prod,
   recette…), plages horaires (« depuis hier », « cette nuit »). Résous « ça », « ce ticket »
   grâce aux dernières phrases de la transcription.
2. **Jira** : lis les tickets cités ; puis une recherche JQL sur les mots-clés (textes et
   résumés), limitée aux tickets récents.
3. **Confluence** : recherche sur les mêmes mots-clés ; lis seulement les pages pertinentes.
4. **Code** (dépôts lisibles) : `rg -n` sur les identifiants, `git log -S <terme> --oneline`,
   `git log --grep <clé>` ; MR liées via `glab mr list --search` / `glab mr view` (lecture).
5. **Datadog** : logs, monitors et incidents sur la fenêtre de temps pertinente (par défaut
   les dernières 24 h), filtrés par service.
6. **Synthèse** :
   - `TL;DR: …` (2 lignes max) ;
   - constats **par source**, chacun avec sa référence ;
   - **contradictions** (« le ticket dit X, le code fait Y ») ;
   - **à vérifier** : ce que tu n'as pas pu confirmer.

Budget : respecte le nombre maximal d'appels d'outils indiqué dans la demande. Quand il est
atteint, réponds avec ce que tu as et propose « approfondir après la réunion ».
Ne consulte que les sources indiquées comme disponibles.
