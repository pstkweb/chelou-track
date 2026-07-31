# Design — Auto-updater

Date : 2026-07-31

## Contexte

L'appli est distribuée via des releases GitHub buildées par `.github/workflows/build.yml`
(`tauri-apps/tauri-action@v1`, déclenché sur `release: published`). Le plugin
`tauri-plugin-updater` (Rust, déjà dans `Cargo.toml` + enregistré dans `lib.rs`) et
`@tauri-apps/plugin-updater` (TS, déjà dans `package.json`) ont été ajoutés via
`npm run tauri add updater`. Les clés de signature (publique/privée) ont été générées
localement mais ne sont branchées nulle part : `tauri.conf.json` n'a pas de bloc
`plugins.updater`, la CI ne connaît pas les clés, et aucun code TS/Rust n'appelle le
plugin pour vérifier ou appliquer une mise à jour.

But de ce travail : appli qui se met à jour toute seule, sans action utilisateur au-delà
d'un redémarrage automatique visible via une notification, cohérent avec l'objectif
transverse « le moins d'actions manuelles possible » (ARCHITECTURE.md §1).

## Décisions

- **Détection** : check unique au démarrage de l'appli (pas de polling périodique en
  cours de session — l'utilisateur peut être en train de jouer, on ne veut pas
  interrompre une session pour un check).
- **Comportement si update trouvée** : entièrement automatique — download, install,
  puis `relaunch()` sans confirmation utilisateur. Un toast s'affiche juste avant le
  redémarrage (« Mise à jour vX.Y.Z installée, redémarrage... ») comme seul signal
  visible ; pas de dialogue de confirmation, pas de barre de progression.
- **Comportement si pas d'update / erreur réseau** : totalement silencieux. Le check
  ne doit jamais bloquer ni ralentir perceptiblement le démarrage normal, et une
  erreur réseau (offline, GitHub down) ne doit produire aucun toast d'erreur.
- **Endpoint** : GitHub Releases, `latest.json` auto-généré par `tauri-action` et
  attaché à chaque release publiée (zéro infra à gérer, cohérent avec le workflow
  existant).

## Changements

### 1. CI — `.github/workflows/build.yml`

Ajouter à l'étape `tauri-apps/tauri-action@v1` deux variables d'environnement :

```yaml
env:
  GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
  TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
```

Dès que `tauri.conf.json` contient `plugins.updater`, `tauri-action` génère et attache
automatiquement `latest.json` + les fichiers `.sig` à la release — aucune option
supplémentaire (`includeUpdaterJson` est `true` par défaut dès que la config updater
est présente).

Prérequis manuel (hors CI, à faire une fois par l'utilisateur) : créer les deux
secrets `TAURI_SIGNING_PRIVATE_KEY` et `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` dans
Settings → Secrets and variables → Actions du repo `pstkweb/chelou-track`, à partir
de la clé privée générée localement (`tauri signer generate`).

### 2. `src-tauri/tauri.conf.json`

Ajouter un bloc `plugins.updater` :

```json
"plugins": {
  "updater": {
    "pubkey": "<clé publique générée localement>",
    "endpoints": [
      "https://github.com/pstkweb/chelou-track/releases/latest/download/latest.json"
    ]
  }
}
```

La clé publique doit être copiée depuis le fichier `.key.pub` généré localement par
l'utilisateur (pas de secret — c'est la clé publique, sûre à committer).

### 3. Rust

- Ajouter la dépendance `tauri-plugin-process` (crate) dans `src-tauri/Cargo.toml`,
  même groupe cible que `tauri-plugin-updater`
  (`[target.'cfg(not(any(target_os = "android", target_os = "ios")))'.dependencies]`).
- L'enregistrer dans `src-tauri/src/lib.rs` : `.plugin(tauri_plugin_process::init())`.
- `tauri-plugin-updater` est déjà enregistré (`.plugin(tauri_plugin_updater::Builder::new().build())`)
  — aucun changement nécessaire là.
- Aucune commande custom (`#[tauri::command]`) à ajouter : le check/download/install/relaunch
  passent entièrement par les plugins JS officiels, pas par `commands/`.

### 4. Capabilities — `src-tauri/capabilities/desktop.json`

Ajouter la permission par défaut du plugin process à côté de `updater:default` :

```json
"permissions": [
  "updater:default",
  "process:default"
]
```

### 5. TypeScript

- Ajouter la dépendance npm `@tauri-apps/plugin-process` (déjà présent :
  `@tauri-apps/plugin-updater`).
- Nouveau hook `src/hooks/useAppUpdater.ts` :
  - au montage (une seule fois), appelle `check()` du plugin updater
  - si `update` non-null : `showToast(...)` puis `await update.downloadAndInstall()`
    puis `await relaunch()` (du plugin process)
  - `try/catch` autour de tout : en cas d'erreur (réseau, parsing), `console.error`
    et sortie silencieuse — jamais de toast d'erreur, jamais de blocage du démarrage
  - dépend de `useToast()` (`contexts/ToastContext.tsx`), donc doit être appelé
    depuis un composant monté **sous** `ToastProvider`
- `src/components/pages/App.tsx` : remonter `ToastProvider` pour qu'il englobe tout
  le composant (actuellement il ne wrap que les états `ready`/`no-auth`/`no-methods`,
  pas `loading`), afin que le toast updater puisse s'afficher dès le lancement, avant
  résolution de l'auth pCloud. Appeler `useAppUpdater()` dans le nouveau scope englobé
  par `ToastProvider` (ex: un petit composant interne monté juste sous le provider,
  au même niveau que le reste du contenu de `App`).

## Hors scope

- Pas de polling périodique en cours de session.
- Pas de dialogue de confirmation ni de barre de progression de download.
- Pas de mécanisme de rollback si l'update installée est cassée (hors scope, géré par
  une future release corrective classique).
- Pas de tests automatisés dédiés : le hook wrap des appels à des plugins Tauri
  externes (réseau, filesystem, process) non testables unitairement sans mock lourd ;
  cohérent avec la règle du projet (`CLAUDE.md` — logique testable unitairement vit
  dans un sous-crate sans dep Tauri, ce qui ne s'applique pas ici côté TS/plugin).
