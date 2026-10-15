# Chelou Track

Lecteur desktop de méthodes de guitare. Les vidéos, backing tracks et tablatures GuitarPro
restent sur ton cloud (pCloud, Dropbox) : Chelou Track les lit en streaming, sans rien
télécharger, et affiche la tablature synchronisée avec le backing track pendant que tu joues.

- **Import automatique** : pointe un dossier de ton cloud, l'app détecte les méthodes, leçons,
  vidéos, backing tracks et tablatures.
- **Vue synchro** : backing track audio + tablature GuitarPro qui défile en rythme, vitesse de
  lecture réglable.
- **Documents** : PDF et images de la méthode consultables dans l'app.
- **Mises à jour automatiques** une fois l'app installée.

---

## Utilisateurs — installer Chelou Track

### 1. Télécharger la bonne version

Va sur la page [**Releases**](https://github.com/pstkweb/chelou-track/releases/latest) et
prends le fichier qui correspond à ton système (dans la section *Assets*) :

| Système | Fichier à prendre | Remarque |
|---|---|---|
| **Windows** 10/11 | `chelou-track_X.Y.Z_windows_x64_release-setup.exe` | Recommandé. Le `.msi` fait la même chose, utile pour un déploiement géré. |
| **macOS** (Apple Silicon M1/M2/M3…) | `chelou-track_X.Y.Z_darwin_aarch64_release.dmg` | Pas de build pour les Mac Intel. |
| **Linux** Debian / Ubuntu / Mint | `chelou-track_X.Y.Z_linux_amd64_release.deb` | |
| **Linux** Fedora / openSUSE | `chelou-track_X.Y.Z_linux_x86_64_release.rpm` | |
| **Linux** autre distribution | `chelou-track_X.Y.Z_linux_amd64_release.AppImage` | Portable, aucune installation. |

Ignore les fichiers `.sig`, `.app.tar.gz` et `latest.json` : ils servent au système de mise à
jour automatique.

### 2. Installer

**Windows** — lance le `-setup.exe`. L'app n'est pas signée par un certificat éditeur, donc
Windows SmartScreen peut afficher « Windows a protégé votre ordinateur » : clique sur
*Informations complémentaires* puis *Exécuter quand même*.

**macOS** — ouvre le `.dmg` et glisse Chelou Track dans *Applications*. L'app n'est pas
notarisée par Apple ; au premier lancement macOS peut refuser de l'ouvrir (« endommagée » ou
« développeur non identifié »). Dans ce cas, lance une fois dans le Terminal :

```bash
xattr -cr /Applications/chelou-track.app
```

puis ouvre l'app normalement.

**Linux**

```bash
# Debian / Ubuntu
sudo apt install ./chelou-track_X.Y.Z_linux_amd64_release.deb

# Fedora
sudo dnf install ./chelou-track_X.Y.Z_linux_x86_64_release.rpm

# AppImage
chmod +x chelou-track_X.Y.Z_linux_amd64_release.AppImage
./chelou-track_X.Y.Z_linux_amd64_release.AppImage
```

La lecture vidéo et audio passe par GStreamer. Si une vidéo affiche « Impossible de lire cette
vidéo », installe les plugins de base de ta distribution (par ex. `gstreamer1.0-plugins-base`
et `gstreamer1.0-plugins-good` sur Debian/Ubuntu).

### 3. Premier lancement

1. Choisis ton service cloud (pCloud ou Dropbox) : le navigateur s'ouvre pour autoriser
   l'accès, puis tu reviens dans l'app.
2. Sélectionne le dossier qui contient tes méthodes ; l'app les scanne et les ajoute à ta
   bibliothèque.

Les mises à jour suivantes sont proposées directement dans l'app, pas besoin de revenir sur
cette page.

---

## Développeurs

La source de vérité de l'architecture est [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), à lire
avant tout changement structurel. [CLAUDE.md](CLAUDE.md) en donne un résumé condensé, et
[docs/PROVIDERS.md](docs/PROVIDERS.md) l'état des providers cloud.

### Stack

Tauri v2 (backend Rust) · React 19 + TypeScript + Vite · Tailwind CSS v4 ·
AlphaTab (tablatures) · PDF.js (documents).

### Prérequis

- **Node.js 22** et npm
- **Rust stable** (via [rustup](https://rustup.rs/))
- Les **prérequis système de Tauri v2** pour ton OS :
  [tauri.app/start/prerequisites](https://v2.tauri.app/start/prerequisites/)
  - Windows : Microsoft C++ Build Tools + WebView2 (déjà présent sur Windows 10/11 récents).
  - macOS : Xcode Command Line Tools (`xcode-select --install`).
  - Linux (Debian/Ubuntu) :

    ```bash
    sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev \
      build-essential curl wget file libssl-dev patchelf
    ```

### Installation

```bash
git clone https://github.com/pstkweb/chelou-track.git
cd chelou-track
npm ci
```

### Identifiants OAuth des providers

Les client ID/secret des providers cloud sont injectés **à la compilation** dans le binaire Rust.
En local, crée `src-tauri/.cargo/config.toml` (gitignoré, Cargo le lit automatiquement) :

```toml
[env]
PCLOUD_CLIENT_ID = "..."
PCLOUD_CLIENT_SECRET = "..."
DROPBOX_CLIENT_ID = "..."
DROPBOX_CLIENT_SECRET = "..."
```

Pour obtenir les tiens, crée une app sur la console développeur du provider
([pCloud](https://docs.pcloud.com/), [Dropbox](https://www.dropbox.com/developers/apps)) et
déclare ces redirect URIs (l'app essaie les ports dans l'ordre) :

```
http://localhost:53682/callback
http://localhost:53683/callback
http://localhost:53684/callback
```

Sans ce fichier, le projet compile quand même ; seule la connexion au provider échoue au
runtime. En CI de release, les valeurs viennent des secrets GitHub Actions.

### Lancer en dev (hot reload)

```bash
npm run dev
```

Démarre Vite sur `http://localhost:1420` et lance l'app Tauri par-dessus :

- les modifications du **frontend** (`src/`) sont rechargées à chaud (HMR) instantanément ;
- les modifications du **backend Rust** (`src-tauri/`) déclenchent une recompilation et un
  redémarrage automatique de l'app.

Le premier lancement compile toutes les dépendances Rust et prend plusieurs minutes ; les
suivants sont incrémentaux.

### Commandes utiles

| Commande | Rôle |
|---|---|
| `npm run dev` | App en mode dev avec hot reload |
| `npm run build` | Build de production + installeurs dans `src-tauri/target/release/bundle/` |
| `npm run ci` | Vérifs TypeScript lancées en CI : Biome + `tsc` + Vitest |
| `npm run lint:fix` | Corrige automatiquement lint et formatage (Biome) |
| `cd src-tauri && cargo check` | Vérification Rust rapide (sans link) |
| `cd src-tauri && cargo clippy` | Lints Rust |
| `cd src-tauri && cargo test -p chelou-manifest -p chelou-providers` | Tests unitaires Rust |

Les tests unitaires Rust ne couvrent que les sous-crates sans dépendance Tauri ; toute logique
testable doit y vivre (cf. §15 de [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)).

### Publier une release

Lancer à la main le workflow [release.yml](.github/workflows/release.yml) (onglet *Actions* →
*Release* → *Run workflow*) en choisissant le type de bump (`patch`, `minor`, `major`) et,
optionnellement, le commit à releaser (par défaut la tête de `main`). Il :

1. calcule la version suivante depuis le plus grand tag `vX.Y.Z` ;
2. l'écrit partout via [scripts/bump-version.mjs](scripts/bump-version.mjs) (`package.json`,
   `package-lock.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`,
   `src-tauri/Cargo.lock`), committe, tague et pousse (le commit n'est poussé sur `main` que si
   le commit visé en est la tête) ;
3. crée la release en brouillon, lance [build.yml](.github/workflows/build.yml) qui compile
   Windows, macOS et Linux, signe les artefacts de mise à jour et les attache avec le
   `latest.json` lu par l'updater intégré ;
4. publie la release une fois tous les builds réussis.

Si un build échoue, la release reste en brouillon : relancer les jobs échoués du run. Un fichier
qui porte la version s'ajoute dans `scripts/bump-version.mjs`, nulle part ailleurs. Publier une
release à la main depuis l'UI GitHub déclenche toujours `build.yml` directement.
