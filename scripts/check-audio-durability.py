#!/usr/bin/env python3
"""Preuve Linux des barrières disque, sur un unique test fictif de Parole.

Observe les appels système ; ne simule pas une coupure électrique et ne prouve
pas la durabilité physique d'un périphérique qui ignore les demandes de flush.
"""
import argparse
import json
from pathlib import Path
import re
import subprocess
import sys

TEST = "conserve_exactement_le_wav_lu_par_whisper_apres_persistance"


def verify_trace(text):
    events = []
    for line in text.splitlines():
        synced = re.search(r"fsync\(\d+<([^>]+)>\)\s+= 0$", line)
        if synced:
            events.append(("sync", synced.group(1), None))
        elif re.search(r"\brename(?:at2|at)?\(.*\)\s+= 0$", line):
            paths = re.findall(r'"((?:[^"\\]|\\.)*)"', line)
            if len(paths) == 2:
                events.append(("rename", *paths))
    publications = {}
    for suffix in ("tranche-00000000.wav", "tranche-00000000.audio.json"):
        matches = [(i, event) for i, event in enumerate(events)
                   if event[0] == "rename" and event[2].endswith("/" + suffix)]
        if len(matches) != 1:
            raise ValueError(f"Publication atomique unique absente : {suffix}")
        index, (_, temporary, target) = matches[0]
        parent = target.rsplit("/", 1)[0]
        before = [i for i, event in enumerate(events[:index])
                  if event == ("sync", temporary, None)]
        after = [i for i, event in enumerate(events)
                 if i > index and event == ("sync", parent, None)]
        if not before or not after:
            raise ValueError(f"Fichier temporaire ou répertoire non synchronisé : {suffix}")
        publications[suffix] = (before[-1], index, after[0], parent)
    wave = publications["tranche-00000000.wav"]
    receipt = publications["tranche-00000000.audio.json"]
    if wave[3] != receipt[3] or not wave[2] < receipt[0]:
        raise ValueError("Le WAV n'est pas publié durablement avant son reçu")
    states = [i for i, event in enumerate(events) if i > receipt[1]
              and event == ("sync", receipt[3] + "/travail.tmp", None)]
    if not states or not receipt[2] < states[0]:
        raise ValueError("L'état du travail est sauvegardé avant la fin des barrières audio")
    if not any(i > states[0] and event == ("rename", receipt[3] + "/travail.tmp", receipt[3] + "/travail.json")
               for i, event in enumerate(events)):
        raise ValueError("Confirmation du travail absente de la trace")
    return {"wave_synced": True, "receipt_synced": True,
            "directories_synced": True, "before_job_confirmation": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--trace", type=Path, required=True)
    parser.add_argument("--test-bin", type=Path)
    args = parser.parse_args()
    if args.test_bin:
        if not sys.platform.startswith("linux"):
            parser.error("Cette observation utilise strace sous Linux uniquement")
        if args.trace.exists():
            parser.error("La trace existe déjà : choisir un nouveau nom")
        if not args.test_bin.is_absolute() or not args.test_bin.is_file():
            parser.error("Indiquer le binaire absolu du test Rust déjà construit")
        completed = subprocess.run(
            ["/usr/bin/strace", "-f", "-yy", "-s", "1024", "-e",
             "trace=fsync,fdatasync,rename,renameat,renameat2", "-o", str(args.trace),
             str(args.test_bin), TEST, "--exact", "--test-threads=1"],
            capture_output=True, text=True, timeout=90, check=False,
        )
        if completed.returncode or f"test {TEST} ... ok" not in completed.stdout \
                or "test result: ok. 1 passed; 0 failed;" not in completed.stdout:
            print("Le test réel ciblé n'a pas réussi", file=sys.stderr)
            return 1
    if args.trace.stat().st_size > 2 * 1024 * 1024:
        parser.error("Trace trop volumineuse pour ce contrôle borné")
    try:
        result = verify_trace(args.trace.read_text())
    except ValueError as error:
        print(str(error), file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
