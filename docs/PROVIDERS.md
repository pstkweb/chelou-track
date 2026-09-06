# Providers cloud — état et roadmap

Suivi de l'état d'implémentation des providers de stockage cloud pour Chelou Track, et des
raisons d'exclusion pour ceux écartés. Les détails techniques d'implémentation des providers
actifs restent dans `ARCHITECTURE.md` §14 — ce document répond à « où en est-on, et pourquoi »,
pas « comment c'est fait ».

## Implémentés, actifs

| Provider | Notes |
|---|---|
| **pCloud** | Provider historique. Endpoint EU (`eapi.pcloud.com`) — ne jamais basculer sur l'endpoint US. |
| **Dropbox** | Actif, aucune contrainte particulière. |

## Implémenté, masqué du sélecteur

| Provider | Raison |
|---|---|
| **Google Drive** | Fonctionnel avec le scope `drive.readonly` (`crates/providers/src/gdrive.rs`), mais Google exige pour ce scope un audit de sécurité **ADA-CASA AL1** annuel et payant (~540 $/an minimum via le partenaire le moins cher, plusieurs semaines de délai) pour faire valider l'app — jugé disproportionné pour ce projet. Le scope alternatif non-restreint que Google recommande (`drive.file` + Google Picker) a été implémenté et testé réellement : il ne fonctionne pas pour notre cas d'usage — il ne donne accès qu'aux fichiers individuellement ouverts via le Picker, pas à ceux découverts par un scan récursif d'un dossier (`403 appNotAuthorizedToFile` en pratique), incompatible avec l'import automatique (§1/§6 ARCHITECTURE.md). Le code reste en place (`ProviderId::GoogleDrive`, `gdrive.rs`) et fonctionne pour qui accepterait de passer l'audit — seul le sélecteur dans `ConnectScreen.tsx` l'exclut de l'UI. |

## Candidats (par ordre de priorité)

| Provider | Notes |
|---|---|
| **OneDrive** (Microsoft Graph) | Priorité 1. Gros recouvrement d'utilisateurs (bundlé Windows/Office). La vérification éditeur Microsoft porte sur l'identité de l'organisation ("Publisher Verification" via Microsoft Partner/Cloud Partner Program), pas sur un audit de sécurité payant façon CASA — friction a priori bien plus faible que Google, **à confirmer concrètement** (créer une app registration test et vérifier le parcours de consentement) avant d'investir dans l'implémentation complète. |
| **Box** | API propre, pas d'équivalent CASA identifié. Public plus B2B/entreprise — recouvrement plus faible avec l'audience visée (élèves de guitare). |
| **Nextcloud / WebDAV** (auto-hébergé) | Pas un "gros" provider grand public, mais pertinent pour une partie du public (profil homelab/auto-hébergement). Comme c'est le serveur de l'utilisateur lui-même, **aucune review d'app tierce à passer** — zéro friction de vérification, contrairement à tous les providers ci-dessus. |

## Exclus

| Provider | Raison |
|---|---|
| **iCloud Drive** | Apple n'expose pas d'API publique permettant de parcourir le Drive général d'un utilisateur — CloudKit est scopé par app (conteneur dédié), pas adapté à ce cas d'usage. |
| **Amazon Drive** | API développeur fermée par Amazon (~2018), plus d'option d'intégration tierce. |

---

*Dernière mise à jour : 2026-09-06.*
