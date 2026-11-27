# Procédure : corrélation avec le code et les outils

Applique la procédure `recherche-croisee` aux **points techniques évoqués** et aux
**questions ouvertes** du compte rendu, sur les sources disponibles seulement.

Réponds avec une section markdown :

```
## Liens avec le code et les outils
```

- Pour chaque point : ce que disent le code, Jira, Confluence, Datadog, avec les références.
- **Alertes** en tête de section quand une décision contredit le comportement actuel
  (« ⚠️ La décision X contredit `fichier:ligne` »).
- Termine par un bloc ```json``` :
  `{"refs": [{"kind": "code|jira|confluence|datadog|gitlab|autre", "ref": "APP-1234 ou chemin:ligne",
    "title": "…", "url": null, "verified": true, "note": "…"}]}`
