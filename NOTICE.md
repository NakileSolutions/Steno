# Third-party components and models

Sténo is distributed under the MIT license (see [`LICENSE`](LICENSE)). It relies on the
components and models below, each under its own license.

## Audio AI models

The models are **not included in this repository**: they are downloaded from Hugging Face on
first launch (or by `make install`) and stay on your computer
(macOS: `~/Library/Application Support/FluidAudio/Models`). They run entirely locally.

| Use in Sténo | Model | Author | License |
|---|---|---|---|
| Live transcription | Parakeet TDT 0.6B v3 (CoreML conversion [`FluidInference/parakeet-tdt-0.6b-v3-coreml`](https://huggingface.co/FluidInference/parakeet-tdt-0.6b-v3-coreml)) | NVIDIA | CC-BY-4.0 |
| End-of-meeting re-transcription | Canary 1B v2 (CoreML conversion [`FluidInference/canary-1b-v2-coreml`](https://huggingface.co/FluidInference/canary-1b-v2-coreml)) | NVIDIA | CC-BY-4.0 |
| Speech detection | Silero VAD (CoreML conversion [`FluidInference/silero-vad-coreml`](https://huggingface.co/FluidInference/silero-vad-coreml)) | Silero Team | MIT |
| Speaker identification | pyannote speaker-diarization community-1 (CoreML conversion [`FluidInference/speaker-diarization-coreml`](https://huggingface.co/FluidInference/speaker-diarization-coreml)) | pyannote | CC-BY-4.0 |

CC-BY-4.0 attribution: the Parakeet TDT 0.6B v3 and Canary 1B v2 models are © NVIDIA
Corporation; pyannote speaker-diarization-community-1 is © pyannoteAI. They are used
after conversion to the CoreML format by FluidInference, without any other modification.

### Linux

On Linux, the `steno-audio` sidecar (`crates/steno-audio-linux`) uses the ONNX conversions
published by the sherpa-onnx project (downloaded from its GitHub releases, stored in
`~/.local/share/Steno/models`):

| Use in Sténo | Model | Author | License |
|---|---|---|---|
| Live transcription | Parakeet TDT 0.6B v3, int8 ([`sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8`](https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8)) | NVIDIA | CC-BY-4.0 |
| End-of-meeting re-transcription | Canary 180M Flash, int8 ([`sherpa-onnx-nemo-canary-180m-flash-en-es-de-fr-int8`](https://huggingface.co/csukuangfj/sherpa-onnx-nemo-canary-180m-flash-en-es-de-fr-int8)) | NVIDIA | CC-BY-4.0 |
| Speech detection | Silero VAD (ONNX) | Silero Team | MIT |
| Speaker identification | pyannote segmentation 3.0 (ONNX) | pyannote | MIT |
| Speaker identification | 3D-Speaker ERes2Net base (ONNX) | Alibaba (3D-Speaker) | Apache-2.0 |

CC-BY-4.0 attribution: Parakeet TDT 0.6B v3 and Canary 180M Flash are © NVIDIA Corporation,
used after ONNX conversion and int8 quantization by the sherpa-onnx project, without any other
modification.

## Main libraries

| Component | License |
|---|---|
| [FluidAudio](https://github.com/FluidInference/FluidAudio) (ASR, VAD, diarization on the Neural Engine) | Apache-2.0 |
| [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) and [ONNX Runtime](https://onnxruntime.ai) (ASR, VAD, diarization on Linux) | Apache-2.0, MIT |
| [WebRTC Audio Processing](https://gitlab.freedesktop.org/pulseaudio/webrtc-audio-processing) and [Abseil](https://abseil.io) (echo cancellation on Linux) | BSD-3-Clause, Apache-2.0 |
| [libpulse-binding](https://github.com/jnqnfe/pulse-binding-rust) / PulseAudio (capture on Linux) | MIT / Apache-2.0, LGPL-2.1 |
| [x11rb](https://github.com/psychon/x11rb) (window titles on Linux) | MIT / Apache-2.0 |
| [Tauri](https://tauri.app) (application) | MIT / Apache-2.0 |
| [Svelte](https://svelte.dev), [Vite](https://vitejs.dev) (UI) | MIT |
| [marked](https://marked.js.org), [DOMPurify](https://github.com/cure53/DOMPurify) (Markdown rendering) | MIT, Apache-2.0 / MPL-2.0 |
| [rusqlite](https://github.com/rusqlite/rusqlite) / SQLite (local database) | MIT / public domain |
| [calamine](https://github.com/tafia/calamine) (spreadsheet reading) | MIT |
| [pdf-extract](https://github.com/jrmuizel/pdf-extract) (PDF reading) | MIT |

The full list of dependencies and their licenses is in `Cargo.lock`,
`app/ui/package-lock.json` and `audio/StenoAudio/Package.resolved`.

## AI agents

Sténo contains no language model: it drives the agent CLI you have installed
(Claude Code, Codex, Gemini CLI, OpenCode…), with your own subscription and under its
vendor's terms.
