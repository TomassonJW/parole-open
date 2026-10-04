# Contribuer

[English](CONTRIBUTING.md) | [Français](CONTRIBUTING.fr.md)

Merci d'aider à rendre la transcription locale plus vérifiable et plus utile.
Commencer par un petit changement reproductible, pas une réécriture sans limite.

## Une contribution utile

1. Expliquer le problème et le résultat attendu dans un ticket ou une demande de
   fusion ciblée, en anglais ou en français, sans données personnelles.
2. Employer des exemples fictifs. Ne jamais joindre un enregistrement, une
   transcription réels, un credential ou un binaire/modèle de provenance inconnue.
3. Lire [BUILD.fr.md](BUILD.fr.md), [TESTING.fr.md](TESTING.fr.md) et
   l'[architecture](docs/architecture.fr.md). Pour un comportement, écrire une
   régression en échec avant la correction minimale.
4. Lancer `bash scripts/verify.sh`, inspecter le delta indexé puis exécuter
   `python3 scripts/check-staged-secrets.py` avant commit. Expliquer toute
   variation du nombre de tests ou des tests ignorés.
5. Actualiser les deux langues quand le sens ou les commandes changent. Les
   documents sont équivalents, pas deux promesses de fonctionnalités différentes.

Ne changer les fichiers de verrouillage ou dépendances que si nécessaire.
Inspecter provenance, licence, comportement et périmètre d'un nouveau composant
avant son exécution. Préserver audio/transcription originaux, annulation
explicite et refus des réponses périmées. Ne pas alléger un test pour verdir CI.

## Droits et revue

Les contributions suivent la licence du composant : MIT pour le code principal,
Apache 2.0 dans `src-gliclass/`. Ne proposer que du travail que tu peux licencier
ainsi ; garder les notices tierces et lire [LICENSING.fr.md](LICENSING.fr.md).
Aucun contrat de contribution, fusion ou délai de réponse garanti n'est imposé.

Rester respectueux, factuel et constructif. Les contributions dans les deux
langues sont bienvenues. Les responsables peuvent refuser un changement hors du
périmètre local. Pour la sécurité, suivre [SECURITY.fr.md](SECURITY.fr.md), pas un
ticket public décrivant une exploitation.
