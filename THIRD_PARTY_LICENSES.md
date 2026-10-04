# Third-party components

[English](THIRD_PARTY_LICENSES.md) | [Français](THIRD_PARTY_LICENSES.fr.md)

Parole publishes original source code, not copies of all dependency sources,
native executables or model weights. Upstream rights remain separate from
Parole's MIT/Apache licences. The model/source URLs in MODEL_MANIFEST.json and
below are public download references, not bundled assets.

## Engine and model references

The preserved upstream table uses the original French component descriptions.
Licence identifiers and pinned source URLs are language-independent.

| Élément | Usage | Licence amont | Source |
|---|---|---|---|
| Tauri 2 | coque de bureau | Apache-2.0 ou MIT | https://github.com/tauri-apps/tauri/blob/dev/LICENSE_APACHE-2.0 |
| React | interface | MIT | https://github.com/facebook/react/blob/main/LICENSE |
| whisper.cpp v1.9.3 (test local) | transcription native | MIT | https://github.com/ggml-org/whisper.cpp/blob/v1.9.3/LICENSE |
| Whisper large-v3-turbo Q5_0 | poids de transcription | MIT (OpenAI Whisper) | https://github.com/openai/whisper/blob/main/LICENSE |
| FFmpeg 8.1.2 | décodage audio/vidéo et analyse de durée | LGPL-2.1-or-later, configuration sans GPL/nonfree, version3 ni réseau | https://ffmpeg.org/legal.html et https://ffmpeg.org/releases/ffmpeg-8.1.2.tar.xz |
| sherpa-onnx v1.13.8 | séparation locale des locuteurs (bibliothèque C) | Apache-2.0 | https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.8/LICENSE |
| ONNX Runtime inclus dans le paquet sherpa-onnx | exécution des modèles | MIT | https://github.com/microsoft/onnxruntime/blob/main/LICENSE |
| Pyannote segmentation 3.0, conversion ONNX publique | plages de parole et locuteurs | MIT, notice CNRS | https://huggingface.co/csukuangfj/sherpa-onnx-pyannote-segmentation-3-0 |
| 3D-Speaker ERes2Net VoxCeleb | empreintes de voix | Apache-2.0, métadonnée du modèle iic/ModelScope ; texte `eres2net-Apache-2.0` inclus dans les ressources | https://www.modelscope.cn/api/v1/models/iic/speech_eres2net_sv_en_voxceleb_16k et https://github.com/modelscope/3D-Speaker/blob/065629c313eaf1a01c65c640c46d77e61e9607b4/LICENSE |
| llama.cpp b11160 | traduction locale par processeur | MIT | https://github.com/ggml-org/llama.cpp/blob/b11160/LICENSE |
| Qwen2.5-1.5B-Instruct Q4_K_M | modèle linguistique quantifié | Apache-2.0 | https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/tree/91cad51170dc346986eccefdc2dd33a9da36ead9 |
| zip 2.4.2 | export Word (archive Office) | MIT | https://crates.io/crates/zip/2.4.2 |

| ort / ort-sys 2.0.0-rc.11 | ONNX bindings | MIT OR Apache-2.0 (resolved metadata) | https://github.com/pykeio/ort |
| tokenizers 0.22.2 | tokenizer | Apache-2.0 | https://github.com/huggingface/tokenizers |

## Locked code dependency inventory

[docs/dependencies.json](docs/dependencies.json) records the declared licence
expressions of the resolved Rust lockfile graph and the locally installed pnpm
packages. The inventory records metadata, not a legal certificate. Its scope
includes build/test and platform-conditional Rust packages; the JavaScript list
covers the installed Linux graph, not every optional platform binary.

All recorded packages have licence metadata. Expressions including `OR` offer
an upstream choice; `AND` requires all stated conditions. Older slash-separated
metadata must be checked against upstream texts rather than interpreted as a
blanket exemption. Some dependencies use MPL-2.0, ISC, BSD, Unicode/data licences,
BlueOak and other licences: they are not changed to MIT.

## Before redistributing binaries or weights

Verify the actual files, transitive native libraries, build options, complete
notices and any corresponding-source/linking obligations. FFmpeg's LGPL status
requires the documented non-GPL/nonfree configuration; a different build may
have different obligations. Model conversions/quantisations need their own
provenance checks, not only the licence of a similarly named original model.
Keep original copyright/NOTICE material. No public installer is approved by this
source inventory; no model download is needed for the source verification suite.
