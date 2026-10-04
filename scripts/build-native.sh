#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
work="${PAROLE_BUILD_WORK:-${TMPDIR:?Définir TMPDIR vers un dossier temporaire autorisé}/parole-native-build}"
mkdir -p "$work" "$root/src-tauri/native"
llama_commit=70c4e1582e37e4fd94104eb09301711a0f2675bc
whisper_commit=371b5a7561823ab2bb32142d2751e35e7534727b
ffmpeg_sha=464beb5e7bf0c311e68b45ae2f04e9cc2af88851abb4082231742a74d97b524c
if [ ! -d "$work/whisper.cpp/.git" ]; then
  git clone --depth 1 --branch v1.9.3 https://github.com/ggml-org/whisper.cpp.git "$work/whisper.cpp"
fi
[ "$(git -C "$work/whisper.cpp" rev-parse HEAD)" = "$whisper_commit" ] || { printf 'Révision whisper.cpp inattendue\n' >&2; exit 1; }
if [ ! -d "$work/llama.cpp/.git" ]; then
  git clone --depth 1 --branch b11160 https://github.com/ggml-org/llama.cpp.git "$work/llama.cpp"
fi
[ "$(git -C "$work/llama.cpp" rev-parse HEAD)" = "$llama_commit" ] || { printf 'Révision llama.cpp inattendue\n' >&2; exit 1; }
if [ ! -f "$work/ffmpeg-8.1.2.tar.xz" ]; then
  curl -fL --retry 3 -o "$work/ffmpeg-8.1.2.tar.xz" https://ffmpeg.org/releases/ffmpeg-8.1.2.tar.xz
fi
if command -v sha256sum >/dev/null 2>&1; then
  printf '%s  %s\n' "$ffmpeg_sha" "$work/ffmpeg-8.1.2.tar.xz" | sha256sum -c -
else
  actual="$(shasum -a 256 "$work/ffmpeg-8.1.2.tar.xz" | cut -d ' ' -f 1)"
  [ "$actual" = "$ffmpeg_sha" ] || { printf 'Archive FFmpeg incorrecte\n' >&2; exit 1; }
fi
[ -d "$work/ffmpeg-8.1.2" ] || tar -xJf "$work/ffmpeg-8.1.2.tar.xz" -C "$work"
native_cmake=(-DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF -DGGML_OPENMP=OFF -DGGML_NATIVE=OFF -DWHISPER_BUILD_TESTS=OFF)
if [ "$(uname -s)" = Darwin ]; then
  [ "$(uname -m)" = arm64 ] || { printf 'Mac Apple Silicon arm64 requis pour ce paquet\n' >&2; exit 1; }
  # Conserver le shader dans l'exécutable livré et utiliser le GPU Apple.
  native_cmake+=(-DGGML_METAL=ON -DGGML_METAL_EMBED_LIBRARY=ON)
else
  native_cmake+=(-DGGML_METAL=OFF)
fi
native_ffmpeg=(--disable-network)
case "$(uname -s)" in
  MINGW*|MSYS*) ext=.exe
                 # Candidate privée pour le PC Windows x64 compatible AVX2 ; ne pas livrer à un processeur plus ancien.
                 native_cmake+=(-DGGML_AVX=ON -DGGML_AVX2=ON -DGGML_SSE42=ON -DGGML_F16C=ON -DGGML_FMA=ON)
                 native_cmake+=("-DCMAKE_C_FLAGS=-D_WIN32_WINNT=0x0A00" "-DCMAKE_CXX_FLAGS=-D_WIN32_WINNT=0x0A00" "-DCMAKE_EXE_LINKER_FLAGS=-static -static-libgcc -static-libstdc++")
                 native_ffmpeg+=("--extra-cflags=-D_WIN32_WINNT=0x0A00" "--extra-ldflags=-static") ;;
  *) ext= ;;
esac
cmake -S "$work/whisper.cpp" -B "$work/whisper-build" "${native_cmake[@]}"
cmake --build "$work/whisper-build" --config Release --parallel 4 --target whisper-cli
cmake -S "$work/llama.cpp" -B "$work/llama-build" "${native_cmake[@]}" -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_EXAMPLES=ON -DLLAMA_OPENSSL=OFF -DGGML_BLAS=OFF
cmake --build "$work/llama-build" --config Release --parallel 4 --target llama-completion
(
  cd "$work/ffmpeg-8.1.2"
  ./configure --disable-gpl --disable-nonfree --disable-version3 --disable-doc --disable-ffplay --disable-avdevice --disable-debug --disable-x86asm --disable-iconv --enable-static --disable-shared --disable-autodetect "${native_ffmpeg[@]}"
  make -j4 "ffmpeg$ext" "ffprobe$ext"
)
whisper_bin="$work/whisper-build/bin/whisper-cli$ext"
[ -f "$whisper_bin" ] || whisper_bin="$work/whisper-build/bin/Release/whisper-cli$ext"
[ -f "$whisper_bin" ] || { printf 'whisper-cli absent\n' >&2; exit 1; }
for name in ffmpeg ffprobe; do
  [ -x "$work/ffmpeg-8.1.2/$name$ext" ] || { printf '%s absent\n' "$name" >&2; exit 1; }
  cp "$work/ffmpeg-8.1.2/$name$ext" "$root/src-tauri/native/$name$ext"
done
cp "$whisper_bin" "$root/src-tauri/native/whisper-cli$ext"
llama_bin="$work/llama-build/bin/llama-completion$ext"
[ -f "$llama_bin" ] || llama_bin="$work/llama-build/bin/Release/llama-completion$ext"
[ -f "$llama_bin" ] || { printf 'llama-completion absent\n' >&2; exit 1; }
cp "$llama_bin" "$root/src-tauri/native/llama-completion$ext"
cp "$work/llama.cpp/LICENSE" "$root/src-tauri/native/llama-MIT"
cp "$work/whisper.cpp/LICENSE" "$root/src-tauri/native/whisper-LICENSE"
cp "$work/ffmpeg-8.1.2/COPYING.LGPLv2.1" "$root/src-tauri/native/ffmpeg-LGPL-2.1"
cp "$work/ffmpeg-8.1.2/LICENSE.md" "$root/src-tauri/native/ffmpeg-LICENSE.md"
printf 'Binaires locaux préparés pour %s (licences incluses).\n' "$(uname -s)"
