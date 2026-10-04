#!/usr/bin/env python3
"""Contrôle déterministe du binaire Windows x64, sans l'exécuter."""

import struct
import sys
from pathlib import Path


def check(path: Path) -> None:
    data = path.read_bytes()
    if len(data) < 0x100 or data[:2] != b"MZ":
        raise ValueError("Exécutable Windows manquant ou invalide")
    offset = struct.unpack_from("<I", data, 0x3C)[0]
    if offset + 24 + 70 > len(data) or data[offset : offset + 4] != b"PE\0\0":
        raise ValueError("En-tête PE invalide")
    optional_size = struct.unpack_from("<H", data, offset + 20)[0]
    if optional_size < 70 or offset + 24 + optional_size > len(data):
        raise ValueError("En-tête optionnel PE incomplet")
    machine = struct.unpack_from("<H", data, offset + 4)[0]
    magic = struct.unpack_from("<H", data, offset + 24)[0]
    subsystem = struct.unpack_from("<H", data, offset + 24 + 68)[0]
    if machine != 0x8664 or magic != 0x20B:
        raise ValueError("Le binaire n'est pas Windows x64")
    if subsystem != 2:
        raise ValueError(f"Une console apparaîtrait au lancement (sous-système {subsystem})")
    print(f"Binaire Windows x64 avec interface graphique : {path}")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("Usage : check-windows-pe.py PAROLE.EXE")
    try:
        check(Path(sys.argv[1]))
    except (OSError, ValueError) as exc:
        raise SystemExit(str(exc)) from exc
