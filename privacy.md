---
title: Politique de confidentialité
layout: default
---

[← Retour à l'accueil](index.html)

# Politique de confidentialité — Chelou Track

*Dernière mise à jour : 26 août 2026*

## Résumé

Chelou Track est un logiciel de bureau gratuit et open source. Il ne collecte, ne
transmet et ne stocke **aucune donnée personnelle sur un serveur appartenant à
l'éditeur**. L'application fonctionne en local sur votre ordinateur et communique
directement avec le service de stockage cloud que vous choisissez de connecter
(pCloud, Dropbox ou Google Drive) avec votre propre compte.

## Qui édite ce logiciel

Chelou Track est un projet personnel, open source, disponible sur
[GitHub](https://github.com/pstkweb/chelou-track). Ce n'est pas un service
commercial : il n'existe pas de société ni de serveur d'exploitation associés.

## Quelles données sont traitées, et où

- **Vos fichiers de méthode** (vidéos, backing tracks, tablatures GuitarPro,
  documents PDF) restent hébergés sur votre compte de stockage cloud personnel.
  Chelou Track ne les copie jamais vers un serveur tiers : il les diffuse en
  streaming directement depuis votre compte vers votre poste, à la demande.
- **Le jeton d'authentification (OAuth)** du fournisseur cloud connecté est
  stocké uniquement en local sur votre machine, de façon chiffrée, via le
  trousseau de votre système d'exploitation (fichier chiffré DPAPI propre à
  votre session sur Windows, Keychain sur macOS, Secret Service sur Linux). Ce
  jeton n'est jamais visible depuis l'interface de l'application ni transmis à
  un service autre que le fournisseur cloud lui-même.
- **Le manifeste local** (structure de vos méthodes, progression, préférences
  de lecture) est un fichier stocké dans le dossier de données de l'application
  sur votre ordinateur. Il ne quitte jamais votre machine.
- Chelou Track ne connaît jamais votre email ni votre mot de passe : la
  connexion passe entièrement par le mécanisme OAuth du fournisseur cloud
  choisi, directement sur son propre site.

## Services tiers sollicités par l'application

- **Le fournisseur de stockage cloud que vous connectez** (pCloud, Dropbox ou
  Google Drive) : régi par sa propre politique de confidentialité. Chelou
  Track agit uniquement comme client de son API, avec les autorisations que
  vous lui accordez explicitement.
- **GitHub** : sollicité uniquement pour vérifier la disponibilité d'une
  nouvelle version de l'application (mise à jour automatique). Cette requête
  ne contient aucune donnée personnelle — seulement la version actuellement
  installée et votre plateforme, nécessaires pour proposer le bon fichier.

## Aucun traceur, aucune télémétrie

Chelou Track ne contient ni outil d'analyse d'audience, ni système de
télémétrie, ni cookie.

## Sécurité

L'interface de l'application (WebView) n'a jamais un accès direct à votre
jeton d'authentification ni à une URL brute du fournisseur cloud : tous les
échanges passent par la partie native (Rust) de l'application, qui joue le
rôle d'intermédiaire.

## Vos droits

Aucune donnée personnelle n'étant collectée par l'éditeur, il n'y a rien à lui
demander de rectifier ou de supprimer. Vos données restent sous votre contrôle
exclusif :

- pour révoquer l'accès de Chelou Track à votre compte cloud, gérez les
  applications autorisées depuis les paramètres de votre compte pCloud,
  Dropbox ou Google ;
- pour supprimer les données locales de Chelou Track, désinstallez
  l'application et supprimez son dossier de données applicatives.

## Mineurs

L'application ne collectant aucune donnée, cela vaut également pour les
utilisateurs mineurs.

## Modifications de cette politique

Toute modification de cette politique sera publiée sur cette page, avec une
date de mise à jour.

## Contact

Pour toute question, ouvrez une
[issue sur GitHub](https://github.com/pstkweb/chelou-track/issues).
