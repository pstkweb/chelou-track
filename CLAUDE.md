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

# Vérification rapide Rust (pas de link — beaucoup plus rapide que build)
cd src-tauri && cargo check

# Lints Rust
cd src-tauri && cargo clippy

# Vérification TypeScript sans emit
npx tsc --noEmit
```

Pas de suite de tests pour l'instant.

## Stack

- **Tauri v2** (Rust backend + WebView2 sur Windows), frontend TypeScript + Vite.
- **AlphaTab** (`@coderline/alphatab@1.8.3`) pour le rendu et la synchro des tablatures GuitarPro.
- **PDF.js** (`pdfjs-dist`) pour la section documents.
- **Web Audio API** (`AudioContext` / `AudioBufferSourceNode`) — pas `<audio>` — pour la précision de synchro.
- **reqwest** côté Rust pour les appels pCloud + streaming Range.
- **keyring** pour le token pCloud (Windows Credential Manager).

## Architecture

### Frontière Rust / TypeScript

```
TS (WebView)          Rust (Tauri backend)           pCloud EU
──────────────────    ───────────────────────        ──────────────
invoke("cmd")    ──►  commands/mod.rs           ──►  eapi.pcloud.com
fetch("stream://") ►  stream/mod.rs (protocol)  ──►  bytes (Range)
                      auth/mod.rs (keyring)
                      pcloud/mod.rs (API client)
                      pcloud/scanner.rs (scan DFS)
                      manifest/mod.rs (JSON local)
```

Le WebView ne touche jamais le token ni une URL pCloud brute. Tout transite par `invoke()` ou le protocole custom `stream://`.

### Modules Rust (`src-tauri/src/`)

| Module | Rôle |
|---|---|
| `lib.rs` | Point d'entrée ; wire le handler `stream://`, l'`AppState` et l'`invoke_handler` |
| `commands/` | Tous les `#[tauri::command]` : auth + liste/scan/save/delete méthodes |
| `auth/` | `AuthStore` : token en mémoire + persistance keychain (Windows Credential Manager) |
| `pcloud/mod.rs` | `PCloudClient` : `getfilelink`, `getvideolink`, `listfolder`, `fetch_range` |
| `pcloud/scanner.rs` | Scan DFS pCloud → `Method` : buckets par mot-clé, tri naturel, parser BPM, `group_by_radical` |
| `stream/mod.rs` | Handler `stream://` : parse l'URI, appelle `getfilelink`/`getvideolink`, forward le header `Range` |
| `manifest/mod.rs` | Miroir Rust du modèle TS + `ManifestStore` (un JSON par méthode dans l'app-data dir) |

### Modules TypeScript (`src/`)

| Fichier | Rôle |
|---|---|
| `types/model.ts` | Interfaces du modèle (§8 ARCHITECTURE.md) — source de vérité TS |
| `lib/ipc.ts` | Wrappers typés autour de `invoke()` — seule surface d'appel vers Rust |
| `lib/stream.ts` | Constructeurs d'URL `stream://video/{id}`, `stream://audio/{id}`, etc. |
| `components/player/VideoPlayer.ts` | `<video>` + fallback transcodé automatique sur erreur de décodage |
| `components/player/SyncView.ts` | Sync audio↔AlphaTab (voir ci-dessous) |
| `components/catalogue/Catalogue.ts` | Navigation méthode→leçons, tri par `.order` |
| `components/documents/DocViewer.ts` | PDF.js + `<img>` pour les images |

### Boucle de synchronisation (SyncView)

Modèle : **audio = horloge maître, AlphaTab = esclave**. Ne jamais faire « jouer » AlphaTab.

```
AudioBufferSourceNode.start()
  └─ rAF loop :
       audioMs      = (audioCtx.currentTime - t0) * 1000
       beatsElapsed = max(0, (audioMs - leadInMs) / 60000 * trackBpm)
       tick         = beatsElapsed * 960          // 960 ticks/noire (constante AlphaTab)
       api.tickPosition = tick                    // pousse le curseur, bypass BPM du .gp
```

`api.tickPosition` (pas `output.updatePosition(ms)`) : on calcule les ticks depuis le BPM du backing track, pas depuis le tempo du score. C'est la seule façon de rester synchronisé pour les versions lentes.

`leadInMs = defaultCountInBars × beatsPerBar × 60000 / trackBpm` — `beatsPerBar` lu depuis `api.score.masterBars[0].timeSignatureNumerator` après `scoreLoaded`.

## Règles non négociables

1. **Le WebView ne voit jamais le token pCloud ni une URL pCloud brute.** Tout transite par Rust via `stream://` ou `invoke()`.
2. **Endpoint pCloud EU : `eapi.pcloud.com`** (jamais l'endpoint US `api.pcloud.com`).
3. **L'audio est l'horloge maître ; AlphaTab est piloté.** Voir boucle ci-dessus.
4. **AlphaTab est l'unique moteur de tablature.** Jamais de PDF dans la vue synchro.
5. **Ne pas appairer tab ↔ backing track par le nom.** Sur-rattacher en pool (cf. `docs/ARCHITECTURE.md` §6).

## APIs vérifiées (ne pas deviner depuis la mémoire)

### AlphaTab 1.8.3

```typescript
import * as alphaTab from '@coderline/alphatab';
import type { IExternalMediaHandler, IExternalMediaSynthOutput } from '@coderline/alphatab';

// Init
const api = new alphaTab.AlphaTabApi(element, {
  player: { playerMode: alphaTab.PlayerMode.EnabledExternalMedia }, // = 4
});

// Branchement handler (après playerReady)
api.playerReady.on(() => {
  (api.player!.output as IExternalMediaSynthOutput).handler = monHandler;
});

// beatsPerBar (après scoreLoaded)
api.scoreLoaded.on((score) => {
  beatsPerBar = score.masterBars[0].timeSignatureNumerator;
});

// IExternalMediaHandler — AlphaTab appelle ces méthodes sur toi
interface IExternalMediaHandler {
  readonly backingTrackDuration: number; // durée WAV en ms
  playbackRate: number;
  masterVolume: number;
  seekTo(timeMs: number): void;
  play(): void;
  pause(): void;
}
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

- `manifest/mod.rs` et `types/model.ts` doivent rester en miroir (serde `rename` pour camelCase).
- `AudioBufferSourceNode` ne peut pas être restarted : le recréer depuis zéro pour chaque `seekTo`.
- Token pCloud : jamais en clair sur disque, toujours via `keyring::Entry`.
