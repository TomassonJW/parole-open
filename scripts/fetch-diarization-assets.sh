#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
work="${PAROLE_BUILD_WORK:-${TMPDIR:?Définir TMPDIR vers un dossier temporaire autorisé}/parole-native-build}"
mkdir -p "$work" "$root/src-tauri/native"
case "$(uname -s):$(uname -m)" in
  Linux:x86_64) variant=linux-x64-shared-no-tts-lib; digest=bf2d998c8b07012cd5098f3b92673bc1333fd9b927767d7cb664be8190d8bc0b ;;
  Darwin:arm64) variant=osx-arm64-shared-no-tts-lib; digest=f3e0cbd86cc3f38dad30c97921b40e9a8bcc6f2c943777eb76ad77176993e417 ;;
  Darwin:x86_64) variant=osx-x64-shared-no-tts-lib; digest=c4d94cce92b6e04df1f17d247c3ac4e61b21359be80e940a0408e039e36a4d7e ;;
  MINGW*:x86_64|MSYS*:x86_64) variant=win-x64-shared-MT-Release-no-tts-lib; digest=a1253e665c4f236119c443c8932a8acfca32a546c78c05962d483c9a0eae21b7 ;;
  *) printf 'Architecture non prise en charge\n' >&2; exit 1 ;;
esac
verify_sha() {
  local found
  if command -v sha256sum >/dev/null 2>&1; then found="$(sha256sum "$1" | cut -d ' ' -f 1)";
  else found="$(shasum -a 256 "$1" | cut -d ' ' -f 1)"; fi
  [ "$found" = "$2" ] || { printf 'Empreinte incorrecte : %s\n' "$1" >&2; exit 1; }
}
archive="sherpa-onnx-v1.13.8-$variant.tar.bz2"
[ -f "$work/$archive" ] || curl -fL --retry 3 -o "$work/$archive" "https://github.com/k2-fsa/sherpa-onnx/releases/download/v1.13.8/$archive"
verify_sha "$work/$archive" "$digest"
for member in $(tar -tf "$work/$archive"); do case "$member" in /*|*../*) printf 'Archive dangereuse\n' >&2; exit 1;; esac; done
tar -xjf "$work/$archive" -C "$work"
seg_url='https://huggingface.co/csukuangfj/sherpa-onnx-pyannote-segmentation-3-0/resolve/9403a6902bb58e3d5ae8c7e77c3422de279db2e0/model.onnx'
emb_url='https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_eres2net_sv_en_voxceleb_16k.onnx'
[ -f "$work/segmentation.onnx" ] || curl -fL --retry 3 -o "$work/segmentation.onnx" "$seg_url"
[ -f "$work/embedding.onnx" ] || curl -fL --retry 3 -o "$work/embedding.onnx" "$emb_url"
verify_sha "$work/segmentation.onnx" 220ad67ca923bef2fa91f2390c786097bf305bceb5e261d4af67b38e938e1079
verify_sha "$work/embedding.onnx" c59158379255ad66e161679cca6af8d52d51e389e3224ab7d7a7baae295c2db5
cp "$work/segmentation.onnx" "$work/embedding.onnx" "$root/src-tauri/native/"
libdir="$work/sherpa-onnx-v1.13.8-$variant/lib"
case "$variant" in
  win-*) cp "$libdir/sherpa-onnx-c-api.dll" "$libdir/onnxruntime.dll" "$libdir/onnxruntime_providers_shared.dll" "$root/src-tauri/native/" ;;
  osx-*) cp "$libdir/libsherpa-onnx-c-api.dylib" "$libdir/libonnxruntime.dylib" "$root/src-tauri/native/" ;;
  linux-*) cp "$libdir/libsherpa-onnx-c-api.so" "$libdir/libonnxruntime.so" "$root/src-tauri/native/" ;;
esac
curl -fL --retry 3 -o "$root/src-tauri/native/segmentation-MIT" 'https://huggingface.co/csukuangfj/sherpa-onnx-pyannote-segmentation-3-0/resolve/9403a6902bb58e3d5ae8c7e77c3422de279db2e0/LICENSE'
verify_sha "$root/src-tauri/native/segmentation-MIT" 14d7016ad68e7394d6e6b78d96cc2ae431c905287b89674cfdf021e79e62b8ba
curl -fL --retry 3 -o "$root/src-tauri/native/sherpa-Apache-2.0" 'https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/v1.13.8/LICENSE'
verify_sha "$root/src-tauri/native/sherpa-Apache-2.0" cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30
curl -fL --retry 3 -o "$root/src-tauri/native/eres2net-Apache-2.0" 'https://raw.githubusercontent.com/modelscope/3D-Speaker/065629c313eaf1a01c65c640c46d77e61e9607b4/LICENSE'
verify_sha "$root/src-tauri/native/eres2net-Apache-2.0" c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4
curl -fL --retry 3 -o "$root/src-tauri/native/onnxruntime-MIT" 'https://raw.githubusercontent.com/microsoft/onnxruntime/ae72e3efb6966ba435a5c46d0b2f8e88fc029280/LICENSE'
verify_sha "$root/src-tauri/native/onnxruntime-MIT" 2f07c72751aed99790b8a4869cf2311df85a860b22ded05fa22803587a48922c
curl -fL --retry 3 -o "$root/src-tauri/native/whisper-model-MIT" 'https://raw.githubusercontent.com/openai/whisper/86098128c0b4f24f0e2aa2994de830614b474227/LICENSE'
verify_sha "$root/src-tauri/native/whisper-model-MIT" b5d65a59060e68c4ff940e1eddfa6f94b2d68fdf58ed7f4dd57721c997e35e9d
curl -fL --retry 3 -o "$root/src-tauri/native/qwen-Apache-2.0" 'https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/91cad51170dc346986eccefdc2dd33a9da36ead9/LICENSE'
verify_sha "$root/src-tauri/native/qwen-Apache-2.0" 832dd9e00a68dd83b3c3fb9f5588dad7dcf337a0db50f7d9483f310cd292e92e
cp "$root/THIRD_PARTY_LICENSES.md" "$root/src-tauri/native/THIRD_PARTY_LICENSES.md"
printf 'Bibliothèques et modèles de locuteurs préparés pour %s.\n' "$variant"
