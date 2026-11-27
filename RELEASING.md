# Publier une version

Les utilisateurs compilent Sténo depuis les sources. Une version est donc un **tag signé**
`vX.Y.Z` sur `main` : l'app le détecte (API publique de GitHub, toutes les 6 h) et propose de
l'installer. L'installation amène le clone de l'utilisateur sur ce tag puis lance
`make install`.

1. Sur une branche, changez le numéro de version partout :
   ```sh
   scripts/bump-version.sh 0.2.0
   git commit -am "Version 0.2.0"
   ```
   Ouvrez une pull request ; une fois la CI verte, fusionnez-la dans `main`.
2. Créez et poussez le tag (signé, réservé aux administrateurs) :
   ```sh
   git checkout main && git pull
   git tag -s v0.2.0 -m "Sténo 0.2.0"
   git push origin v0.2.0
   ```
3. Le workflow **Publication** vérifie que le tag correspond à la version du code et crée la
   version GitHub, avec des notes générées depuis les pull requests. Complétez-les au besoin :
   elles s'affichent dans Réglages → Mises à jour.

Règles :
- `main` n'accepte que des pull requests, avec CI verte et commits signés.
- Un tag publié ne se déplace ni ne se supprime : en cas d'erreur, publiez la version suivante.
- Versions `X.Y.Z` uniquement ; une pré-version (`v0.3.0-rc1`, marquée « pre-release ») n'est
  pas proposée aux utilisateurs.
