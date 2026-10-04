# Tests et preuves

[English](TESTING.md) | [Français](TESTING.fr.md)

Lancer `bash scripts/verify.sh` depuis la racine, ou ajouter `--offline` si tous
les caches sont prêts. Voir [BUILD.fr.md](BUILD.fr.md) pour les prérequis. Le
script est l'entrée reproductible ; les commandes individuelles sont visibles
dans `scripts/verify.sh`.

## Ce que les contrôles des sources couvrent

- État du socle, annulation/reprise, audio conservé, validation des comptes rendus
  cités, présentation de la transcription et exports.
- Protocole du worker local de thèmes, identités, limites et résultats périmés.
- Comportements React, commandes du lecteur et enveloppes audio produites par Rust.
- Publication : noms interdits sans ouvrir les contenus, motifs de credentials
  sans afficher de valeurs, références privées, taille, liens symboliques et
  liens locaux de la documentation.
- Contrôles Python des fixtures synthétiques, preuves de signature et XML Word.

Le script crée `PAROLE_AUDIO_FIXTURE_OUTPUT` dans un nouveau dossier temporaire
**avant** les tests Rust et garde la variable pour l'interface. Les tests de
transport, sinon ignorés, sont ainsi exercés. L'audio est synthétique. Ne pas
remplacer ces enveloppes par du JSON inventé ou une vraie réunion.

## Ce qu'un résultat vert ne prouve pas

Les tests de modèles marqués ignorés demandent des ressources locales explicites
et ne sont pas lancés par cette suite de sources. Les tests unitaires ne prouvent
ni la qualité des vrais modèles, ni les médias longs, ni un budget mémoire de
8 Go, ni le fonctionnement installé sous Windows/macOS. Compiler l'interface
n'est pas accepter l'application native. Traduction, voix et compte rendu cité
demandent toujours une relecture humaine. Une citation ne rend pas vraie une
conclusion déduite.

Le contrôleur de publication est une défense supplémentaire fondée sur des
motifs, pas une preuve absolue d'absence de tout secret. Vérifier aussi la
provenance, les nouveaux binaires et les licences. Ne jamais soumettre un vrai
credential pour tester un détecteur.

## Discipline de modification

Pour un comportement : écrire la régression en échec, observer l'échec voulu,
corriger au minimum, rejouer les tests ciblés et pertinents puis committer un
petit changement revu. Préserver transcription et audio originaux ; ne pas
alléger un test pour obtenir du vert. Expliquer toute variation de tests
collectés/ignorés. Ne committer que des données fictives publiables.

Les fixtures lues par Rust via `include_str!` gardent leurs chemins, notamment
`ui/tests/fixtures/` et les quatre états autorisés dans
`docs/evaluations/orion-2026-09-28/`. Ce dossier est une entrée de régression
limitée, pas une archive de réunions réelles ou de preuves de remise privées.
