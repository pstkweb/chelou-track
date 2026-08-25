# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Lecteur desktop de méthodes de guitare : vidéos + exercices stockés sur pCloud, avec une vue
synchronisant un backing track audio et une tablature GuitarPro. Streaming sans téléchargement,
import le plus automatique possible.

**La source de vérité de l'architecture est `docs/ARCHITECTURE.md`. Le lire avant tout
changement structurel. Les décisions qui y figurent ont été tranchées volontairement — ne pas
les re-dériver ni les contredire sans raison explicite.**

## Commandes

```bash
# Développement (démarre Vite + le backend Rust en watch)
npm run dev

# Build production
npm run build

# CI TypeScript : biome + tsc (lancé par GitHub Actions)
npm run ci

# Vérification rapide Rust (pas de link — beaucoup plus rapide que build)
cd src-tauri && cargo check

# Lints Rust
cd src-tauri && cargo clippy

# Tests unitaires Rust (sous-crates sans dépendances système Tauri — cf. §15 ARCHITECTURE.md)
cd src-tauri && cargo test -p chelou-manifest -p chelou-providers

# Vérification TypeScript sans emit
npx tsc --noEmit
```

## Stack

- **Tauri v2** (Rust backend + WebView2 sur Windows), frontend TypeScript + Vite.
- **AlphaTab** (`@coderline/alphatab@1.8.3`) pour le rendu et la synchro des tablatures GuitarPro.
- **PDF.js** (`pdfjs-dist`) pour la section documents.
- **Balise `<audio>`** pointée sur `stream://audio/{id}` pour la lecture des backing tracks (streaming progressif, `playbackRate` natif). **Web Audio API** (`AudioContext` / `decodeAudioData`) réservée à un usage ponctuel : l'analyse du silence de tête d'un track, pas la lecture.
- **reqwest** côté Rust pour les appels pCloud + streaming Range.
- **keyring-core** pour le token du provider connecté (store par OS — `keyring-dpapi-store` maison sur Windows, Keychain/Secret Service natifs ailleurs).

## Architecture

### Frontière Rust / TypeScript

```
TS (WebView)          Rust (Tauri backend)                 Provider cloud (pCloud aujourd'hui)
──────────────────    ─────────────────────────────        ────────────────────────────────────
invoke("cmd")    ──►  commands/mod.rs                 ──►  eapi.pcloud.com
fetch("stream://") ►  stream/mod.rs (protocol)         ──►  bytes (Range)
                      auth/mod.rs (keyring-core, store par OS)
                      providers/mod.rs (registre ProviderClient/ProviderAuthClient)
                      crates/providers (trait StorageProvider + adapters par provider)
                      manifest/mod.rs (JSON local)
```

Le WebView ne touche jamais le token ni une URL de provider brute. Tout transite par `invoke()` ou le protocole custom `stream://`. Cette garde s'applique uniformément quel que soit le provider connecté — voir `docs/ARCHITECTURE.md` §3.

### Modules Rust

`src-tauri/` est un workspace Cargo avec trois sous-crates (`chelou-manifest`, `chelou-providers`, `keyring-dpapi-store`) en plus du crate principal :

**`src-tauri/` (crate principal — dépend de Tauri)**

| Module | Rôle |
|---|---|
| `src/lib.rs` | Point d'entrée ; wire le handler `stream://`, l'`AppState` et l'`invoke_handler` |
| `src/commands/` | Tous les `#[tauri::command]` : auth + liste/scan/save/delete méthodes, paramétrées par `ProviderId` |
| `src/auth/` | `AuthStore` : credentials d'une connexion active (n'importe quel provider) en mémoire + persistance via keyring-core (store par OS) |
| `src/providers/mod.rs` | Registre : enum `ProviderClient`/`ProviderAuthClient` (dispatch par `match`, un bras par provider) + `make_client`/`make_auth` |
| `src/stream/mod.rs` | Handler `stream://{kind}/{provider}/{fileId}` : parse l'URI, résout le client via le registre, forward le header `Range` |
| `src/manifest/mod.rs` | Stub : `pub use chelou_manifest::*;` — re-exporte le sous-crate |

**`src-tauri/crates/manifest/` (sous-crate `chelou-manifest` — dépendances : serde + anyhow uniquement)**

| Fichier | Rôle |
|---|---|
| `src/lib.rs` | Modèle Rust miroir de `types/model.ts` + `ManifestStore` (un JSON par méthode dans l'app-data dir). `MethodSource.provider` est une `String` libre, non couplée à `chelou-providers` |

**`src-tauri/crates/providers/` (sous-crate `chelou-providers` — dépendances : pas de Tauri, un module par provider)**

| Fichier | Rôle |
|---|---|
| `src/lib.rs` | Types neutres (`Entry`, `FolderContents`, `RangeResponse`), `ProviderId` (Display/FromStr), traits `StorageProvider`/`ProviderAuth`, `StoredCredentials`, `fetch_range` (fonction libre) |
| `src/scanner.rs` | Scan DFS générique (`scan_tree`/`scan_methods_in_folder<P: StorageProvider>`) : buckets par mot-clé, tri naturel, parser BPM, `group_by_radical` — indépendant du provider |
| `src/oauth.rs` | Boucle loopback OAuth partagée (`bind_loopback`, `wait_for_token`) — mécanique HTTP générique, aucune connaissance d'un provider particulier |
| `src/pcloud.rs` | Adapter pCloud : `PCloudClient`/`PCloudAuth`, `map_entry`/`map_folder`, impls `StorageProvider` + `ProviderAuth` |

Tous les providers (pCloud et Dropbox aujourd'hui, Google Drive à venir) vivent comme modules dans ce même sous-crate plutôt qu'un crate séparé chacun — aucun n'a de dépendance système, donc aucune perte de testabilité (cf. §15 ARCHITECTURE.md) à les regrouper. Ce découpage à trois sous-crates permet de tester le modèle, la persistance et la logique provider — et, sur le même principe, le store d'identifiants (`keyring-dpapi-store`, lui aussi sans dépendance Tauri) — sans compiler Tauri ni les dépendances système (GTK, WebKit).

**`src-tauri/crates/keyring-dpapi-store/`** (sous-crate `keyring-dpapi-store` — dépendances : `keyring-core` + `windows-dpapi`, zéro dépendance Tauri, Windows uniquement)

| Fichier | Rôle |
|---|---|
| `src/lib.rs` | `#![cfg(windows)]`. Implémente les traits `CredentialStoreApi`/`CredentialApi` de `keyring-core` : store de credentials backé par des fichiers chiffrés DPAPI (`Scope::User`), un fichier par couple `<service, user>` |

### Modules TypeScript (`src/`)

| Fichier | Rôle |
|---|---|
| `types/model.ts` | Interfaces du modèle (§8 ARCHITECTURE.md) — source de vérité TS |
| `lib/ipc.ts` | Wrappers typés autour de `invoke()` — seule surface d'appel vers Rust ; commandes auth/scan paramétrées par `Provider` |
| `lib/stream.ts` | Constructeurs d'URL `stream://video/{provider}/{id}`, `stream://audio/{provider}/{id}`, etc. |
| `lib/providers.ts` | Registre `PROVIDERS` (label, icône par `Provider`) — source unique pour l'UI (ConnectScreen, FolderPicker, TitleBar) |
| `lib/silence-detection.ts` | Détection du silence de tête d'un backing track (`decodeAudioData` + analyse d'amplitude) |
| `components/organisms/VideoPlayer.tsx` | `<video>` + fallback transcodé automatique sur erreur de décodage |
| `components/templates/TabScreen.tsx` | Vue synchro : `<audio>` + AlphaTab (boucle de synchro, voir ci-dessous) |
| `hooks/useAlphaTabPlayer.ts` | Instancie AlphaTab, branche `<audio>` comme `IExternalMediaHandler` |
| `hooks/useBackingTrackPlayback.ts` | Boucle de synchro : lit `audio.currentTime`, pousse la position à AlphaTab |
| `hooks/useLeadInDetection.ts` | Déclenche la détection de silence de tête, persiste `leadInMsOverride` |
| `components/templates/MethodScreen.tsx`, `LibraryScreen.tsx` | Navigation méthode→leçons, tri par `.order` |
| `components/templates/DocumentsScreen.tsx` | PDF.js + `<img>` pour les images |

### CSS — classe custom vs utilitaires Tailwind

Trois fichiers, chargés dans cet ordre (`index.html`) : `globals.css` (import Tailwind + tokens
de couleur/rayon mappés en `@theme inline` — seule source de vérité pour ces tokens, ne pas les
redéclarer ailleurs), `theme.css` (tokens non couverts par Tailwind : `--pad`, `--gap`, `--row-h`,
`--fs*`, reset de base), `components.css` (classes de composants réutilisables).

**Règle :** une classe dans `components.css` (`.btn`, `.chip`, `.card`, `.tree-row`, `.row-btn`…)
seulement pour un motif visuel réutilisé à plusieurs endroits distincts de l'app. Pour tout le
reste — mise en page, espacement, cas ponctuels — utilitaires Tailwind inline directement dans le
JSX, y compris pour les valeurs hors échelle (`text-[clamp(20px,3vw,44px)]`, `bg-(--pad)`) : elles
restent lisibles sans devoir ouvrir un autre fichier. Passer par `cn()` (`clsx` + `tailwind-merge`)
dès qu'un composant a un `className` conditionnel ou surchargeable depuis l'extérieur.

### Boucle de synchronisation (TabScreen)

Modèle : **audio = horloge maître, AlphaTab = esclave**. Ne jamais faire « jouer » AlphaTab.
`audio.currentTime` (balise `<audio>`, pas `AudioContext`) est l'unique source de vérité, relue
sur `timeupdate`/`seeked` et, pendant la lecture, via un `setInterval` de 50 ms (`useBackingTrackPlayback.ts`) :

```
audioMs = audio.currentTime * 1000
scale   = trackBpm / notatedBpm            // = 1 si le backing track est au tempo noté du .gp
output.updatePosition((audioMs - leadInMs) * scale)
```

`updatePosition(ms)` fait convertir en interne par AlphaTab ce temps en tick selon le tempo
**noté** du `.gp` — d'où la mise à l'échelle par `scale`, qui réexprime le temps écoulé au tempo
du backing track comme s'il s'était écoulé au tempo noté. Reste correct tant que le tempo est
constant sur tout le fichier (hypothèse posée dans ARCHITECTURE.md §7).

`leadInMs = silenceMs + defaultCountInBars × beatsPerBar × 60000 / trackBpm` — `silenceMs` détecté
une fois par track (`lib/silence-detection.ts`) et mis en cache dans `leadInMsOverride` ;
`beatsPerBar` lu depuis `api.score.masterBars[0].timeSignatureNumerator` après `scoreLoaded`.
Détail complet : ARCHITECTURE.md §7.

## Tests

Deux couches distinctes (cf. `docs/ARCHITECTURE.md` §15) :

| Couche | Périmètre | Vitesse | Commande |
|---|---|---|---|
| **Unitaires** | Sous-crates `chelou-manifest`, `chelou-providers` — logique pure, zéro dep Tauri | Rapide, CI ubuntu | `cargo test -p chelou-manifest -p chelou-providers` |
| **Intégration** | Comportement complet (Tauri + WebView2 + pCloud) | Lent, lié à la plateforme | *(aucun pour l'instant)* |

**Règle :** toute logique testable unitairement vit dans un sous-crate sans dep Tauri. Si une
fonction nécessite Tauri pour compiler, la refacto vers un sous-crate est la réponse — pas un mock.

## Règles non négociables

1. **Le WebView ne voit jamais le token ni une URL brute du provider de stockage.** Tout transite par Rust via `stream://` ou `invoke()` — règle uniforme quel que soit le provider connecté, pas seulement une conséquence de la contrainte pCloud ci-dessous (cf. `docs/ARCHITECTURE.md` §3).
2. **Endpoint pCloud EU : `eapi.pcloud.com`** (jamais l'endpoint US `api.pcloud.com`). Spécifique à pCloud — ne généralise pas aux autres providers.
3. **L'audio est l'horloge maître ; AlphaTab est piloté.** Voir boucle ci-dessus.
4. **AlphaTab est l'unique moteur de tablature.** Jamais de PDF dans la vue synchro.
5. **Ne pas appairer tab ↔ backing track par le nom.** Sur-rattacher en pool (cf. `docs/ARCHITECTURE.md` §6).

## APIs vérifiées (ne pas deviner depuis la mémoire)

### AlphaTab 1.8.3

```typescript
import * as alphaTab from '@coderline/alphatab';

// IExternalMediaHandler et IExternalMediaSynthOutput sont déclarés dans alphaTab.d.ts
// mais NON exportés du module. Les déclarer localement (typage structurel).
interface IExternalMediaHandler {
  readonly backingTrackDuration: number; // durée WAV en ms
  playbackRate: number;
  masterVolume: number;
  seekTo(timeMs: number): void;
  play(): void;
  pause(): void;
}
interface IExternalMediaSynthOutput {
  handler: IExternalMediaHandler | null;
}

// Init
const api = new alphaTab.AlphaTabApi(element, {
  player: { playerMode: alphaTab.PlayerMode.EnabledExternalMedia }, // = 4
});

// Branchement handler (après playerReady)
// Cast double nécessaire : ISynthOutput (type de player.output) n'overlaps pas IExternalMediaSynthOutput
api.playerReady.on(() => {
  const player = api.player;
  if (player) {
    (player.output as unknown as IExternalMediaSynthOutput).handler = monHandler;
  }
});

// beatsPerBar (après scoreLoaded)
api.scoreLoaded.on((score) => {
  beatsPerBar = score.masterBars[0].timeSignatureNumerator;
});
```

### Tauri v2 — handler stream://

```rust
// Le premier paramètre est UriSchemeContext, pas AppHandle.
use tauri::{Manager, Runtime, UriSchemeContext, UriSchemeResponder};

pub fn handle<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: tauri::http::Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = ctx.app_handle().clone(); // cloner avant spawn ('static requis)
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>(); // Manager en scope requis
        // ...
    });
}
```

## Conventions

- `crates/manifest/src/lib.rs` (Rust) et `types/model.ts` (TS) doivent rester en miroir (serde `rename` pour camelCase). `src/manifest/mod.rs` est un stub `pub use chelou_manifest::*` — ne pas y écrire de logique.
- Token/credentials du provider connecté : jamais en clair sur disque — `keyring-core::Entry`, backé par un store par OS : `keyring-dpapi-store` (maison, fichier chiffré DPAPI `Scope::User`) sur Windows, `apple-native-keyring-store` (Keychain) sur macOS, `zbus-secret-service-keyring-store` (Secret Service) sur Linux. Le store Windows maison existe parce que Windows Credential Manager plafonne un mot de passe générique à ~2560 caractères UTF-16, dépassé par certains couples access+refresh token (Dropbox) ; `tauri-plugin-stronghold` a aussi été essayé et abandonné (commit à 60s+, round-trip peu fiable) — cf. `docs/ARCHITECTURE.md` §5.
- Ajouter un provider (Google Drive, Dropbox, …) = un nouveau module dans `crates/providers/src/` implémentant `StorageProvider` + `ProviderAuth`, plus un bras dans `ProviderId`/`make_client`/`make_auth` (Rust) et dans `PROVIDERS` (`src/lib/providers.ts`) — pas de nouveau crate sauf si ce provider a une dépendance système (cf. `docs/ARCHITECTURE.md` §15 et la conception dans les décisions d'architecture).
