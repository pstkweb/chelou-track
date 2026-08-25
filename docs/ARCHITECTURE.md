# Architecture — Lecteur de méthodes guitare (Tauri)

> Source de vérité du projet. Toute décision structurelle doit être cohérente avec
> ce document. Les choix ci-dessous ont été tranchés volontairement ; ne pas les
> re-dériver ni les contredire sans raison explicite.

## 1. Objectif

Application desktop Windows pour suivre des méthodes de guitare achetées (vidéos +
exercices) stockées sur pCloud. Lecture des vidéos dans l'ordre du cursus, et vue
ergonomique synchronisant un **backing track audio** avec la **tablature GuitarPro**
correspondante. Objectif transverse : **lecture en streaming sans téléchargement** et
**le moins d'actions manuelles possible** avant de pouvoir utiliser l'outil.

## 2. Stack technique

- **Shell desktop : Tauri v2** (backend Rust + WebView2 système sur Windows).
- **Frontend : TypeScript.**
- **Rendu + synchro tablature : AlphaTab** (version JS/web). Seul moteur sérieux pour
  afficher *et* faire défiler un curseur sur des fichiers GuitarPro. Gère `.gp3/.gp4/.gp5`,
  `.gpx` (GP6) et `.gp` (GP7+).
- **PDF : PDF.js** (uniquement pour la section « documents », voir §10).
- **Audio : balise `<audio>`** pointée sur `stream://audio/{fileId}` — streaming progressif
  (mêmes Range requests que la vidéo, cohérent avec l'objectif « pas de téléchargement », §1),
  `playbackRate` natif avec préservation du pitch pour les variantes de tempo.
  `AudioContext`/`decodeAudioData` réservé à un usage ponctuel et séparé : l'analyse en tâche de
  fond du silence de tête d'un track (§7), qui a besoin d'un buffer décodé mais pas de lecture
  temps réel.

Electron a été explicitement écarté.

## 3. Contraintes dures (load-bearing — ne jamais violer)

1. **Le WebView ne voit jamais le token ni une URL brute du provider de stockage.** Tout passe
   par le backend Rust. C'est une exigence de sécurité **appliquée uniformément à tous les
   providers** (pCloud aujourd'hui ; Google Drive, Dropbox et autres potentiellement demain,
   cf. décisions d'architecture multi-provider) — pas seulement une conséquence de la contrainte
   2 ci-dessous. L'IP-binding pCloud (contrainte 2) est une raison suffisante pour pCloud, mais
   ce n'est pas *la* raison de la règle : même un provider sans cette contrainte technique
   (Google Drive et Dropbox n'ont pas d'IP-binding sur leurs liens temporaires) doit passer par
   le même chemin, par cohérence et pour garder le token hors du WebView dans tous les cas.
2. **Les liens pCloud sont liés à l'IP qui les génère** et leur referer est restreint à
   `pcloud.com` (une appli web qui les consomme directement reçoit `Invalid link referer`
   + erreurs CORS). Conséquence : c'est **Rust** qui génère ET consomme le lien, sur la même
   machine donc la même IP. Une appli purement navigateur est impossible — d'où le shell natif.
   **Spécifique à pCloud** — ne pas supposer que ça généralise aux autres providers.
3. **Endpoint EU obligatoire : `eapi.pcloud.com`** (utilisateur en France). Utiliser
   `api.pcloud.com` (US) donne des liens sous-optimaux voire invalides.
4. **AlphaTab est l'unique moteur de tablature.** Ne pas tenter d'afficher le PDF dans la
   vue synchro : un PDF est figé, sans notion de position musicale.

## 4. Streaming — protocole URI custom

Ne jamais servir une URL pCloud directement au WebView. Implémenter un **protocole custom**
`stream://` via `register_asynchronous_uri_scheme_protocol` (Tauri v2) — **pas** un serveur
localhost (pas de port à gérer, pas de CORS).

Flux : le front demande `stream://video/{fileId}` → Rust appelle l'API pCloud (EU) pour
obtenir le lien (`getfilelink` par défaut) → Rust récupère les octets et les renvoie **en
supportant les Range requests**.

Conséquences :
- IP-binding réglé (Rust génère et consomme le lien sur la même IP).
- Referer / CORS réglés (Rust maîtrise les headers ; `stream://` n'a pas de souci d'origine).
- Le token ne quitte jamais Rust.
- Range = **seek dans la vidéo sans tout télécharger**.
- « Pas de téléchargement » = pass-through en flux, rien écrit sur disque.

**Fallback vidéo transcodée :** si la balise `<video>` lève un `error` de décodage, le front
redemande le même `fileId` en mode transcodé → Rust route vers `getvideolink` (flux à
bitrate/résolution réduits) au lieu de `getfilelink`. Côté `stream://`, c'est un simple
paramètre supplémentaire. Coût nul si jamais déclenché. **Ne pas** sonder les codecs à
l'import : on laisse échouer puis on rattrape.

Le `.wav` : streamé via `<audio src="stream://audio/{fileId}">`, même mécanisme Range que la
vidéo — lecture immédiate sans attendre le téléchargement complet. Seule exception : la
détection du silence de tête (§7) télécharge le fichier en entier et le décode via
`decodeAudioData` — coût accepté car ponctuel (une fois par track, résultat mis en cache dans
`leadInMsOverride`), sans rapport avec la lecture elle-même. Le `.gp`/`.gpx` est minuscule,
chargé en `ArrayBuffer`.

## 5. Frontière Rust / TS

Règle : **token pCloud + octets → Rust. Rendu + synchro → TS.**

**Rust (backend Tauri) :**
- Auth pCloud/Dropbox + stockage sécurisé du token : `keyring_core::Entry`, jamais en clair sur
  disque. Le store backend derrière `Entry` est choisi par OS au démarrage
  (`set_default_store` dans `src/lib.rs`) : `keyring-dpapi-store` (sous-crate maison, fichier
  chiffré DPAPI `Scope::User` — lié au compte Windows courant) sur Windows,
  `apple-native-keyring-store` (Keychain) sur macOS, `zbus-secret-service-keyring-store`
  (Secret Service) sur Linux. Deux mécanismes testés et abandonnés avant le store DPAPI
  maison : `keyring` (Windows Credential Manager) plafonne un mot de passe générique à
  ~2560 caractères UTF-16, dépassé par certains couples access+refresh token (Dropbox) ;
  `tauri-plugin-stronghold` ouvrait/committait le vault en 60s+ et ne relisait pas fiablement
  ce qui venait d'être écrit. DPAPI n'a ni plafond de taille ni gestion de clé séparée (le
  compte Windows fait office de clé), et le round-trip est quasi instantané — d'où le store
  maison plutôt que Credential Manager, réservé à Windows où keyring-core n'a pas
  d'équivalent natif sans plafond. macOS et Linux utilisent leurs stores keyring-core
  standards (Keychain, Secret Service), sans cette contrainte de taille connue.
- Client API pCloud, endpoint EU (`eapi.pcloud.com`) : listing de dossiers (catalogue),
  `getfilelink` / `getvideolink`.
- Handler du protocole `stream://` (Range + fallback transcodé).
- Persistance du manifeste local (§9).

**TS (frontend) :**
- Navigation catalogue (méthode → leçons dans l'ordre).
- Lecteur vidéo (`<video>` pointant sur `stream://`).
- Vue synchro (AlphaTab + `<audio>` + boucle de synchro, §7).
- Viewer documents (PDF.js + viewer image, §10).
- **Aucun secret, aucun appel réseau direct vers pCloud** : uniquement des `invoke()` vers Rust.

## 6. Import — scan à règles locales (PAS d'appariement)

La structure pCloud est de **profondeur variable** et hétérogène d'une méthode à l'autre
(parties rares, backing tracks tantôt au niveau épisode tantôt en partie, parfois plusieurs
vidéos par dossier). **Ne pas modéliser une hiérarchie figée.** Appliquer des règles locales :

- **Dossiers spéciaux par mot-clé** (insensible à la casse, à n'importe quel niveau) :
  - `archive*` → ignoré
  - `document*` → bucket documents (§10)
  - `backing track*` → bucket backing tracks
  - `tab*` → bucket tabs
- **Tout autre dossier** = nœud de structure, ordonné par son numéro de tête en **tri naturel**,
  parcouru en **profondeur d'abord (DFS)**.
- **Les vidéos définissent les « leçons »** (ce qu'on regarde dans l'ordre). L'ordre de
  visionnage = parcours DFS trié naturellement. Pas besoin de savoir si un dossier « est »
  un chapitre, un épisode ou une partie.

**Pas d'appariement tab ↔ backing track à l'import.** L'appariement par similarité de nom est
mort (ex. un tab `Trouvons des idées sombres.gp` coexiste avec des tracks `partie distorsion`,
aucun rapport de nom). À la place, on rattache à chaque leçon le **pool** de tabs + backing
tracks de son dossier et de ses descendants (les backing tracks d'un niveau partagé
« redescendent » vers les vidéos en dessous). Le choix final (quel tab, quel tempo) devient
une **action runtime dans l'UI** (l'utilisateur clique le tab puis le tempo).

> **Compromis assumé : on sur-rattache plutôt que de sous-rattacher.** Pire cas, un sélecteur
> affiche un track qui n'est pas strictement le sien. C'est préféré à un appariement manuel
> forcé. La majorité des dossiers n'ont qu'un seul tab → trivialement correct la plupart du temps.
>
> **Réserve (heuristique de repli si le sélecteur devient trop bruyant à l'usage) :**
> appariement par sous-dossier (tab unique du dossier → tous les tracks ; sinon scoper par
> sous-dossier). À garder en tête, non implémenté au départ.

**Parsing BPM :** le tempo est dans le nom du fichier, ex. `Backing track partie distorsion (120bpm).wav`.
Le parser (`(NNNbpm)`) sert à (a) étiqueter les boutons de tempo dans l'UI, (b) calculer le
`leadInMs` automatiquement (§7).

**Groupes de backing tracks par radical :** regrouper par nom moins le `(NNNbpm)`. Ex. donne
`partie distorsion` {100, 120, 150} et `partie distorsion 2 façon riff` {100, 115, 130, 160}.

**Détection du PDF d'export :** un `.pdf` qui partage son nom avec un `.gp`/`.gpx` adjacent est
un export de tablature → **ignoré partout**. Un PDF/image dans un dossier `document*` (ou isolé)
est un vrai document → viewer (§10).

## 7. Synchronisation — audio horloge maître

**Modèle mental : l'audio est l'horloge maître, AlphaTab est piloté (esclave).** On ne demande
jamais à AlphaTab de « jouer ». On lit la position du `.wav` et on la pousse à AlphaTab.

Caractéristiques des backing tracks (confirmées) : **tempo constant** par fichier (plusieurs
versions à des BPM différents pour l'apprentissage), **count-in quasi systématique d'une mesure**,
**silence de tête de durée variable** avant le count-in (l'hypothèse initiale d'un count-in
démarrant à `audioMs = 0` ne tenait pas en pratique).

**`leadInMs` dérivé automatiquement** (zéro réglage manuel dans le cas nominal) :

```
leadInMs = silenceMs + countInBars × beatsPerBar × 60000 / trackBpm
```

- `silenceMs` = fin du silence de tête, détectée une seule fois par track par une analyse
  d'amplitude (`src/lib/silence-detection.ts` — seuil sur le pic par fenêtre de 20 ms, calibré
  sur un percentile bas des premières fenêtres). Nécessite un buffer décodé
  (`AudioContext.decodeAudioData` sur un téléchargement complet séparé, cf. §4) — seul endroit du
  code qui décode un backing track entier ; la lecture elle-même ne s'en sert pas. Résultat mis
  en cache dans `leadInMsOverride` (persistance Rust), jamais recalculé une fois posé.
- `trackBpm` vient du nom de fichier ; absent/à `0` → repli sur le tempo noté du `.gp`
  (`notatedBpm`) plutôt que de diviser par zéro.
- `beatsPerBar` vient de la signature rythmique fournie par AlphaTab.
- `countInBars` = réglage **par méthode** (`defaultCountInBars`, défaut `1`). Presque toujours
  constant à l'échelle d'une méthode → si une méthode a un count-in de 2 mesures, on change
  *un* nombre une fois. Override par track seulement pour les rares exceptions.
- `leadInMsOverride` réglé manuellement garde toujours la priorité sur la détection auto.

**Boucle de synchro (TS), pilotée depuis la balise `<audio>`** — pas de `requestAnimationFrame`
ni d'`AudioContext` : `audio.currentTime` est la seule source de vérité, relue à chaque évènement
`timeupdate`/`seeked` du `<audio>` et, pendant la lecture, par un `setInterval` de 50 ms (filet de
sécurité, la fréquence native de `timeupdate` n'étant garantie par aucune spec) :

```
audioMs = audio.currentTime * 1000

// audioMs < silenceMs         : silence de tête, rien à afficher
// silenceMs ≤ audioMs < leadInMs : count-in, overlay avec le numéro du temps
// audioMs ≥ leadInMs          : morceau démarré, curseur poussé à AlphaTab

scale = trackBpm / notatedBpm      // = 1 si le backing track est au tempo noté du .gp
output.updatePosition((audioMs - leadInMs) * scale)
```

`updatePosition(ms)` fait convertir en interne par AlphaTab ce temps en position selon le tempo
**noté** du `.gp` — d'où la mise à l'échelle par `scale` : un temps écoulé au tempo du backing
track doit être réexprimé comme s'il s'était écoulé au tempo noté pour tomber sur le bon tick.
Reste correct tant que le tempo est constant sur tout le fichier (hypothèse posée plus haut —
pas de tempo variable interne, cf. `syncPoints` abandonné en faveur de cette approche plus
simple).

**Branchement AlphaTab :** mode média externe (`PlayerMode.EnabledExternalMedia`), la balise
`<audio>` branchée comme `IExternalMediaHandler` — `play`/`pause`/`volume`/`playbackRate`
délégués directement à l'élément DOM. `seekTo(time)` applique la transformation inverse :
`audio.currentTime = (time / scale + leadInMs) / 1000`. **⚠️ Les signatures exactes d'AlphaTab
ont changé selon les versions — vérifier l'API contre la version réellement installée plutôt que
de se fier à la mémoire.** Le modèle mental ci-dessus, lui, ne bouge pas.

**Fallbacks :**
- Track sans BPM dans le nom → repli sur le tempo noté du `.gp`, pas d'interpolation par durée.
- Détection du silence de tête en échec ou pas encore terminée (track jamais joué avant) →
  `leadInMs` traité comme non défini pour la session ; pas de count-in affiché tant que la
  détection (fire-and-forget) n'a pas abouti.

## 8. Modèle de données

```ts
interface Method {
  id: string;
  title: string;
  source: { provider: 'pcloud' | 'gdrive' | 'dropbox'; rootFolderId: string };
  defaultCountInBars: number;     // 1 par défaut, réglage unique par méthode
  lessons: Lesson[];              // à plat, ordre de visionnage (tri naturel DFS)
  documents: DocumentRef[];       // bucket DOCUMENTS UTILES
}

interface Lesson {
  id: string;
  order: number;
  title: string;                  // dérivé du chemin
  videos: FileRef[];              // souvent 1, parfois plusieurs
  tabs: TabSet[];                 // pool rattaché (pas appairé)
  backingGroups: BackingGroup[];  // pool rattaché, groupé par radical
}

interface TabSet {
  id: string;
  title: string;                  // radical commun
  gp?: FileRef;                   // priorité au .gp
  gpx?: FileRef;                  // sinon .gpx
  // pdf d'export volontairement absent : ignoré
}

interface BackingGroup {
  label: string;                  // ex. "partie distorsion"
  tracks: BackingTrack[];         // triés par bpm
}

interface BackingTrack {
  audio: FileRef;
  bpm: number;                    // parsé depuis "(NNNbpm)"
  leadInMsOverride?: number;      // détecté auto (silence de tête + count-in) ou réglé à la main
}

interface DocumentRef { file: FileRef; kind: 'pdf' | 'image'; title: string; }
interface FileRef { fileId: string; name: string; }
```

## 9. Persistance

- **Contenu (pCloud) = lecture seule.** Métadonnées (local, éditables) = à part.
- **Manifeste local par méthode** (JSON dans l'app dir Tauri), bootstrapé par le scan §6 puis
  éditable. JSON tant que ça reste quelques méthodes ; migrer vers SQLite seulement si besoin
  de requêter.
- Le manifeste porte tout ce qui ne peut pas vivre sur pCloud : `leadIn`, ordre, titres
  corrigés, `defaultCountInBars`, overrides éventuels.

## 10. Section documents (séparée de la synchro)

Brique indépendante du lecteur synchro. PDF.js pour les PDF, `<img>` / petit viewer (zoom/pan)
pour les images. Aucune contrainte de synchro, juste de l'affichage depuis un flux pCloud
(via `stream://`). Alimentée par le bucket `document*` du scan.

## 11. Vidéo

Tout est en **MP4 1080p**. H.264/AAC attendu (cas ultra-probable, contenu de cours 2022) → lu
nativement par WebView2. Le seul format qui coincerait est **HEVC/H.265** (WebView2 ne le décode
que si le support HEVC est installé sur la machine). On ne sonde pas : le fallback `getvideolink`
(§4) rattrape automatiquement un éventuel échec de décodage.

> Optimisation prématurée (ne pas faire tant que tout est H.264) : parser le fourcc dans la box
> `stsd` du MP4 via une Range request de quelques Ko côté Rust pour éviter le bref échec initial.

## 12. Actions manuelles résiduelles (acceptées, faites paresseusement)

- Réordonner une leçon mal triée (le cas `épisode 0` vs `épisode 00` est indécidable depuis
  les noms seuls).
- Ajuster `defaultCountInBars` d'une méthode si son count-in n'est pas d'une mesure.
- Caler un track récalcitrant (override manuel de `leadInMsOverride`).

## 13. À vérifier à l'implémentation

- **Signatures AlphaTab** (`PlayerMode.EnabledExternalMedia`, `IExternalMediaHandler`) contre la
  version installée — l'API a bougé entre versions.
- **API protocole custom Tauri v2** (`register_asynchronous_uri_scheme_protocol` et son support
  des Range) contre la version de Tauri installée.
- Codec vidéo réel (seulement si un échec de décodage survient — sinon non pertinent).

## 15. Stratégie de tests

Deux couches, jamais mélangées :

**Tests unitaires** — rapides, zéro dépendance système (pas de Tauri, gtk, WebKit).
Vivent dans les sous-crates sans dep Tauri : `chelou-manifest` et `chelou-providers` (ce
dernier regroupe la logique de tous les providers cloud — pCloud aujourd'hui — en modules,
aucun n'ayant de dépendance système qui justifierait un crate séparé).
Lancés dans la CI sur `ubuntu-latest` sans installation d'libs système :

```
cargo test -p chelou-manifest -p chelou-providers
```

Un troisième sous-crate, `keyring-dpapi-store` (§5), partage la même propriété de zéro
dépendance Tauri, mais ses tests sont spécifiques à Windows (`#![cfg(windows)]`) — la CI tourne
sur `ubuntu-latest`, donc ils n'y sont pas encore câblés. Ils passent (`cargo test -p
keyring-dpapi-store`) mais restent, pour l'instant, lancés localement uniquement ; la commande
canonique ci-dessus ne les inclut pas.

Règle d'or : **toute logique testable unitairement doit vivre dans un sous-crate.**
Si une fonction a besoin de Tauri pour être compilée, elle ne peut pas être testée unitairement.
La refacto vers un sous-crate est le bon mouvement, pas le mock de Tauri.

**Tests d'intégration** — aucun pour l'instant. Quand ils existent, ils dépendent de la stack
complète (Tauri + WebView2) et sont donc lents et liés à la plateforme. Ils valident un
comportement de bout en bout, pas de la logique isolée.

**Corollaire architectural :** la séparation en sous-crates (`chelou-manifest`, `chelou-providers`)
n'est pas qu'une commodité d'organisation — c'est ce qui rend les tests unitaires possibles.
Tout nouveau module avec de la logique non-triviale devrait idéalement rejoindre un sous-crate.

## 16. Décidé contre (ne pas réintroduire sans raison)

- Electron.
- Appli purement navigateur (impossible, cf. §3).
- Appariement tab ↔ backing track par similarité de nom.
- Affichage du PDF dans la vue synchro.
- Lecture du backing track via Web Audio API pure (`AudioContext` + `AudioBufferSourceNode` +
  `decodeAudioData`, boucle `requestAnimationFrame`, `tickPosition`). Remplacé par une balise
  `<audio>` en streaming (§2, §7) : l'argument initial de précision ne s'est pas vérifié
  nécessaire (`audio.currentTime` suffit), et exiger un téléchargement complet avant lecture
  contredit l'objectif §1 « streaming sans téléchargement ». `decodeAudioData` reste utilisé,
  mais seulement pour l'analyse ponctuelle du silence de tête (§7), pas pour la lecture.
- Mocks de Tauri dans les tests : on extrait la logique dans un sous-crate à la place (cf. §15).