#!/usr/bin/env python3
"""Refuse les dépendances externes des binaires natifs d'un paquet macOS."""
import pathlib
import struct
import sys

LOADS = {0xC, 0x80000018, 0x8000001F, 0x80000023}
RPATH = 0x8000001C
MACHO64 = 0xFEEDFACF
REQUIRED_NATIVE = {"ffmpeg", "ffprobe", "whisper-cli", "llama-completion", "libsherpa-onnx-c-api.dylib", "libonnxruntime.dylib"}


def commands(path):
    data = path.read_bytes()
    if len(data) < 32 or struct.unpack_from("<I", data)[0] != MACHO64:
        raise ValueError(f"Format macOS inattendu : {path.name}")
    ncmds, sizeofcmds = struct.unpack_from("<II", data, 16)
    if ncmds > 10000 or 32 + sizeofcmds > len(data):
        raise ValueError(f"En-tête macOS invalide : {path.name}")
    offset = 32
    for _ in range(ncmds):
        cmd, size = struct.unpack_from("<II", data, offset)
        if size < 12 or offset + size > len(data):
            raise ValueError(f"Commande macOS invalide : {path.name}")
        if cmd in LOADS or cmd == RPATH:
            start = offset + struct.unpack_from("<I", data, offset + 8)[0]
            if start >= offset + size:
                raise ValueError(f"Nom de bibliothèque invalide : {path.name}")
            end = data.find(b"\0", start, offset + size)
            if end < 0:
                raise ValueError(f"Nom de bibliothèque non terminé : {path.name}")
            yield cmd, data[start:end].decode("utf-8")
        offset += size


def main():
    if len(sys.argv) != 2:
        raise ValueError("Indiquez le chemin de Parole.app")
    app = pathlib.Path(sys.argv[1]).resolve()
    contents = app / "Contents"
    native = contents / "Resources" / "native"
    files = [contents / "MacOS" / "parole-desktop"] + sorted(
        p for p in native.iterdir() if p.name in REQUIRED_NATIVE or p.suffix == ".dylib"
    )
    if not REQUIRED_NATIVE.issubset({p.name for p in files}) or any(not p.is_file() for p in files):
        raise ValueError("Moteurs natifs macOS incomplets")
    problems = []
    for binary in files:
        for cmd, name in commands(binary):
            if cmd == RPATH and name == "@loader_path":
                continue
            if name.startswith(("/System/Library/", "/usr/lib/")):
                continue
            if name.startswith("@rpath/") and (native / name[7:]).is_file():
                continue
            if name.startswith("@loader_path/") and (binary.parent / name[13:]).is_file():
                continue
            if name.startswith("@executable_path/") and (contents / "MacOS" / name[17:]).is_file():
                continue
            problems.append(f"{binary.name}: {name} (commande {cmd:#x})")
    if problems:
        raise ValueError("Dépendances non empaquetées : " + "; ".join(problems))
    print(f"Dépendances macOS vérifiées : {len(files)} binaires, aucune bibliothèque externe")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, UnicodeError) as exc:
        print(str(exc), file=sys.stderr)
        sys.exit(1)
