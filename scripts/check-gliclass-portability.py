#!/usr/bin/env python3
"""Contrôle du graphe réel. Ce contrôle seul n’est pas une compilation."""
import argparse
import json
from pathlib import Path
import subprocess

def inspect_graph(metadata):
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    todo = [metadata["resolve"]["root"]]
    seen = set()
    while todo:
        identity = todo.pop()
        if identity in seen:
            continue
        seen.add(identity)
        todo.extend(d["pkg"] for d in nodes[identity]["deps"])
    names = {packages[i]["name"] for i in seen}
    forbidden = {"onig", "onig_sys", "hf-hub", "reqwest", "ureq"} & names
    if forbidden:
        raise ValueError("Dépendance non portable ou accès réseau activé : " + ", ".join(sorted(forbidden)))
    tokenizer = [nodes[i] for i in seen if packages[i]["name"] == "tokenizers"]
    if len(tokenizer) != 1:
        raise ValueError("Graphe du tokenizer inattendu.")
    features = set(tokenizer[0]["features"])
    if "fancy-regex" not in features or features & {"onig", "http", "hf-hub", "default", "esaxx_fast", "unstable_wasm"}:
        raise ValueError("Options de découpage non qualifiées.")
    ort_features = {f for i in seen if packages[i]["name"] in {"ort", "ort-sys"} for f in nodes[i]["features"]}
    if "download-binaries" in ort_features:
        raise ValueError("Le téléchargement automatique du moteur est interdit.")
    return {"portable_regex_graph": True, "package_count": len(seen), "tokenizer_features": sorted(features), "ort_features": sorted(ort_features), "native_build_proven_by_this_check": False}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--target", default="x86_64-pc-windows-gnu")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    run = subprocess.run([args.cargo, "metadata", "--manifest-path", str(root / "src-gliclass/Cargo.toml"), "--locked", "--offline", "--format-version", "1", "--filter-platform", args.target], capture_output=True, text=True, check=True)
    print(json.dumps(inspect_graph(json.loads(run.stdout)), indent=2, ensure_ascii=False))

if __name__ == "__main__":
    main()
