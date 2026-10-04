# Architecture

[English](architecture.md) · [Français](architecture.fr.md) · [Accueil](../README.fr.md)

Parole est une application de bureau Tauri avec une interface React et un coeur de traitement Rust. Sa frontière de confiance est l'appareil local : l'application prépare ou vérifie des ressources épinglées, exécute les moteurs natifs sur place et conserve les travaux dans le répertoire local de l'application. Le traitement n'intègre ni inférence distante ni repli vers le cloud. La construction initiale et les téléchargements de modèles demandés explicitement peuvent utiliser le réseau.

## Repères dans les sources

| Emplacement | Rôle |
| --- | --- |
| `ui/` | Parcours React/TypeScript pour l'import, les travaux, le lecteur, les pistes de sujets, les réglages de présentation et les exports. `ui/src/lib/backend.ts` délimite l'interface avec le backend. |
| `src-tauri/src/main.rs` | Initialisation Tauri et commandes enregistrées pour les modèles, travaux, lecture, pistes lexicales, présentation et exports. |
| `src-tauri/src/jobs.rs`, `audio.rs`, `models.rs`, `exports.rs` | Orchestration des travaux, accès à l'audio conservé, préparation et contrôle des modèles, export des fichiers. |
| `src-tauri/src/presentation.rs`, `presentation_store.rs` | Aperçu et persistance des réglages de lecture par travail et par défaut. |
| `src-tauri/src/topics.rs` | Lecture ou préparation explicite de pistes lexicales pour un travail. |
| `src-core/src/` | Types des travaux et segments, traitement par tranches reprenable, séparation des voix, traduction et rapport, rendu du rapport sourcé, projection de la transcription et logique des pistes/cache. |
| `src-gliclass/` | Service de classification GLiClass local, séparé et optionnel (Apache-2.0) ; ce n'est pas un parcours de classement en production dans l'interface. |
| `MODEL_MANIFEST.json`, `scripts/`, `src-tauri/native/` | Inventaire des ressources épinglées, auxiliaires de construction/téléchargement et structure attendue du paquet natif. Binaires et modèles ne figurent pas dans Git. |

## Traitement et persistance

L'utilisateur choisit un fichier local et confirme la langue parlée, la traduction facultative et le compte rendu facultatif avant le lancement. Les décodeurs natifs, la reconnaissance vocale et la séparation des voix travaillent sur place ; la parole est traitée par tranches. Le type `Job` conserve les segments, identifiants de locuteurs, étapes complémentaires demandées, progression et état. Une tranche est persistée avant d'être comptée comme terminée. Au redémarrage, un travail encore en cours devient interrompu ; la reprise vise les étapes inachevées. Un échec de traduction ou un rapport manquant n'est pas présenté silencieusement comme un résultat complet. L'application stocke l'état localement et distingue les résultats incomplets à la lecture et à l'export.

Les modèles ne sont pas des dépendances obtenues en téléversant le média. `src-tauri/src/models.rs` contrôle la taille et l'empreinte des fichiers locaux épinglés, puis une préparation explicite installe les modèles de parole et de langue requis. Les ressources de séparation des voix appartiennent au paquet natif assemblé pour la compilation. `MODEL_MANIFEST.json` décrit les identités et modes d'obtention. Voir [BUILD.fr.md](../BUILD.fr.md) pour préparer aussi les moteurs natifs et leurs licences, pas uniquement le frontend JavaScript.

## La source n'est pas sa présentation

Les `Job.segments` horodatés et le média conservé constituent la source. `src-core/src/transcript_presentation.rs` projette des blocs de lecture fluides ou détaillés à partir des segments ; les options d'affichage et d'export ne réécrivent ni transcription ni audio. `presentation_store.rs` conserve les préférences hors du travail, avec réglages propres à chaque travail et valeurs par défaut pour les suivants. Une sauvegarde exige la révision attendue ; les conflits et valeurs invalides sont refusés. Un aperçu porte l'identifiant du travail et une révision calculée à partir des segments. `ui/src/hooks/usePresentation.ts` vérifie cette identité et écarte les réponses tardives après changement de travail ou d'options. L'export n'est pas une modification de la source : TXT, Markdown et DOCX peuvent suivre la présentation choisie, tandis que JSON conserve les données du travail et SRT/VTT les horaires d'origine.

La lecture du média repose sur l'enregistrement local conservé, non sur les blocs affichés. Si le média source n'est plus accessible, l'écoute peut être indisponible alors que la transcription stockée reste lisible. Les étiquettes de locuteurs doivent être vérifiées ; une attribution absente reste non attribuée au lieu d'être inventée.

## Sources et limites

La chaîne de rapport (`src-core/src/language.rs`, `report_lifecycle.rs`, `verified_report.rs`) produit un texte à partir de la transcription ou de sa traduction et présente des citations horodatées issues des segments sources. Elle persiste les étapes intermédiaires et lie les résultats conservés à l'identité de la source et des options, pour ne pas réutiliser sans contrôle un cache périmé. Cette conception ne démontre **pas** la justesse de chaque interprétation générée, attribution de tâche, traduction ou contexte de citation. Relire les paroles et écouter le média avant diffusion.

La préparation des sujets (`src-core/src/topic_access.rs`, `src-tauri/src/topics.rs`) est une opération lexicale locale explicite ; l'interface permet d'examiner les pistes et de revenir aux passages originaux. Elle vérifie l'identité travail/source avant d'afficher un instantané, pour ne pas montrer les pistes d'une autre transcription comme actuelles. Le code optionnel `src-gliclass` est distinct et n'est pas raccordé comme classement automatique en production dans ce lecteur. Un rapprochement lexical est une piste, non une affectation thématique ni une décision.

## Frontière de distribution

Une copie des sources n'est pas une application installée. Moteurs natifs, modèles, notices et validation sur le système cible doivent être traités avant distribution. Les textes de l'interface et de l'installateur sont en français ; cette documentation bilingue ne les traduit pas. Voir [utilisation](usage.fr.md), [licences](../LICENSING.fr.md) et [critères de publication](../RELEASE.fr.md).
