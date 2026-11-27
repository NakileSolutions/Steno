# Adaptateurs d'agent — options utilisées et versions testées

Code : `crates/steno-core/src/agents/`. C'est le **seul** endroit qui connaît les CLI.
Les options des CLI changent vite : à chaque mise à jour d'un agent, relancer
`steno doctor --live` (test réel « poste sur Slack ») et mettre ce fichier à jour.

| Adaptateur | Statut | Version vérifiée | Bloque les MCP en écriture ? | Reprise de session |
|---|---|---|---|---|
| `claude-code` | supporté (référence) | 2.1.332 (`--help` vérifié) | **oui** (crochet `PreToolUse`) | oui (`--session-id` / `--resume`) |
| `codex` | supporté | non installé ici — À VÉRIFIER | non → connecteurs grisés | oui (`codex exec resume`) |
| `antigravity` | expérimental | non installé ici — options d'après la doc officielle, À VÉRIFIER | non → connecteurs grisés | oui (`--conversation`) |
| `grok` | expérimental | non installé ici — options d'après la doc officielle, À VÉRIFIER | non → connecteurs grisés | oui (`-s <uuid>`) |
| `gemini` | expérimental | non installé ici — À VÉRIFIER | non → connecteurs grisés | non (résumé rejoué) |
| `opencode` | expérimental | non installé ici — À VÉRIFIER | non → connecteurs grisés | oui (`--session`) |
| `custom` | générique | — | non → connecteurs grisés | non (résumé rejoué) |

## Principe commun

- Un **dossier d'exécution par réunion** (`~/Library/Application Support/Steno/runs/<réunion>/`),
  stable pour que la reprise de session fonctionne. Les consignes (`agent/INSTRUCTIONS.md` +
  `agent/playbooks/*.md`, complétées par le profil) y sont écrites au format du CLI :
  `CLAUDE.md`, `AGENTS.md`, `GEMINI.md` ou `INSTRUCTIONS.md`.
- L'invite est envoyée sur l'**entrée standard** (pas de limite de taille de ligne de commande).
- Les clés d'API présentes dans l'environnement (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`,
  `GEMINI_API_KEY`…) sont **retirées** de l'environnement de l'agent, sauf si « Autoriser
  l'utilisation d'une clé API » est coché : l'agent utilise donc l'abonnement du dev.
- The removed list is best-effort: keys whose mere presence makes a CLI bill the API instead of
  the subscription (OpenCode also loses its providers' keys). Cloud credentials (`AWS_*`,
  `GOOGLE_APPLICATION_CREDENTIALS`, Azure) are kept, so Claude Code on Bedrock or Vertex keeps
  working. The `custom` command receives the full environment.
- Repository exclusions (`Repo.exclude_globs`) are enforced by the `PreToolUse` hook, so only
  with Claude Code: Codex (read-only sandbox) and OpenCode can still read excluded files in a
  ticked repository.
- Le processus est lancé dans son propre groupe et tué entièrement au délai ou à l'annulation.
- Niveaux : `quick` (note, récap, test), `search` (recherche croisée, corrélation, historique),
  `summary` (compte rendu). Modèle vide = modèle par défaut du CLI.

## `claude-code`

```
claude -p --output-format stream-json --verbose
       --permission-mode dontAsk
       --settings <run>/.steno/settings-<id>.json
       [--model <m>]
       --session-id <uuid> | --resume <id> | --no-session-persistence
       # sans outils (compte rendu, récap) :
       --tools "" --strict-mcp-config --mcp-config '{"mcpServers":{}}'
       # avec outils (recherche, corrélation) :
       --tools Read,Grep,Glob,Bash
       --allowedTools Read Grep Glob "Bash(rg *)" … mcp__<connecteur>…
       --disallowedTools Write Edit MultiEdit NotebookEdit WebFetch WebSearch Task
       --add-dir <dépôt>…
```

Lecture seule, en trois couches :
1. `dontAsk` : tout outil non listé dans `--allowedTools` est refusé sans question.
2. `--disallowedTools` + `permissions.deny` (projet et `--settings`) : écriture, web, sous-agents.
3. **Crochet `PreToolUse`** (`<exécutable Sténo> policy-hook <politique.json>`) appelé avant
   **chaque** outil : motifs MCP interdits, mots d'écriture dans le nom de l'outil
   (`addOrEditJiraIssueComment`, `executeWrite`…), connecteurs non activés (nom exact),
   commandes shell hors liste (`;`, `>`, `$`, `(`… refusés), options qui lancent un programme
   ou écrivent un fichier (`--pre`, `-exec`, `-delete`, `sort -o`, `git grep -O`…) ou qui
   suivent les liens symboliques (`rg -L`, `grep -R`, `find -L`…), chemins hors des dépôts
   cochés (liens symboliques résolus), fichiers exclus du dépôt lus directement,
   budget d'appels (15 en direct, 30 en corrélation). Code de sortie 2 = refus, raison renvoyée
   à l'agent ; 0 = autorisé. Toute erreur interne du crochet (panique comprise) sort en 2.
   Le crochet est le même binaire que l'app (`Steno.app/Contents/MacOS/steno-app policy-hook …`)
   ou la CLI (`steno policy-hook …`). Sans exécutable de crochet, Claude Code n'est pas lancé.

Sortie : `system/init` (session, modèle), `assistant` (`tool_use` → outils utilisés), `user`
(`tool_result` en erreur → outil refusé, retiré de `tools_used`), `result` (réponse finale).

Connecteurs : `claude mcp list` (« nom: cible - ✓ Connected »). Les connecteurs du compte
Claude (« claude.ai Atlassian ») apparaissent dans les outils sous `mcp__claude_ai_Atlassian__…`.
**Spike S3** : vérifier qu'ils sont bien disponibles en mode `-p` sur la version installée ;
sinon plan B : `claude mcp add --transport http <nom> <url>`.

## Modèle et niveau de réflexion par usage

Réglages → Agent IA → « Modèles et réflexion » : pour **Recherches et corrélation**
(`tiers.search`), **Compte rendu** (`tiers.summary`) et **Notes et récapitulatifs**
(`tiers.quick`), on choisit l'agent, le modèle (liste lue depuis le CLI quand il sait la
fournir, saisie libre sinon) et le niveau de réflexion.

| Agent | Modèle | Liste des modèles | Réflexion |
|---|---|---|---|
| `claude-code` | `--model` | suggestions (alias `fable`, `opus`, `sonnet`, `haiku` + identifiants connus) | `--effort low\|medium\|high\|xhigh\|max` |
| `codex` | `-m` | modèles cités dans `~/.codex/config.toml` (pas de commande de liste documentée) | `-c model_reasoning_effort="…"` (low…max, selon le modèle) |
| `antigravity` | `--model` | `agy models` | `--effort low…max` (selon le modèle) |
| `grok` | `-m` | `grok models` | `--effort` (low, medium, high, xhigh selon le modèle ; le CLI refuse un niveau non pris en charge) |
| `custom` | `{model}` | — | `{effort}` |

## `antigravity` (expérimental)

```
agy -p "<consignes + invite>" --output-format json [--model <slug>] [--effort <niveau>] [--conversation <id>]
```

Sortie : `{conversation_id, status, response, error, …}`. En mode non interactif, tout outil
qui demande une approbation est refusé d'office ; on ne passe jamais
`--dangerously-skip-permissions`. Fichier de consignes lu par le CLI non confirmé : les
consignes sont incluses dans l'invite. Doc : https://antigravity.google/docs/cli/reference/

## `grok` (expérimental)

```
grok -p "<invite>" --output-format json --no-auto-update --disable-web-search --no-subagents
     --disallowed-tools Bash,Edit,MCPTool,WebFetch,WebSearch   # (+ Read,Grep sans outils)
     --rules "<consignes>" [-m <modèle>] [--effort <niveau>] [-s <uuid de session>]
```

Forme de la sortie JSON non documentée (lecture souple : `result`/`response`/`text`,
`session_id`). Syntaxe de `--disallowed-tools` à vérifier (S3). Doc :
https://docs.x.ai/build/cli/reference

## `codex`

```
codex exec [resume <thread>] --json --skip-git-repo-check --sandbox read-only
           -c mcp_servers={} -c approval_policy="never"
           -C <run> --output-last-message <fichier> [-m <m>] -
```

- Bac à sable `read-only` : aucun fichier écrit, pas de réseau pour le shell.
- Les dépôts lisibles sont listés dans l'invite (lecture autorisée partout en `read-only`).
- MCP : pas de liste de refus par motif → **connecteurs non activables** ; `-c mcp_servers={}`
  doit neutraliser les serveurs configurés : **À VÉRIFIER (S3)**.
- Sortie JSONL : `thread.started` (identifiant de reprise), `item.completed`
  (`agent_message`, `command_execution`, `mcp_tool_call`), `turn.failed` ; l'ancien format
  `{"msg": …}` est aussi lu. La réponse finale vient de `--output-last-message`.

## `gemini` (expérimental)

```
gemini --output-format json --approval-mode default
       --allowed-mcp-server-names steno-aucun-serveur [-m <m>]
       [--include-directories <dépôt>…] -p "Réponds à la demande ci-dessus…"   # invite sur stdin
```

En mode non interactif, les outils qui demandent une approbation (écriture, shell) sont
refusés ; aucun serveur MCP n'est autorisé. Sortie `{"response": …, "stats": …}` ou texte brut.

## `opencode` (expérimental)

```
opencode run --format json --agent steno-lecture|steno-sans-outil [-m fournisseur/modèle]
             [--session <id>] "<consigne courte>"     # invite sur stdin
```

`opencode.json` dans le dossier d'exécution : `edit` et `webfetch` refusés, `bash` refusé
sauf les commandes de lecture de `agent/policy.toml`, tous les autres outils (MCP compris)
coupés par `"*": false`. Agent `steno-sans-outil` (aucun outil) pour les tâches sans outil,
`steno-lecture` pour les recherches. Sortie JSON en flux (parties `text`) ou texte brut.

## `custom`

Modèle de commande saisi dans Réglages → Agent IA, lancé par `sh -c`. Variables :
`{prompt_file}` (fichier contenant consignes + invite ; sinon envoyées sur stdin),
`{model}`, `{run_dir}`. La sortie standard est la réponse. Aucun connecteur.

## Tester un adaptateur

```bash
steno doctor            # détection, version, politique (test local)
steno doctor --live     # + test réel : l'agent doit refuser « poste sur Slack »
cargo test -p steno-cli --test acceptance   # avec le faux `claude` de fixtures/fake-bin
```
