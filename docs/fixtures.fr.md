# Données de régression publiées

[English](fixtures.md) | [Français](fixtures.fr.md)

Les données sont fictives ou synthétiques. Elles révèlent des régressions
concrètes ; elles ne démontrent pas la qualité d'un modèle sur une vraie réunion
et ne représentent pas des personnes réelles.

- `src-core/tests/fixtures/` : scénarios synthétiques Orion/Atlas, candidats de
  thèmes et oracle/erratum indépendant. Les déclarations et contrats des
  fixtures originales documentent leur caractère synthétique.
- `ui/tests/fixtures/` : états produits par des exemples Rust, lus par
  l'interface et les tests de contrat entre langages.
- `docs/evaluations/orion-2026-09-28/` : quatre états de résultats autorisés,
  lus par `src-core/tests/verified_report.rs`. Ils proviennent du scénario Orion
  fictif et ne publient pas de réunion privée.
- Les enveloppes audio sont produites par Rust à la vérification dans un dossier
  temporaire. Ce ne sont pas des enregistrements utilisateur, ni des fichiers Git.

Conserver la distinction scénario/oracle et les chemins exacts. Un oracle de
régression n'est pas une confirmation humaine d'un modèle. Ne pas inventer une
fixture pour masquer un transport cassé ni supprimer un test parce que l'export
des sources est incomplet. Toute nouvelle fixture doit être synthétique,
minimale, documentée et redistribuable sous la bonne licence. Aucun média ou
transcription réels dans les tickets.
