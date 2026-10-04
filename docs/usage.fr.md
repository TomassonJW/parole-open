# Utiliser Parole

[English](usage.md) · [Français](usage.fr.md) · [Accueil](../README.fr.md)

Ce guide distingue **la préparation des sources** de **l'utilisation d'une application de bureau construite localement**. La copie du code ne contient ni installateur public officiel, ni binaires natifs et modèles indispensables au traitement des médias. L'interface décrite ci-dessous est en français.

## Avant d'ouvrir un média

Pour contribuer, commencer par [BUILD.fr.md](../BUILD.fr.md) : prérequis de la plateforme, moteurs natifs, ressources de séparation des voix, notices de licence et construction de l'application Tauri. Suivre ensuite [TESTING.fr.md](../TESTING.fr.md) pour les contrôles applicables à votre plateforme. Un aperçu du frontend Vite ne remplace pas l'application de bureau : il ne peut pas traiter les médias avec les moteurs natifs. La construction et la préparation des modèles peuvent accéder au réseau ; le traitement lui-même n'utilise que l'inférence locale.

Pour utiliser l'application, vérifier que le paquet natif est complet, que l'appareil a assez d'espace pour les modèles choisis et les données des travaux, et que vous avez le droit de traiter l'enregistrement. Le panneau des modèles de l'écran d'import contrôle leur disponibilité locale. Demander explicitement leur préparation si nécessaire ; l'application vérifie les téléchargements épinglés. La traduction et le compte rendu avec le modèle de base exigent le modèle de langue en plus des ressources de parole. Ne pas contourner un avertissement de ressource manquante par des fichiers non vérifiés. Ce dépôt ne propose ni installateur public ni garantie universelle de performances matérielles.

## Un enregistrement, étape par étape

1. **Importer.** Sélectionner un fichier audio/vidéo local compatible dans l'application ou l'y déposer. Le choix du fichier ne démarre pas le travail. Conserver la source accessible pour l'écoute ultérieure ; la conservation locale du média par l'application n'est pas une sauvegarde distante.
2. **Choisir le traitement.** Confirmer la langue parlée ou laisser sa détection automatique, choisir éventuellement une langue de traduction et décider de générer ou non un compte rendu. Dans ce dernier cas, choisir une langue de rapport compatible et un modèle installé et disponible. Le formulaire signale les combinaisons interdites et les ressources manquantes avant le lancement.
3. **Lancer et reprendre.** Démarrer la transcription. Le travail passe par la reconnaissance en segments puis, si demandé, la traduction et le rapport ; certains passages peuvent rester sans locuteur attribué. Suivre la progression dans la vue de traitement. Un travail interrompu peut être rouvert et repris ; un résultat incomplet est indiqué comme tel, non présenté silencieusement comme définitif. Conserver les données locales du travail et le média source si vous prévoyez de reprendre ou d'écouter plus tard.
4. **Examiner le résultat.** Lire les passages originaux horodatés puis, si demandés, la traduction et le compte rendu. Écouter les paroles douteuses et vérifier les voix lorsque la source est accessible. Ne nommer les locuteurs qu'après vérification. Une citation du rapport permet de retrouver un indice, pas de certifier que la synthèse qui l'entoure est juste.
5. **Régler la lecture.** Dans **Présentation**, choisir le regroupement fluide ou détaillé, le seuil de pause, les horaires et les couleurs des locuteurs. Ces réglages modifient l'affichage et certains exports, non la transcription ou l'enregistrement d'origine. Les préférences du travail sont distinctes des valeurs par défaut des futurs travaux. Si un aperçu ou une sauvegarde est périmé, recharger plutôt que de l'appliquer à une autre révision de la source.
6. **Explorer sur demande seulement.** Dans **Pistes de sujets**, préparer explicitement les pistes lexicales pour ce travail, examiner les passages originaux et revenir à la transcription ou à l'audio. L'interface ne fournit pas de classement automatique par modèle ; une correspondance de mots ne doit pas devenir une affectation vérifiée.
7. **Exporter en connaissance de cause.** Choisir TXT, Markdown, DOCX, JSON, SRT ou VTT. Les documents texte, Markdown et Word peuvent suivre les réglages de présentation ; JSON contient les données du travail et les sous-titres gardent les horaires des segments originaux. Un travail incomplet peut donner lieu à un document texte avec avertissement, mais JSON et sous-titres attendent sa reprise. Examiner le fichier enregistré et son contenu avant de le diffuser.

## Relecture et confidentialité

Le traitement local ne garantit ni l'exactitude de la transcription ni le droit d'utiliser un enregistrement. Vérifier dans l'audio et le contexte les noms sensibles, les voix, le sens traduit, les citations, les personnes responsables des actions et les conclusions du rapport. Protéger les exports et les données locales des travaux selon leur sensibilité. Installer un modèle ou construire depuis des sources épinglées constitue une activité réseau explicite, pas une inférence réseau sur l'enregistrement.

Pour les frontières d'implémentation, voir [architecture](architecture.fr.md). Pour les obligations de redistribution, lire [LICENSING.fr.md](../LICENSING.fr.md), [THIRD_PARTY_LICENSES.fr.md](../THIRD_PARTY_LICENSES.fr.md) et [RELEASE.fr.md](../RELEASE.fr.md).
