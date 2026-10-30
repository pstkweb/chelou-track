# Architecture — Lecteur de méthodes guitare (Tauri)

> Source de vérité du projet. Toute décision structurelle doit être cohérente avec
> ce document. Les choix ci-dessous ont été tranchés volontairement ; ne pas les
> re-dériver ni les contredire sans raison explicite.

## 1. Objectif

Application desktop Windows pour suivre des méthodes de guitare achetées (vidéos +
exercices) stockées sur pCloud. Lecture des vidéos dans l'ordre du cursus, et vue
ergonomique synchronisant un **backing track audio** avec la **tablature GuitarPro**
correspondante. Objectif transverse : **lecture en streaming sans téléchargement** et
**le moins d'actions manuelles possible** avant de pouvoir utiliser l'outil. Exception
assumée : le backing track de la vue synchro est téléchargé et décodé en entier avant d'être
joué (§7) — un ralentissement du provider ne doit jamais devenir une coupure audio.

## 2. Stack technique

- **Shell desktop : Tauri v2** (backend Rust + WebView2 système sur Windows).
- **Frontend : TypeScript.**
- **Rendu + synchro tablature : AlphaTab** (version JS/web). Seul moteur sérieux pour
  afficher *et* faire défiler un curseur sur des fichiers GuitarPro. Gère `.gp3/.gp4/.gp5`,
  `.gpx` (GP6) et `.gp` (GP7+).
- **PDF : PDF.js** (uniquement pour la section « documents », voir §10).
- **Audio : moteur natif Rust** (`cpal`, sous-crates `chelou-audio-core` et
  `chelou-audio-engine`, §7) — il possède la sortie audio du système et joue le backing track,
  téléchargé puis décodé en entier (`symphonia`) et rééchantillonné (`rubato`) à la fréquence de
  la sortie. Le track est lu à vitesse 1 : c'est lui qui donne le tempo. Une fois une carte son
  choisie, le moteur l'ouvre en **duplex** (ASIO ou WASAPI sur Windows, CoreAudio, ALSA) et
  mixe la guitare — passée dans **un plugin d'ampli VST3** choisi par l'utilisateur, sinon en
  son direct — avec le backing track. Le métronome reste pour l'instant un `AudioContext` côté
  WebView, déclenché par la position du moteur.
- **Hôte VST3 : `chelou-vst3-host`** — bindings `vst3` (+ classes COM `com-scrape-types`) et
  `libloading`, sans SDK C++ ni CMake (voie A du spike de dérisquage, §7).

Electron a été explicitement écarté.

## 3. Contraintes dures (load-bearing — ne jamais violer)

1. **Le WebView ne voit jamais le token ni une URL brute du provider de stockage.** Tout passe
   par le backend Rust. C'est une exigence de sécurité **appliquée uniformément à tous les
   providers** (pCloud, Dropbox et Google Drive aujourd'hui, cf. §14) — pas seulement une
   conséquence de la contrainte 2 ci-dessous. L'IP-binding pCloud (contrainte 2) est une raison
   suffisante pour pCloud, mais ce n'est pas *la* raison de la règle : même un provider sans
   cette contrainte technique doit passer par le même chemin, par cohérence et pour garder le
   token hors du WebView dans tous les cas. Dropbox n'a pas d'IP-binding sur son lien temporaire
   mais en a un ; **Google Drive n'a même pas de lien temporaire du tout** — `alt=media` exige un
   header `Authorization` à chaque requête, cf. §14.
2. **Les liens pCloud sont liés à l'IP qui les génère** et leur referer est restreint à
   `pcloud.com` (une appli web qui les consomme directement reçoit `Invalid link referer`
   + erreurs CORS). Conséquence : c'est **Rust** qui génère ET consomme le lien, sur la même
   machine donc la même IP. Une appli purement navigateur est impossible — d'où le shell natif.
   **Spécifique à pCloud** — ne pas supposer que ça généralise aux autres providers.
3. **Endpoint EU obligatoire : `eapi.pcloud.com`** (utilisateur en France). Utiliser
   `api.pcloud.com` (US) donne des liens sous-optimaux voire invalides.
4. **AlphaTab est l'unique moteur de tablature.** Ne pas tenter d'afficher le PDF dans la
   vue synchro : un PDF est figé, sans notion de position musicale.

Corollaires de la contrainte 1 :
- **Erreurs :** le `Display` d'une erreur reqwest contient l'URL de la requête — pour pCloud/
  Dropbox, un lien pré-signé qui vaut un token. Toute erreur qui sort vers le WebView (commandes
  `invoke()`, corps des réponses `stream://` et du serveur média Linux) passe par
  `chelou_providers::redact_urls`, et les requêtes de téléchargement utilisent
  `reqwest::Error::without_url`.
- **CSP** (`tauri.conf.json`) : `connect-src`/`media-src`/`img-src` limités à `'self'`, à l'IPC
  Tauri, à `stream:` et au serveur média loopback — une XSS ne peut rien exfiltrer ailleurs.
  Le CSP n'est pas appliqué en `tauri dev` desktop (la page vient directement de Vite) : le
  tester sur un build.

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

**Exception Linux — serveur loopback (`src-tauri/src/media_server.rs`).** Le backend média
GStreamer de WebKitGTK ne sait pas charger `<video>`/`<audio>` depuis un schéma custom (bug
WebKit non corrigé, cf. en-tête du module). Sur Linux uniquement, vidéo et audio passent donc
par `http://127.0.0.1:47812`. Un port loopback étant joignable par tout processus local **et
par n'importe quelle page web ouverte dans le navigateur**, deux gardes sont obligatoires :
chaque requête porte `?t=<jeton>` (128 bits aléatoires par session, servi au WebView par la
commande `get_media_token`, 403 sinon), et le CORS n'est accordé qu'à l'origine de l'app
(`tauri://localhost`, plus `http://localhost:1420` en debug), jamais `*`.

Le backing track (`.wav`/`.mp3`/`.flac`) ne passe plus par `stream://` : la commande
`backing_load` le télécharge en entier côté Rust (même `resolve_media`, sans Range) pour le
moteur audio natif (§7). La route `stream://audio` reste servie mais n'a plus d'appelant côté
WebView. Le `.gp`/`.gpx` est minuscule, chargé en `ArrayBuffer`.

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
- Flux OAuth loopback (RFC 8252) avec `state` anti-CSRF vérifié au callback et PKCE S256
  (Dropbox, Google ; pCloud ne documente que `state`). Les secrets clients restent compilés
  dans le binaire — inévitable pour une app desktop (« client public »), assumé.
- Client API pCloud, endpoint EU (`eapi.pcloud.com`) : listing de dossiers (catalogue),
  `getfilelink` / `getvideolink`.
- Handler du protocole `stream://` (Range + fallback transcodé).
- Persistance du manifeste local (§9).
- Moteur audio natif (§7) : sortie audio, lecture du backing track, détection du silence de
  tête, statut de lecture poussé au WebView ; carte son en duplex (son direct de la guitare),
  inventaire des cartes, réglages audio persistés (§9), statut de la carte et crêtes des
  vumètres poussés au WebView.
- Hôte VST3 (§7) : scan des plugins, plugin d'ampli de la voie guitare, sa fenêtre d'interface
  native. Le WebView désigne un plugin par son identifiant dans le dernier scan, jamais par un
  chemin.

**TS (frontend) :**
- Navigation catalogue (méthode → leçons dans l'ordre).
- Lecteur vidéo (`<video>` pointant sur `stream://`).
- Vue synchro (AlphaTab piloté par la position du moteur natif + boucle de synchro, §7).
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
jamais à AlphaTab de « jouer ». On lit la position du backing track et on la pousse à AlphaTab.

**L'horloge est le moteur audio natif** (`chelou-audio-engine`), pas le WebView : il possède la
sortie audio du système (mode « sortie seule » : périphérique par défaut, WASAPI partagé sur
Windows, aucune configuration), et sa tête de lecture (compteur d'échantillons du
`BackingPlayer`, `chelou-audio-core`) est la seule source de vérité. Le track est entièrement
téléchargé, décodé et rééchantillonné à la fréquence de la sortie avant de pouvoir jouer
(commande `backing_load`). Le thread audio ne fait que du calcul : ni allocation, ni verrou
bloquant, ni E/S, ni libération (un track remplacé repart par une file lock-free pour être
libéré ailleurs). Si la sortie disparaît, le moteur retente chaque seconde d'ouvrir la nouvelle
sortie par défaut ; si sa fréquence a changé, la vue recharge le track.

**Mode duplex (carte son choisie dans les réglages audio).** Une seule carte pour l'entrée et la
sortie : flux d'entrée + flux de sortie reliés par un ring lock-free mono (`rtrb`, sans
pré-remplissage — en ASIO, l'entrée est servie juste avant la sortie dans le même cycle). Le
callback de sortie mixe la voie guitare (`GuitarLane` : canal d'entrée choisi, dupliqué en
stéréo, gain en rampe linéaire sans clic, crêtes pour les vumètres) puis le `BackingPlayer`, sur
la paire de sorties choisie. Pilotes : **ASIO** (recommandé, fréquence et buffer réglables) et
**WASAPI** partagé (fréquence imposée par Windows, buffer système) sur Windows, CoreAudio sur
macOS, ALSA sur Linux. Les coupures (ring vide ou plein, callback plus long que son budget)
sont comptées ; les 100 ms qui suivent l'ouverture restent muettes et non comptées. Une
reconfiguration reconstruit les flux sur le thread moteur : à même fréquence, le track chargé
et sa position sont conservés, sinon la vue recharge le track. Échec d'ouverture (pilote ASIO
tenu par une autre application — cpal répond alors « Sample rate N Hz is not supported », traduit
en « Pilote audio déjà utilisé par une autre application ») ou carte perdue : repli en sortie
seule sur la sortie par défaut avec l'erreur dans le statut, la backing track continue ; pas de
réessai automatique de la carte (« Redémarrer » dans les réglages). Le statut de la carte
(`audio_status`, ~30 Hz, permanent) alimente le contrôle guitare de la barre de titre.

**Plugin d'ampli VST3 (un seul slot sur la voie guitare).** VST3 uniquement ; aucun plugin =
son direct. Le plugin est **chargé, réglé et déchargé sur le thread principal** (les plugins JUCE
— Neural DSP, Blob… — rattachent leur thread de messages au premier thread qui les appelle, et
leur interface est pompée par la boucle d'évènements de Tauri) ; seule sa partie temps réel
(trait `GuitarFx`) est installée dans la voie guitare (`Engine::install_fx`), où elle remplace le
son direct : entrée mono (repli stéréo si le plugin refuse le mono) → plugin → stéréo × gain.
Ordre imposé au retrait (déchargement, autre plugin, sortie de l'app) : partie temps réel
retirée de la voie, interface fermée, puis plugin libéré — sa DLL ne disparaît jamais sous le
callback. Le plugin est préparé à la fréquence réelle du flux et pour `MAX_BLOCK` (4096) frames,
puis **pré-chauffé** (quelques `process` sur du silence : le premier `process` d'Archetype prend
~10 ms) avant d'être branché. Flux rouverts à une autre fréquence : la voie retire l'effet
(flux fermés, il n'est jamais joué à la mauvaise fréquence) et l'app le recharge à la nouvelle,
avec ses réglages ; un plugin préparé pendant qu'une autre réouverture changeait la fréquence
est refusé par la voie et rechargé. Un chargement raté laisse l'ancien plugin en place ; un
plugin qui a fait planter l'app en se rechargeant au démarrage n'est pas rechargé au lancement
suivant (marqueur `pluginLoading`) (état composant + contrôleur, gardé en mémoire). Latence affichée = 2 ×
buffer + latence du plugin. **Scan** à la main uniquement : dossiers standard de l'OS + dossiers
ajoutés ; `Contents/Resources/moduleinfo.json` (JSON5) lu sans charger le binaire, sinon le
bundle est chargé dans un **processus enfant** (le binaire de l'app relancé avec
`--vst3-probe <bundle>`, tué au bout de 15 s) : un plugin qui plante pendant le scan ne touche
pas l'app. Seules les classes `Audio Module Class` sont listées, instruments purs exclus.
Plugin mémorisé introuvable au démarrage : son direct et bandeau, le choix reste mémorisé.
Plateformes : Windows complet ; Linux (`ModuleEntry`) et macOS (`bundleEntry`, **non testé**)
chargent et traitent le son, sans interface de plugin pour l'instant. Un plugin qui plante fait
tomber l'app (hébergement dans le processus, accepté).

Caractéristiques des backing tracks (confirmées) : **tempo constant** par fichier (plusieurs
versions à des BPM différents pour l'apprentissage), **count-in quasi systématique d'une mesure**,
**silence de tête de durée variable** avant le count-in (l'hypothèse initiale d'un count-in
démarrant à `audioMs = 0` ne tenait pas en pratique).

**`leadInMs` dérivé automatiquement** (zéro réglage manuel dans le cas nominal) :

```
leadInMs = silenceMs + countInBars × beatsPerBar × 60000 / trackBpm
```

- `silenceMs` = fin du silence de tête, détectée par une analyse d'amplitude
  (`chelou_audio_core::silence` — seuil sur le pic par fenêtre de 20 ms, calibré sur un
  percentile bas des premières fenêtres), faite par `backing_load` sur le track déjà décodé pour
  la lecture (aucun téléchargement supplémentaire) et renvoyée au WebView. Le `leadInMs` qui en
  découle est mis en cache dans `leadInMsOverride` (persistance Rust), jamais recalculé une
  fois posé.
- `trackBpm` vient du nom de fichier ; absent/à `0` → repli sur le tempo noté du `.gp`
  (`notatedBpm`) plutôt que de diviser par zéro.
- `beatsPerBar` vient de la signature rythmique fournie par AlphaTab.
- `countInBars` = réglage **par méthode** (`defaultCountInBars`, défaut `1`). Presque toujours
  constant à l'échelle d'une méthode → si une méthode a un count-in de 2 mesures, on change
  *un* nombre une fois. Override par track seulement pour les rares exceptions.
- `leadInMsOverride` réglé manuellement garde toujours la priorité sur la détection auto.

**Boucle de synchro (TS), pilotée par le statut du moteur** — la commande `backing_status`
pousse via un `tauri::ipc::Channel` la position, l'état de lecture et la fin du track, toutes
les 16 ms pendant la lecture et à chaque changement sinon (pause, seek, chargement, fin).
`useBackingTrackPlayback.onStatus` applique :

```
audioMs = status.positionMs

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

**Branchement AlphaTab :** mode média externe (`PlayerMode.EnabledExternalMedia`), un
`IExternalMediaHandler` qui délègue au moteur (`play`/`pause`/`masterVolume` → commandes
`backing_*`, `backingTrackDuration` = durée du track décodé, `playbackRate` fixé à 1).
`seekTo(time)` applique la transformation inverse : `backingSeek(time / scale + leadInMs)`. **⚠️ Les signatures exactes d'AlphaTab
ont changé selon les versions — vérifier l'API contre la version réellement installée plutôt que
de se fier à la mémoire.** Le modèle mental ci-dessus, lui, ne bouge pas.

**Fallbacks :**
- Track sans BPM dans le nom → repli sur le tempo noté du `.gp`, pas d'interpolation par durée.
- Aucun onset net trouvé, ou track pas encore chargé (jamais joué avant) → `leadInMs` traité
  comme non défini pour la session ; pas de count-in affiché tant que le chargement n'a pas
  abouti.

## 8. Modèle de données

```ts
interface Method {
  schemaVersion: number;          // version du format sur disque (SCHEMA_VERSION côté Rust)
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
- **Robustesse :** écriture atomique (fichier temporaire + `fsync` + `rename`), écritures
  sérialisées par un mutex (les mises à jour sont des read-modify-write concurrents), `id`
  validé (`[A-Za-z0-9_-]+`, il vient du WebView et sert de nom de fichier).
- **`schemaVersion`** : réécrit à `SCHEMA_VERSION` à chaque sauvegarde ; absent = 0 (fichiers
  d'avant le versioning). Un fichier illisible ou d'une version plus récente est ignoré au
  chargement (log stderr) **sans être modifié**, pour qu'une version plus récente de l'app
  puisse toujours le lire. Incrémenter `SCHEMA_VERSION` à tout changement cassant du format.
- **Réglages audio** (`audio/settings.json` dans l'app dir — pas à sa racine, où chaque `.json` est
  un manifeste de méthode ; un ancien `audio-settings.json` y est déplacé —, `chelou_audio_core::settings`) :
  carte choisie (pilote, entrée/sortie, canal, fréquence, buffer), carte arrêtée ou non, volume
  et mute de la guitare, plugin d'ampli choisi, dossiers VST3 ajoutés, dernier résultat de scan
  (champs ajoutés sans changer `schemaVersion` : absents → valeurs par défaut). Même politique que le manifeste : écriture atomique, `schemaVersion`,
  fichier absent, illisible ou plus récent → réglages par défaut (sortie seule) sans le modifier.

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

## 14. Multi-provider — pCloud / Dropbox / etc.

**État et roadmap des providers (implémentés, candidats, exclus, et pourquoi) : voir
`docs/PROVIDERS.md`.** Cette section-ci ne couvre que les détails techniques d'implémentation
des providers actifs.

Chaque provider implémente les traits `StorageProvider`/`ProviderAuth` (`chelou-providers`,
un module par provider : `pcloud.rs`, `dropbox.rs`, `gdrive.rs`). Deux points où Google Drive
force une abstraction plus large que ce que pCloud/Dropbox seuls auraient suggéré :

- **`resolve_download_url` renvoie un `DownloadTarget { url, headers }`, pas une simple
  `String`.** pCloud (`getfilelink`/`getvideolink`) et Dropbox (`get_temporary_link`) renvoient
  un lien temporaire pré-signé, consommable tel quel sans en-tête (`headers: vec![]`) — c'est ce
  lien que `fetch_range` (§4) suit avec les requêtes Range. **Google Drive n'a pas d'équivalent** :
  `GET /files/{fileId}?alt=media` est une URL fixe qui exige un header `Authorization: Bearer`
  sur *chaque* requête, y compris chaque chunk Range. `GDriveClient::resolve_download_url` ne
  fait donc aucun appel réseau — il construit l'URL depuis le seul `fileId` et attache le token
  courant dans `headers`.
- **Le cache d'URL (`url_cache` dans `stream/mod.rs`) est bypassé pour Google Drive.** Sa raison
  d'être est d'éviter un aller-retour réseau coûteux (`getfilelink`/`get_temporary_link`) répété à
  chaque chunk — round-trip que Google Drive ne fait pas (point précédent). Le cacher quand même
  risquerait de resservir, jusqu'à `URL_TTL` (10 min), un header `Authorization` déjà remplacé par
  `get_or_refresh_credentials` (appelé, lui, à chaque requête) : un token tourné mais pas encore
  expiré resterait invisible pour le provider tant que l'entrée de cache n'a pas expiré.

**Provider local (`local.rs`) — dossier sur la machine (disque, lecteur réseau, UNC, montage
`/mnt/…`).** Pas d'OAuth ni de token : « se connecter » = choisir un dossier racine via la boîte
de dialogue native (commande `connect_local`, `tauri-plugin-dialog` appelé côté Rust — aucune
permission dialog exposée au WebView). Le chemin racine occupe le slot `access_token` de
`StoredCredentials`, ce qui garde uniformes `AuthStore`, `get_or_create_client` (cache indexé par
token → changer de racine invalide le client) et `make_client` ; `make_auth(Local)` renvoie une
erreur. Trois choix structurants :

- **`DownloadTarget` est un enum `Http { url, headers } | File(PathBuf)`.** Un fichier local n'a
  pas d'URL ; `fetch_range` (et `serve_file` dans `media_server.rs`) lisent la plage demandée
  directement sur disque (`local::open_range` : 200/206/416 + `Content-Range`, `Content-Type`
  déduit de l'extension). Le serveur média Linux streame le fichier par blocs de 64 Ko — même
  contrat « jamais tout bufferiser » que le proxy HTTP.
- **Ids opaques relatifs à la racine** : base64url (sans padding) du chemin relatif séparé par
  `/`, racine = `''`. URL-safe dans `stream://…/{fileId}` (espaces, accents, `#`, `?` dans les
  noms), et chaque id est décodé segment par segment : tout ce qui n'est pas un simple nom (`..`,
  chemin absolu, préfixe de lecteur, `:`) est rejeté → aucune requête ne sort de la racine.
  Conséquence assumée : les méthodes importées dépendent de la racine choisie — se reconnecter
  sur une autre racine casse leurs ids (comme changer de compte cloud).
- **Pas de cache d'URL** (`is_cacheable() == false`, résoudre = joindre un chemin) **ni de
  version transcodée** (`transcoded` → erreur explicite).

Autres divergences, mineures, par provider :

- **Racine du scan (`rootId`, `src/lib/providers.ts`)** : pCloud `'0'` (id numérique racine de
  l'API), Dropbox `''` (chemin vide = racine dans l'API Dropbox), Google Drive `'root'` — alias
  documenté par l'API Drive utilisable directement dans une requête `'{id}' in parents`, pas une
  simple chaîne vide.
- **OAuth Google Drive** : client enregistré en **Desktop app** (Google Cloud Console) plutôt
  qu'en Web application — ce type autorise nativement le flow loopback (RFC 8252) sur n'importe
  quel port `localhost`, sans avoir à lister chaque port comme pour pCloud/Dropbox. Scope minimal
  `drive.readonly` : le scan (§6) parcourt une arborescence existante appartenant à l'utilisateur,
  ce que ne permet pas `drive.file` (limité aux fichiers créés par l'app ou choisis via un
  Google Picker — testé réellement, ne fonctionne pas pour ce cas d'usage). `drive.readonly`
  étant un scope restreint, Google exige un audit de sécurité payant annuel pour le valider en
  production — jugé disproportionné pour ce projet ; le provider est donc implémenté mais masqué
  du sélecteur (détails et alternatives : `docs/PROVIDERS.md`).
- **Racine locale** : `rootId` `''` = le dossier choisi dans `connect_local` (cf. ci-dessus).

## 15. Stratégie de tests

Deux couches, jamais mélangées :

**Tests unitaires** — rapides, zéro dépendance système (pas de Tauri, gtk, WebKit).
Vivent dans les sous-crates sans dep Tauri : `chelou-manifest`, `chelou-providers` (ce
dernier regroupe la logique de tous les providers cloud — pCloud aujourd'hui — en modules,
aucun n'ayant de dépendance système qui justifierait un crate séparé) et `chelou-audio-core`
(décodage, rééchantillonnage, lecteur temps réel, voie guitare, réglages audio, détection du
silence). L'E/S audio
(`chelou-audio-engine`, qui dépend de `cpal` donc d'ALSA sur Linux) n'a pas de tests
unitaires : toute sa logique testable est dans `chelou-audio-core`, y compris celle du scan VST3
(`plugins` : `moduleinfo.json`, filtre, bundles). L'hôte VST3 (`chelou-vst3-host`) ne se teste
pas sans vrais plugins ; seul son scan en processus enfant a des tests (`cargo test -p
chelou-vst3-host`, en local : ses exemples dépendent du moteur, donc d'ALSA).
Lancés dans la CI sur `ubuntu-latest` sans installation d'libs système :

```
cargo test -p chelou-manifest -p chelou-providers -p chelou-audio-core
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
  `decodeAudioData`, boucle `requestAnimationFrame`, `tickPosition`), puis via une balise
  `<audio>` en streaming. Les deux sont remplacées par le moteur audio natif (§7) : la guitare
  passée dans un plugin VST3 doit être mixée avec le backing track dans un même flux natif à
  faible latence, ce qu'aucun chemin WebView ne permet. Le téléchargement complet avant lecture,
  écarté à l'époque au nom de l'objectif §1, est désormais assumé : il garantit qu'un
  ralentissement du provider ne devient jamais une coupure audio.
- Mocks de Tauri dans les tests : on extrait la logique dans un sous-crate à la place (cf. §15).