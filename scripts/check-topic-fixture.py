#!/usr/bin/env python3
"""Vérifie l'oracle fictif figé avant toute inférence ; n'appelle aucun modèle."""
import hashlib
import json
import re
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / 'src-core/tests/fixtures/topics-classification-fr-v1.json'
EXPECTED_SHA256 = 'd02ec3ab4ad0ebd02e8b65cdcfb3a1f564bd39f71c5067ef18076605946063c1'


def require(condition, message):
    if not condition:
        raise ValueError(message)


def nonempty(value):
    return isinstance(value, str) and bool(value.strip())


def identifier(value):
    return isinstance(value, str) and re.fullmatch(r'[a-z][a-z0-9-]*', value) is not None


def validate(data):
    require(isinstance(data, dict), 'Oracle illisible')
    require(type(data.get('schema_version')) is int and data['schema_version'] == 1, 'Version inconnue')
    require(data.get('synthetic') is True, 'Corpus non déclaré fictif')
    topics = data.get('topics')
    cases = data.get('cases')
    require(isinstance(topics, list) and bool(topics), 'Catalogue vide')
    require(isinstance(cases, list) and bool(cases), 'Corpus vide')
    topic_ids = set()
    for topic in topics:
        require(isinstance(topic, dict), 'Sujet illisible')
        tid = topic.get('id')
        require(identifier(tid) and tid not in topic_ids, 'Identifiant de sujet invalide ou dupliqué')
        require(nonempty(topic.get('label')) and nonempty(topic.get('description')), 'Sujet sans libellé ou description')
        topic_ids.add(tid)
    ids = set()
    splits = Counter()
    kinds = Counter()
    critical = 0
    for entry in cases:
        require(isinstance(entry, dict), 'Cas illisible')
        eid = entry.get('id')
        require(identifier(eid) and eid not in ids, 'Identifiant de cas invalide ou dupliqué')
        ids.add(eid)
        require(entry.get('synthetic') is True, 'Cas non déclaré fictif')
        require(type(entry.get('critical')) is bool, 'Indicateur critique invalide')
        require(nonempty(entry.get('passage')) and nonempty(entry.get('rationale')), 'Passage ou justification absent')
        require(all(isinstance(entry.get(key), str) for key in ('context_before', 'context_after')), 'Contexte absent')
        split = entry.get('split')
        require(split in ('development', 'holdout'), 'Séparation du banc invalide')
        candidates = entry.get('candidate_topic_ids')
        require(isinstance(candidates, list) and 1 <= len(candidates) <= 25
                and all(identifier(x) for x in candidates), 'Candidats invalides')
        require(len(set(candidates)) == len(candidates) and set(candidates) <= topic_ids, 'Candidats dupliqués ou inconnus')
        expected = entry.get('expected')
        require(isinstance(expected, dict), 'Oracle du cas absent')
        kind = expected.get('kind')
        require(kind in ('single', 'multiple', 'abstain'), 'Type de réponse attendu inconnu')
        assigned = expected.get('topic_ids')
        plausible = expected.get('plausible_topic_ids')
        for group in (assigned, plausible):
            require(isinstance(group, list) and all(identifier(x) for x in group), 'Références attendues invalides')
            require(len(set(group)) == len(group), 'Référence attendue dupliquée')
            require(set(group) <= set(candidates), 'Référence attendue hors des candidats')
        if kind == 'single':
            require(len(assigned) == 1 and not plausible, 'Sujet unique incohérent')
        elif kind == 'multiple':
            require(len(assigned) > 1 and not plausible, 'Sujets multiples incohérents')
        else:
            require(not assigned, 'Abstention incompatible avec une attribution ferme')
        splits[split] += 1
        kinds[kind] += 1
        critical += int(entry['critical'])
    require(splits['development'] > 0 and splits['holdout'] > 0, 'Un des deux groupes est absent')
    return {'cases': len(cases), 'development': splits['development'], 'holdout': splits['holdout'],
            'single': kinds['single'], 'multiple': kinds['multiple'], 'abstain': kinds['abstain'], 'critical': critical}


def validate_file():
    raw = FIXTURE.read_bytes()
    require(hashlib.sha256(raw).hexdigest() == EXPECTED_SHA256,
            "L'oracle figé a changé : nouvelle version et erratum requis, pas de réécriture silencieuse")
    return validate(json.loads(raw))


if __name__ == '__main__':
    print(json.dumps({'oracle': 'v1', 'sha256': EXPECTED_SHA256, **validate_file()}, ensure_ascii=False))
