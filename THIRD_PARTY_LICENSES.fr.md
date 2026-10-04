# Composants tiers

[English](THIRD_PARTY_LICENSES.md) | [Français](THIRD_PARTY_LICENSES.fr.md)

Parole publie ses sources originales, pas une copie des sources de toutes les
dépendances, des moteurs natifs ou des poids de modèles. Les droits amont restent
distincts de MIT/Apache pour Parole. Les liens de MODEL_MANIFEST.json et du
tableau sont des références publiques de téléchargement, pas des fichiers inclus.

## Références des moteurs et modèles

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

## Inventaire des dépendances verrouillées

[docs/dependencies.json](docs/dependencies.json) relève les licences déclarées du
graphe Rust verrouillé et des paquets pnpm installés localement. Il décrit des
métadonnées, pas un certificat juridique. Il comprend les dépendances Rust de
construction/test et spécifiques aux plateformes ; la liste JavaScript couvre
le graphe installé sous Linux, pas tous les binaires optionnels d'autres OS.

Chaque paquet relevé déclare une licence. `OR` propose un choix amont ; `AND`
impose toutes les conditions. Les anciennes expressions avec une barre oblique
doivent être rapprochées des textes amont, pas interprétées comme une dispense.
Certaines dépendances utilisent MPL-2.0, ISC, BSD, des licences Unicode/données,
BlueOak et d'autres licences : elles ne deviennent pas MIT.

## Avant de redistribuer des binaires ou poids

Vérifier les fichiers réels, bibliothèques natives transitives, options de
construction, notices complètes et obligations de sources/liaison. Le statut
LGPL de FFmpeg suppose la configuration sans GPL/nonfree décrite ; un autre
paquet peut avoir d'autres obligations. Une conversion/quantification de modèle
demande sa propre provenance, pas seulement la licence d'un modèle du même nom.
Conserver les attributions et NOTICE amont. Cet inventaire des sources ne valide
aucun installateur public ; la suite de vérification ne télécharge pas de modèles.
