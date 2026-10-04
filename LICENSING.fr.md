# Licences

[English](LICENSING.md) | [Français](LICENSING.fr.md)

## Code et documentation d'origine

Le code original, la documentation et les illustrations de Parole sont sous
[licence MIT](LICENSE), **sauf `src-gliclass/`**, sous
[Apache 2.0](src-gliclass/LICENSE). Les attributions existantes sont conservées.
`src-core` et `src-tauri` déclarent MIT ; le paquet de l'interface aussi.
Les contributions suivent la licence du composant qu'elles modifient.

Ces licences autorisent l'utilisation, la modification et la redistribution,
y compris commerciales. MIT n'oblige pas un dérivé à publier ses modifications.
Les textes de licence anglais font foi ; cette page explique leur périmètre sans
les remplacer. Aucune garantie n'est fournie.

## Dépendances, modèles et données

Les dépendances externes gardent leurs licences. Celle de Parole ne remplace pas
les droits de FFmpeg, ONNX Runtime, des moteurs ou des poids de modèles. Voir
[THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md),
[MODEL_MANIFEST.json](MODEL_MANIFEST.json) et les notices amont.

Ce dépôt contient des jeux de régression fictifs ou synthétiques autorisés, pas
d'enregistrements utilisateur ni de transcriptions de vraies réunions. Les
paquets audio produits pour les tests restent temporaires. Les fixtures
synthétiques originales suivent MIT ; tout futur contenu tiers doit conserver
ses propres notices.

## Publier les sources ne valide pas les binaires

Aucun poids de modèle, moteur natif binaire ou installateur officiel n'est livré
ici. Une distribution exécutable devra contrôler séparément les composants
exacts, notices, obligations de fournir les sources et conditions de liaison.
En particulier, FFmpeg/LGPL et les dépendances sous MPL ne deviennent pas MIT ou
Apache. Une compilation réussie ne clôt pas cet audit.
