# Construire les sources

[English](BUILD.md) | [Français](BUILD.fr.md)

## Prérequis

- Rust stable compatible avec l'édition 2024 et les dépendances verrouillées.
  Cette publication et CI utilisent Rust 1.98.1 ; les versions plus anciennes
  ne sont pas certifiées ici.
- Node.js 22 et pnpm **10.34.5**, fixé dans `ui/package.json`.
- Python 3.11 ou ultérieur pour les contrôles de publication et XML.
- Bash pour les scripts. Sous Windows, utiliser un environnement Bash adapté à
  la chaîne d'outils choisie, pas la syntaxe PowerShell.

Inspecter les manifests et scripts avant exécution. Installer les dépendances
peut utiliser Internet ; l'inférence et les médias ne passent pas par un cloud.

## Reproduire les contrôles des sources

Depuis une copie complète du dépôt :

```sh
bash scripts/verify.sh
```

Le script installe les dépendances verrouillées de l'interface sans scripts
post-installation, produit les paquets audio synthétiques Rust, teste le socle
et le service de thèmes optionnel, contrôle format/Clippy et TypeScript, lance
les tests de l'interface et Python, compile l'interface et vérifie l'hygiène du
dépôt. Il **ne télécharge aucun modèle d'inférence et ne fabrique pas
d'installateur**. Le dossier temporaire est indiqué à la fin : ses paquets de
test sont générés, ce ne sont pas des médias utilisateur.

Avec les dépendances déjà en cache, refuser toute récupération réseau :

```sh
bash scripts/verify.sh --offline
```

Un cache manquant est un problème de préparation, pas une raison de changer les
fichiers de verrouillage. Voir [TESTING.fr.md](TESTING.fr.md) pour les limites des
tests et les essais de modèles ignorés.

## Travailler sur l'interface

```sh
pnpm --dir ui install --frozen-lockfile --ignore-scripts
pnpm --dir ui dev
```

L'interface de développement écoute sur le port 1420. Un navigateur aide à
modifier la présentation mais ne fournit pas les commandes natives Tauri et ne
prouve pas le fonctionnement de l'application de bureau. Aucune application
web de production n'est proposée.

## Préparer une application native

Tauri exige aussi la chaîne d'outils et les bibliothèques de ses
[prérequis officiels](https://v2.tauri.app/start/prerequisites/) : outils C++
Windows/WebView2, outils développeur en ligne de commande macOS, ou paquets de
développement GTK 3 et WebKit2GTK 4.1 sous Linux.

Les ressources natives sont exclues du dépôt. Lire et inspecter :

- `src-tauri/native/README.md` pour les ressources nécessaires ;
- `scripts/build-native.sh` pour les sources et options des moteurs ;
- `scripts/fetch-diarization-assets.sh` pour les ressources vocales et notices ;
- `MODEL_MANIFEST.json` pour les téléchargements de modèles fixés.

Cette préparation supplémentaire est explicite : elle télécharge des sources
et ressources tierces, exige une chaîne d'outils adaptée, de l'espace disque et
un `TMPDIR` inscriptible. Ne pas la lancer sans explication pour une simple
contribution documentaire. Vérifier les droits et bibliothèques du vrai paquet
cible.

Une fois les ressources et prérequis présents, utiliser la disposition Tauri du
dépôt et le CLI installé avec l'interface :

```sh
cd src-tauri
node ../ui/node_modules/@tauri-apps/cli/tauri.js dev
```

Pour empaqueter, remplacer `dev` par `build --ci --no-sign`. Le résultat est une
candidate locale, **pas un installateur officiel signé, notarisé ou validé**.
Tester installation et fonctionnement réel sur le système destinataire avant
diffusion. Respecter les protections des postes gérés et la politique de
sécurité de l'employeur.
