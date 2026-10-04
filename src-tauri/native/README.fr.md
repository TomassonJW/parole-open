# Ressources natives

[English](README.md) | [Français](README.fr.md)

Un paquet de bureau exige `ffmpeg`, `ffprobe`, `whisper-cli` et
`llama-completion` (avec `.exe` sous Windows), sherpa-onnx et ses bibliothèques
ONNX Runtime, `segmentation.onnx`, `embedding.onnx` et leurs notices de licence.
Binaires et poids ne sont pas versionnés dans Git.

Les scripts de la racine `scripts/build-native.sh` et
`scripts/fetch-diarization-assets.sh` préparent explicitement ces ressources
fixées et vérifiées. Inspecter prérequis, contenu, périmètre et licences avant
exécution. La préparation des modèles de parole/langue dans l'application est
également explicite. Un dossier complet ne prouve pas que l'application
installée fonctionne.

Lire [BUILD.fr.md](../../BUILD.fr.md) et
[THIRD_PARTY_LICENSES.fr.md](../../THIRD_PARTY_LICENSES.fr.md) avant empaquetage.
Ne pas diffuser sans contrôles d'installation, d'usage et de licences sur le
système cible. Aucun contournement de politique de sécurité n'est recommandé.
