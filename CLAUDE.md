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

# CI TypeScript : biome + tsc + vitest (lancé par GitHub Actions)
npm run ci

# Vérification rapide Rust (pas de link — beaucoup plus rapide que build)
cd src-tauri && cargo check

# Lints Rust
cd src-tauri && cargo clippy

# Windows : la feature `asio` de cpal exige LLVM (`LIBCLANG_PATH`, ex. `C:\Program Files\LLVM\bin`)
# et, optionnellement, le SDK ASIO Steinberg (`CPAL_ASIO_DIR` ; sinon asio-sys le télécharge) —
# à placer dans le `[env]` de src-tauri/.cargo/config.toml (cf. config.toml.example)

# Vérification manuelle du moteur audio (sortie seule / son direct de la guitare)
cd src-tauri && cargo run -p chelou-audio-engine --example play -- <fichier audio>
cd src-tauri && cargo run -p chelou-audio-engine --example duplex -- --host asio --device "Focusrite USB ASIO" --input 2

# Vérification manuelle d'un plugin d'ampli VST3 sur la voie guitare (`--editor` : son interface, Windows)
cd src-tauri && cargo run --release -p chelou-vst3-host --example amp -- --plugin "<bundle.vst3>" --device "Focusrite USB ASIO" --input 2 --editor

# Tests unitaires Rust (sous-crates sans dépendances système Tauri — cf. §15 ARCHITECTURE.md)
cd src-tauri && cargo test -p chelou-manifest -p chelou-providers -p chelou-audio-core

# Vérification TypeScript sans emit
npx tsc --noEmit
```

## Stack

- **Tauri v2** (Rust backend + WebView2 sur Windows), frontend TypeScript + Vite.
- **AlphaTab** (`@coderline/alphatab@1.8.4`) pour le rendu et la synchro des tablatures GuitarPro.
- **PDF.js** (`pdfjs-dist`) pour la section documents.
- **Moteur audio natif Rust** (`cpal` + `symphonia` + `rubato`, sous-crates `chelou-audio-core`/`chelou-audio-engine`) pour la lecture des backing tracks : téléchargés et décodés en entier avant lecture, lus à vitesse 1, détection du silence de tête au décodage. Sans carte configurée : sortie par défaut du système ; carte choisie : duplex (ASIO/WASAPI sur Windows, CoreAudio, ALSA) avec la guitare passée dans un plugin d'ampli VST3 (sinon en son direct).
- **Hôte VST3** (`vst3` + `libloading`, sous-crate `chelou-vst3-host`) : un seul plugin sur la voie guitare, chargé sur le thread principal, interface native dans une fenêtre Win32 (Windows seulement pour l'instant). Le métronome reste un `AudioContext` côté WebView (en attendant son passage dans le moteur).
- **reqwest** côté Rust pour les appels aux providers + streaming Range.
- **keyring-core** pour le token du provider connecté (store par OS — `keyring-dpapi-store` maison sur Windows, Keychain/Secret Service natifs ailleurs).

## Architecture

### Frontière Rust / TypeScript

```
TS (WebView)          Rust (Tauri backend)                 Provider cloud (pCloud aujourd'hui)
──────────────────    ─────────────────────────────        ────────────────────────────────────
invoke("cmd")    ──►  commands/mod.rs                 ──►  eapi.pcloud.com
fetch("stream://") ►  stream/mod.rs (protocol)         ──►  bytes (Range)
invoke("backing_*") ► audio/mod.rs                    ──►  fichier entier (resolve_media)
Channel ◄──────────── backing_status (~60 Hz)              crates/audio-engine (cpal : sortie seule / duplex)
invoke("audio_*") ──► audio/mod.rs (réglages, cartes)
Channel ◄──────────── audio_status (~30 Hz, vumètres)
invoke("plugin_*") ─► plugin/mod.rs (thread principal)    crates/vst3-host (scan en processus enfant)
                      auth/mod.rs (keyring-core, store par OS)
                      providers/mod.rs (registre ProviderClient/ProviderAuthClient)
                      crates/providers (trait StorageProvider + adapters par provider)
                      manifest/mod.rs (JSON local)
```

Le WebView ne touche jamais le token ni une URL de provider brute. Tout transite par `invoke()`, le protocole custom `stream://` ou un `Channel` Tauri (statut du moteur audio). Cette garde s'applique uniformément quel que soit le provider connecté — voir `docs/ARCHITECTURE.md` §3.

### Modules Rust

`src-tauri/` est un workspace Cargo avec six sous-crates (`chelou-manifest`, `chelou-providers`, `keyring-dpapi-store`, `chelou-audio-core`, `chelou-audio-engine`, `chelou-vst3-host`) en plus du crate principal :

**`src-tauri/` (crate principal — dépend de Tauri)**

| Module | Rôle |
|---|---|
| `src/lib.rs` | Point d'entrée ; wire le handler `stream://`, l'`AppState` et l'`invoke_handler` |
| `src/commands/` | Tous les `#[tauri::command]` : auth (`oauth_start`, `connect_local`) + liste/scan/save/delete méthodes, paramétrées par `ProviderId` |
| `src/auth/` | `AuthStore` : credentials d'une connexion active (n'importe quel provider) en mémoire + persistance via keyring-core (store par OS) |
| `src/providers/mod.rs` | Registre : enum `ProviderClient`/`ProviderAuthClient` (dispatch par `match`, un bras par provider) + `make_client`/`make_auth` |
| `src/stream/mod.rs` | Handler `stream://{kind}/{provider}/{fileId}` : parse l'URI, résout le client via le registre, forward le header `Range` |
| `src/media_server.rs` | **Linux uniquement.** Serveur HTTP loopback `127.0.0.1:47812` pour `<video>` (GStreamer ne sait pas lire un schéma custom). Sensible : exige le jeton de session `?t=` (`AppState::media_token`, servi au WebView par `get_media_token`) et ne donne le CORS qu'à l'origine de l'app — cf. ARCHITECTURE.md §4 |
| `src/manifest/mod.rs` | Stub : `pub use chelou_manifest::*;` — re-exporte le sous-crate |
| `src/audio/mod.rs` | Commandes `backing_*` (load/unload/play/pause/seek/set_volume) et `backing_status` (statut poussé au WebView par `tauri::ipc::Channel`, ~60 Hz pendant la lecture) ; carte son : `audio_devices`, `audio_settings`, `audio_configure`, `audio_set_enabled`, `guitar_set_fader`/`guitar_set_muted`, `audio_reset_xruns`, `audio_status` (~30 Hz, permanent) ; réglages lus au démarrage depuis `audio/settings.json` (hors de l'app dir, où chaque `.json` est un manifeste) — cf. ARCHITECTURE.md §7, §9 |
| `src/plugin/mod.rs` | Plugin d'ampli VST3 : instance et interface vivent sur le **thread principal** (`thread_local!`, `on_main`) ; commandes `plugin_folders`/`plugin_add_folder`/`plugin_remove_folder`/`plugin_scan`/`plugin_load` (par identifiant du dernier scan, jamais par chemin)/`plugin_unload`/`plugin_open_editor` ; rechargement au démarrage et après un changement de fréquence ; démontage ordonné à la sortie (`RunEvent::Exit`) ; `probe_main` = mode `--vst3-probe <bundle>` du binaire (`main.rs`) |

**`src-tauri/crates/manifest/` (sous-crate `chelou-manifest` — dépendances : serde + anyhow uniquement)**

| Fichier | Rôle |
|---|---|
| `src/lib.rs` | Modèle Rust miroir de `types/model.ts` + `ManifestStore` (un JSON par méthode dans l'app-data dir). `MethodSource.provider` est une `String` libre, non couplée à `chelou-providers` |

**`src-tauri/crates/providers/` (sous-crate `chelou-providers` — dépendances : pas de Tauri, un module par provider)**

| Fichier | Rôle |
|---|---|
| `src/lib.rs` | Types neutres (`Entry`, `RangeResponse`, `DownloadTarget` = `Http` \| `File`), `ProviderId` (Display/FromStr), traits `StorageProvider`/`ProviderAuth`, `StoredCredentials`, `fetch_range` (fonction libre), `random_token`, `redact_urls` (toute erreur renvoyée au WebView passe par là) |
| `src/scanner.rs` | Scan DFS générique (`scan_tree`/`scan_methods_in_folder<P: StorageProvider>`) : buckets par mot-clé, tri naturel, parser BPM, `group_by_radical` — indépendant du provider |
| `src/oauth.rs` | Boucle loopback OAuth partagée (`bind_loopback`, `wait_for_code`) avec `state` anti-CSRF + PKCE S256, et `request_token` (échange/refresh sur un token endpoint RFC 6749 standard) — aucune connaissance d'un provider particulier |
| `src/registry.rs` | Enums `ProviderClient`/`ProviderAuthClient` (dispatch par `match`) + `make_client`/`make_auth` |
| `src/pcloud.rs` | Adapter pCloud : `PCloudClient`/`PCloudAuth`, `map_entry`, impls `StorageProvider` + `ProviderAuth` (pas de PKCE côté pCloud) |
| `src/dropbox.rs`, `src/gdrive.rs` | Adapters Dropbox / Google Drive (Drive masqué dans l'UI, cf. `ProviderId::GoogleDrive`) |
| `src/local.rs` | Provider local (disque ou montage réseau) : `LocalClient` enraciné sur le dossier choisi par `connect_local` (chemin stocké dans `access_token`), ids = base64url du chemin relatif, `open_range`/`read_range` pour `DownloadTarget::File` — cf. ARCHITECTURE.md §14 |

Tous les providers (pCloud, Dropbox, Google Drive) vivent comme modules dans ce même sous-crate plutôt qu'un crate séparé chacun — aucun n'a de dépendance système, donc aucune perte de testabilité (cf. §15 ARCHITECTURE.md) à les regrouper. Ce découpage en sous-crates permet de tester le modèle, la persistance, la logique provider et la logique audio (`chelou-audio-core`) — et, sur le même principe, le store d'identifiants (`keyring-dpapi-store`, lui aussi sans dépendance Tauri) — sans compiler Tauri ni les dépendances système (GTK, WebKit, ALSA).

**`src-tauri/crates/audio-core/`** (sous-crate `chelou-audio-core` — `symphonia`, `rubato`, `rtrb` : zéro dépendance système, testé en CI)

| Fichier | Rôle |
|---|---|
| `src/decode.rs` | `decode_stereo` (wav/mp3/flac → stéréo f32 entrelacé, paquets corrompus sautés) + `resample` (fréquence de la sortie, durée conservée) |
| `src/player.rs` | `BackingPlayer` (côté callback audio : aucune allocation/verrou/libération) + `PlayerControl` (ordres par file lock-free, statut par atomiques, tracks remplacés libérés hors thread audio, `load` refuse un track rééchantillonné pour une autre fréquence) + `LoadGeneration` (un chargement périmé ne remplace pas le track en place) |
| `src/silence.rs` | `scan_leading_silence` : fin du silence de tête (pic par fenêtre, plancher au percentile bas) |
| `src/gain.rs` | `GainRamp` (rampe linéaire de 256 frames, indépendante du découpage en blocs) + `fader_gain` (fader 0-100, 80 = 0 dB → gain `(v/80)²`) |
| `src/guitar.rs` | `GuitarLane` (callback : entrée mono passée dans l'effet `GuitarFx` s'il y en a un, sinon dupliquée en stéréo, × gain en rampe, crêtes en atomiques ; `install_fx` refuse un effet préparé pour une autre fréquence que celle des flux, `set_stream_rate` retire l'effet quand elle change, `take_fx`) + `GuitarControl` (volume, mute, lecture des crêtes) |
| `src/settings.rs` | `AudioSettings`/`DuplexConfig` (miroir de `lib/ipc.ts` ; plugin choisi, dossiers VST3, dernier scan), `load_settings`/`save_settings` (écriture atomique, `schemaVersion`), `latency_ms` (buffers + latence plugin), `explain_open_error` (pilote ASIO occupé) |
| `src/plugins.rs` | Scan VST3 sans rien charger : `parse_module_info` (JSON5), `is_listed` (classes audio, instruments purs exclus), `find_bundles`, `binary_path` par OS, `standard_vst3_folders`, `dedupe` ; `PluginInfo` (désigné par `id` = CID) |

**`src-tauri/crates/audio-engine/`** (sous-crate `chelou-audio-engine` — `cpal`, donc ALSA sur Linux : hors de la commande de test canonique, ses tests ne tournent qu'en local via `cargo test -p chelou-audio-engine`)

| Fichier | Rôle |
|---|---|
| `src/lib.rs` | `start(config)` / `Engine::apply` : thread dédié propriétaire des flux, en sortie seule (sortie par défaut, rouverte chaque seconde si elle disparaît) ou en duplex (entrée → ring `rtrb` → voie guitare + `BackingPlayer` → sortie ; échec ou carte perdue → repli en sortie seule avec l'erreur dans `EngineState`). Absorbe l'à-coup d'ouverture, compte les coupures ; `requires_rebuild` : les accrocs dont le flux se remet seul ne reconstruisent rien. `examples/play.rs` et `examples/duplex.rs` pour une vérification manuelle |
| `src/devices.rs` | `list_devices` : cartes par pilote (canaux, fréquences, bornes du buffer) ; `Engine::devices` y ajoute la carte ASIO ouverte (un pilote ASIO chargé empêche d'énumérer les autres) |

`Engine::install_fx(fx, rate)` installe le plugin dans la voie guitare (rend l'ancien, libéré par l'appelant ; refusé si les flux tournent entre-temps à une autre fréquence) ; des flux rouverts à une autre fréquence retirent l'effet (`Engine::fx_cleared_at`), que l'app recharge.

**`src-tauri/crates/vst3-host/`** (sous-crate `chelou-vst3-host` — `vst3`, `libloading`, `windows` (Win32) ; pas testable sans vrais plugins, hors de la commande de test canonique ; `cargo test -p chelou-vst3-host` en local pour le scan)

| Fichier | Rôle |
|---|---|
| `src/module.rs` | Chargement d'un module par OS (`InitDll` Windows, `ModuleEntry` Linux, `bundleEntry` macOS — non testé), classes de la fabrique, `probe` |
| `src/plugin.rs` | `Vst3Plugin` (thread principal ; contrôleur séparé + `IConnectionPoint`, bus mono sinon stéréo, préparation à la fréquence réelle, pré-chauffage, état, démontage ordonné — fuite volontaire si la partie temps réel vit encore) + `RtProc` (`GuitarFx` : éditions de l'interface → `IParameterChanges` pré-alloué) |
| `src/host_objects.rs` | Objets COM fournis au plugin (`IHostApplication` qui fabrique `IMessage`/`IAttributeList`, flux mémoire, `IComponentHandler`, changements de paramètres) |
| `src/editor.rs` | `Editor` : interface du plugin dans une fenêtre Win32 de premier niveau (pas de boucle de messages : celle de Tauri), vue détachée avant destruction ; erreur explicite hors Windows |
| `src/scan.rs` | `scan` : `moduleinfo.json`, sinon `run_probe` (processus enfant `--vst3-probe`, tué après 15 s) |

**`src-tauri/crates/keyring-dpapi-store/`** (sous-crate `keyring-dpapi-store` — dépendances : `keyring-core` + `windows-dpapi`, zéro dépendance Tauri, Windows uniquement)

| Fichier | Rôle |
|---|---|
| `src/lib.rs` | `#![cfg(windows)]`. Implémente les traits `CredentialStoreApi`/`CredentialApi` de `keyring-core` : store de credentials backé par des fichiers chiffrés DPAPI (`Scope::User`), un fichier par couple `<service, user>` |

### Modules TypeScript (`src/`)

| Fichier | Rôle |
|---|---|
| `types/model.ts` | Interfaces du modèle (§8 ARCHITECTURE.md) — source de vérité TS |
| `lib/ipc.ts` | Wrappers typés autour de `invoke()` — seule surface d'appel vers Rust ; commandes auth/scan paramétrées par `Provider`, commandes `backing_*` du moteur audio et `subscribeBackingStatus` (`Channel`), commandes `audio_*`/`guitar_*` de la carte son et `subscribeAudioStatus`, commandes `plugin_*` |
| `lib/stream.ts` | Constructeurs d'URL `stream://video/{provider}/{id}`, `stream://tab/…`, `stream://doc/…` (serveur loopback + jeton sur Linux, `initMediaToken` appelé avant le premier rendu) ; `fetchStream` = `fetch` qui rejette sur statut non-2xx |
| `lib/providers.ts` | Registre `PROVIDERS` (label, icône par `Provider`) — source unique pour l'UI (ConnectScreen, FolderPicker, TitleBar) |
| `lib/serial-queue.ts` | `createSerialQueue` : file d'exécution séquentielle (testée vitest) — ordonne chargements et déchargements du moteur entre toutes les vues |
| `lib/fader.ts` | `faderDb` : position de fader (80 = 0 dB) en dB affichables (testé vitest) |
| `contexts/AudioEngineContext.tsx` | `useAudioEngine()` : abonnement unique au statut de la carte (crêtes gardées hors React pour les vumètres), réglages, inventaire des cartes, actions, ouverture du tiroir de réglages |
| `components/organisms/GuitarControl.tsx` | Barre de titre : état de la carte (« Configurer l'audio », « Audio arrêté », « Audio : erreur ») ou mute + vumètre guitare + popover (plugin et son interface, volume) |
| `components/templates/AudioSettings.tsx` | Tiroir des réglages audio : état de la carte, pilote, carte (WASAPI : entrée et sortie séparées), entrée guitare, sortie, fréquence, buffer, latence, coupures |
| `components/molecules/LevelMeter.tsx` | Vumètre animé par `requestAnimationFrame`, hors rendu React |
| `components/organisms/PluginSection.tsx` | Section « Plugin d'ampli » des réglages : plugin actif (interface, retrait), bandeau « introuvable », scan et liste des VST3, dossiers scannés |
| `lib/lead-in.ts` | `leadInFromSilence` : lead-in à partir du silence détecté par le moteur, refusé si les infos se rapportent à un autre track (testé vitest) |
| `components/organisms/VideoPlayer.tsx` | `<video>` + fallback transcodé automatique sur erreur de décodage |
| `components/templates/TabScreen.tsx` | Vue synchro : moteur natif + AlphaTab (boucle de synchro, voir ci-dessous) |
| `hooks/useBackingEngine.ts` | Backing track du moteur natif : chargement et déchargement (en quittant la vue) passés par une file partagée par toutes les vues, abonnement au statut, rechargement si la fréquence de sortie change |
| `hooks/useAlphaTabPlayer.ts` | Instancie AlphaTab, branche le moteur comme `IExternalMediaHandler` |
| `hooks/useBackingTrackPlayback.ts` | Boucle de synchro : reçoit le statut du moteur, pousse la position à AlphaTab |
| `hooks/useLeadInDetection.ts` | Calcule le lead-in à partir du silence détecté côté Rust, persiste `leadInMsOverride` |
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
La position de lecture du moteur audio natif est l'unique source de vérité, poussée par
`backing_status` toutes les 16 ms pendant la lecture et à chaque changement sinon
(`useBackingTrackPlayback.onStatus`) :

```
audioMs = status.positionMs
scale   = trackBpm / notatedBpm            // = 1 si le backing track est au tempo noté du .gp
output.updatePosition((audioMs - leadInMs) * scale)
```

`updatePosition(ms)` fait convertir en interne par AlphaTab ce temps en tick selon le tempo
**noté** du `.gp` — d'où la mise à l'échelle par `scale`, qui réexprime le temps écoulé au tempo
du backing track comme s'il s'était écoulé au tempo noté. Reste correct tant que le tempo est
constant sur tout le fichier (hypothèse posée dans ARCHITECTURE.md §7).

`leadInMs = silenceMs + defaultCountInBars × beatsPerBar × 60000 / trackBpm` — `silenceMs` détecté
par `backing_load` au décodage (`chelou_audio_core::silence`) et mis en cache dans `leadInMsOverride` ;
`beatsPerBar` lu depuis `api.score.masterBars[0].timeSignatureNumerator` après `scoreLoaded`.
Détail complet : ARCHITECTURE.md §7.

## Tests

Deux couches distinctes (cf. `docs/ARCHITECTURE.md` §15) :

| Couche | Périmètre | Vitesse | Commande |
|---|---|---|---|
| **Unitaires** | Sous-crates `chelou-manifest`, `chelou-providers`, `chelou-audio-core` — logique pure, zéro dep Tauri | Rapide, CI ubuntu | `cargo test -p chelou-manifest -p chelou-providers -p chelou-audio-core` |
| **Unitaires TS** | Logique pure du frontend (`src/lib/*.test.ts`) | Rapide | `npx vitest run` (inclus dans `npm run ci`) |
| **Intégration** | Comportement complet (Tauri + WebView2 + pCloud) | Lent, lié à la plateforme | *(aucun pour l'instant)* |

**Règle :** toute logique testable unitairement vit dans un sous-crate sans dep Tauri. Si une
fonction nécessite Tauri pour compiler, la refacto vers un sous-crate est la réponse — pas un mock.

## Règles non négociables

1. **Le WebView ne voit jamais le token ni une URL brute du provider de stockage.** Tout transite par Rust via `stream://` ou `invoke()` — règle uniforme quel que soit le provider connecté, pas seulement une conséquence de la contrainte pCloud ci-dessous (cf. `docs/ARCHITECTURE.md` §3). Ça vaut aussi pour les **messages d'erreur** : reqwest met l'URL (lien pré-signé) dans son `Display` — passer par `redact_urls` / `public_err` avant de renvoyer une erreur au WebView.
2. **Endpoint pCloud EU : `eapi.pcloud.com`** (jamais l'endpoint US `api.pcloud.com`). Spécifique à pCloud — ne généralise pas aux autres providers.
3. **Le moteur audio natif est l'horloge maître ; AlphaTab est piloté.** Voir boucle ci-dessus.
4. **AlphaTab est l'unique moteur de tablature.** Jamais de PDF dans la vue synchro.
5. **Ne pas appairer tab ↔ backing track par le nom.** Sur-rattacher en pool (cf. `docs/ARCHITECTURE.md` §6).

## APIs vérifiées (ne pas deviner depuis la mémoire)

### AlphaTab 1.8.4

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
