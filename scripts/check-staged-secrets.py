#!/usr/bin/env python3
"""Contrôle pré-commit : signale des catégories, jamais les valeurs."""
import re
import subprocess
import sys

PATTERNS = {
    "affectation sensible": re.compile(rb"(?i)(?:api[_-]?key|token|secret|password|passwd)\s*[:=]\s*['\"][^'\"\r\n]{8,}['\"]"),
    "clé privée": re.compile(rb"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"),
    "jeton connu": re.compile(rb"(?:sk-[A-Za-z0-9]{20,}|gh[po]_[A-Za-z0-9]{36}|xox[bpras]-[A-Za-z0-9-]{24,})"),
}

def main() -> int:
    raw = subprocess.check_output(["git", "diff", "--cached", "--name-only", "-z"])
    paths = [p for p in raw.split(b"\0") if p]
    if not paths:
        print("Aucun fichier indexé : contrôle impossible")
        return 2
    alerts = []
    skipped = 0
    for path in paths:
        name = path.decode("utf-8", "replace")
        if any(part in {".env", "auth.json", "vault"} or part.endswith((".pem", ".key", ".p12")) for part in name.lower().split("/")):
            alerts.append((name, "fichier interdit"))
            continue
        data = subprocess.check_output(["git", "show", ":" + name])
        if b"\0" in data[:4096]:
            skipped += 1
            continue
        for category, pattern in PATTERNS.items():
            if pattern.search(data):
                alerts.append((name, category))
    for name, category in alerts:
        print(f"Présent : {category} dans {name}")
    print(f"Contrôle : {len(paths)} fichiers, {skipped} binaires exclus, {len(alerts)} alerte(s).")
    return 1 if alerts else 0

if __name__ == "__main__":
    sys.exit(main())