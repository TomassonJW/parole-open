#!/usr/bin/env python3
"""Garde statique des outils locaux de l'application GUI Windows.

Ne remplace pas l'essai installé : chaque commande doit être configurée avant
son exécution, même si le nombre total d'appels au garde reste inchangé.
"""

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SITES = {
    "src-core/src/native.rs": 3,
    "src-core/src/language.rs": 1,
    "src-core/src/diarization.rs": 1,
}
CREATION = re.compile(r"Command::new\(")
PROTECTED = re.compile(
    r"let\s+mut\s+(?P<variable>[A-Za-z_]\w*)\s*=\s*"
    r"(?P<creation>Command::new\([^;\n]*\));\s*"
    r"suppress_child_console\(&mut\s+(?P=variable)\);"
)


def unprotected(text: str) -> list[int]:
    creations = {match.start() for match in CREATION.finditer(text)}
    guarded = {match.start("creation") for match in PROTECTED.finditer(text)}
    return sorted(creations - guarded)


def main() -> None:
    valid = "let mut a = Command::new(p);\nsuppress_child_console(&mut a);"
    same_count_but_wrong = (
        valid + "\nlet mut b = Command::new(p);\nsuppress_child_console(&mut a);"
    )
    if unprotected(valid) or len(unprotected(same_count_but_wrong)) != 1:
        raise SystemExit("Le contrôle des sites de lancement est lui-même défectueux")

    seen = set()
    for source_dir in ("src-core/src", "src-tauri/src"):
        for path in (ROOT / source_dir).rglob("*.rs"):
            text = path.read_text(encoding="utf-8")
            count = len(CREATION.findall(text))
            if count == 0:
                continue
            name = str(path.relative_to(ROOT))
            seen.add(name)
            if count != SITES.get(name) or unprotected(text):
                raise SystemExit(
                    f"{name} : {count} commandes, {len(unprotected(text))} sans garde "
                    "immédiat ; vérifier chaque nouveau lancement Windows"
                )
    if seen != set(SITES):
        raise SystemExit("Inventaire des sites de lancement Windows incomplet")
    helper = (ROOT / "src-core/src/child_process.rs").read_text(encoding="utf-8")
    if "#[cfg(windows)]" not in helper or "creation_flags(CREATE_NO_WINDOW)" not in helper:
        raise SystemExit("Drapeau de création Windows manquant")
    print("Cinq lancements d'outils locaux vérifiés pour l'application Windows")


if __name__ == "__main__":
    main()
