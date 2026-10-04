# Native resources

[English](README.md) | [Français](README.fr.md)

Desktop packages require `ffmpeg`, `ffprobe`, `whisper-cli` and
`llama-completion` (with `.exe` on Windows), sherpa-onnx and its ONNX Runtime
libraries, `segmentation.onnx`, `embedding.onnx` and their licence notices.
Binaries and weights are not checked into Git.

The root scripts `scripts/build-native.sh` and
`scripts/fetch-diarization-assets.sh` prepare these explicitly from pinned,
verified sources. Inspect them, their prerequisites, scope and licences before
execution. Speech/language model preparation inside the app is also explicit.
A complete directory does not prove an installed application works.

Read [BUILD.md](../../BUILD.md) and
[THIRD_PARTY_LICENSES.md](../../THIRD_PARTY_LICENSES.md) before packaging. Do not
distribute a package without target-system installation, operation and dependency
licence checks. No security-policy bypass is recommended.
