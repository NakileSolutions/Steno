# Composants tiers et modèles

Sténo est distribué sous licence MIT (voir [`LICENSE`](LICENSE)). Il s'appuie sur les
composants et modèles ci-dessous, chacun sous sa propre licence.

## Modèles d'IA audio

Les modèles ne sont **pas inclus dans ce dépôt** : ils sont téléchargés depuis Hugging Face au
premier lancement (ou par `make install`) et restent sur votre Mac
(`~/Library/Application Support/FluidAudio/Models`). Ils tournent entièrement en local.

| Usage dans Sténo | Modèle | Auteur | Licence |
|---|---|---|---|
| Transcription en direct | Parakeet TDT 0.6B v3 (conversion CoreML [`FluidInference/parakeet-tdt-0.6b-v3-coreml`](https://huggingface.co/FluidInference/parakeet-tdt-0.6b-v3-coreml)) | NVIDIA | CC-BY-4.0 |
| Retranscription de fin de réunion | Canary 1B v2 (conversion CoreML [`FluidInference/canary-1b-v2-coreml`](https://huggingface.co/FluidInference/canary-1b-v2-coreml)) | NVIDIA | CC-BY-4.0 |
| Détection de la parole | Silero VAD (conversion CoreML [`FluidInference/silero-vad-coreml`](https://huggingface.co/FluidInference/silero-vad-coreml)) | Silero Team | MIT |
| Identification des intervenants | pyannote speaker-diarization community-1 (conversion CoreML [`FluidInference/speaker-diarization-coreml`](https://huggingface.co/FluidInference/speaker-diarization-coreml)) | pyannote | CC-BY-4.0 |

Attribution CC-BY-4.0 : les modèles Parakeet TDT 0.6B v3 et Canary 1B v2 sont © NVIDIA
Corporation ; pyannote speaker-diarization-community-1 est © pyannoteAI. Ils sont utilisés
après conversion au format CoreML par FluidInference, sans autre modification.

## Bibliothèques principales

| Composant | Licence |
|---|---|
| [FluidAudio](https://github.com/FluidInference/FluidAudio) (ASR, VAD, diarisation sur le Neural Engine) | Apache-2.0 |
| [Tauri](https://tauri.app) (application) | MIT / Apache-2.0 |
| [Svelte](https://svelte.dev), [Vite](https://vitejs.dev) (interface) | MIT |
| [marked](https://marked.js.org), [DOMPurify](https://github.com/cure53/DOMPurify) (rendu Markdown) | MIT, Apache-2.0 / MPL-2.0 |
| [rusqlite](https://github.com/rusqlite/rusqlite) / SQLite (base locale) | MIT / domaine public |
| [calamine](https://github.com/tafia/calamine) (lecture des classeurs) | MIT |
| [pdf-extract](https://github.com/jrmuizel/pdf-extract) (lecture des PDF) | MIT |

La liste complète des dépendances et de leurs licences se trouve dans `Cargo.lock`,
`app/ui/package-lock.json` et `audio/StenoAudio/Package.resolved`.

## Agents IA

Sténo ne contient aucun modèle de langage : il pilote le CLI d'agent que vous avez installé
(Claude Code, Codex, Gemini CLI, OpenCode…), avec votre propre abonnement et selon les
conditions de son éditeur.
