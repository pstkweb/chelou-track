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
- **Audio : Web Audio API** (`AudioContext`, `decodeAudioData`) — pas la balise `<audio>`,
  pour la précision de synchro.

Electron a été explicitement écarté.

## 3. Contraintes dures (load-bearing — ne jamais violer)

1. **Le WebView ne voit jamais le token pCloud ni une URL pCloud brute.** Tout passe par
   le backend Rust. C'est à la fois une exigence de sécurité et une exigence fonctionnelle
   (voir contrainte 2).
2. **Les liens pCloud sont liés à l'IP qui les génère** et leur referer est restreint à
   `pcloud.com` (une appli web qui les consomme directement reçoit `Invalid link referer`
   + erreurs CORS). Conséquence : c'est **Rust** qui génère ET consomme le lien, sur la même
   machine donc la même IP. Une appli purement navigateur est impossible — d'où le shell natif.
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

Le `.wav` : chargé **en RAM** via `decodeAudioData` (≈30 Mo pour 3 min stéréo, négligeable),
pas streamé via `<audio>`, pour la précision de synchro. Le `.gp`/`.gpx` est minuscule,
chargé en `ArrayBuffer`.

## 5. Frontière Rust / TS

Règle : **token pCloud + octets → Rust. Rendu + synchro → TS.**

**Rust (backend Tauri) :**
- Auth pCloud + stockage sécurisé du token (keychain OS via `keyring`, ou
  `tauri-plugin-stronghold` chiffré — **jamais** en clair dans un fichier).
- Client API pCloud, endpoint EU (`eapi.pcloud.com`) : listing de dossiers (catalogue),
  `getfilelink` / `getvideolink`.
- Handler du protocole `stream://` (Range + fallback transcodé).
- Persistance du manifeste local (§9).

**TS (frontend) :**
- Navigation catalogue (méthode → leçons dans l'ordre).
- Lecteur vidéo (`<video>` pointant sur `stream://`).
- Vue synchro (AlphaTab + Web Audio + boucle de synchro, §7).
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
versions à des BPM différents pour l'apprentissage), **count-in quasi systématique d'une mesure**.

**`leadInMs` dérivé automatiquement** (zéro réglage manuel dans le cas nominal) :

```
leadInMs = countInBars × beatsPerBar × 60000 / trackBpm
```

- `trackBpm` vient du nom de fichier.
- `beatsPerBar` vient de la signature rythmique fournie par AlphaTab.
- `countInBars` = réglage **par méthode** (`defaultCountInBars`, défaut `1`). Presque toujours
  constant à l'échelle d'une méthode → si une méthode a un count-in de 2 mesures, on change
  *un* nombre une fois. Override par track seulement pour les rares exceptions.

**Boucle de synchro (TS), pilotée en beats absolus** (plus robuste que l'interpolation par
durée, qui dépend des silences de fin du wav) :

```
// au démarrage : t0 = audioCtx.currentTime, lancement de l'AudioBufferSourceNode
// dans une boucle requestAnimationFrame :
audioMs      = (audioCtx.currentTime - t0) * 1000
beatsEcoules = max(0, (audioMs - leadInMs) / 60000 * trackBpm)
tick         = beatsEcoules * ticksParNoire        // 4/4
// -> positionner le curseur AlphaTab à ce tick
```

Pendant le count-in (`audioMs < leadInMs`), `beatsEcoules` clampé à 0 : curseur masqué ou figé
sur la première note. **Le tempo noté du `.gp` n'intervient pas** : la version lente n'est
qu'un `trackBpm` plus petit ⇒ curseur plus lent ⇒ chaque mesure atteinte pile au bon instant audio.

**Branchement AlphaTab :** mode média externe (`PlayerMode.EnabledExternalMedia` + implémentation
de `IExternalMediaHandler` à qui on pousse la position). **⚠️ Les signatures exactes d'AlphaTab
ont changé selon les versions — vérifier l'API contre la version réellement installée plutôt que
de se fier à la mémoire.** Le modèle mental ci-dessus, lui, ne bouge pas.

**Fallbacks :**
- Track sans BPM dans le nom → interpolation par durée (deux ancres : `leadInMs` → début,
  `duréeWav` → fin).

## 8. Modèle de données

```ts
interface Method {
  id: string;
  title: string;
  source: { provider: 'pcloud'; rootFolderId: number };
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
interface FileRef { fileId: number; name: string; }
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
Vivent dans les sous-crates sans dep Tauri : `chelou-manifest` et `chelou-pcloud`.
Lancés dans la CI sur `ubuntu-latest` sans installation d'libs système :

```
cargo test -p chelou-manifest -p chelou-pcloud
```

Règle d'or : **toute logique testable unitairement doit vivre dans un sous-crate.**
Si une fonction a besoin de Tauri pour être compilée, elle ne peut pas être testée unitairement.
La refacto vers un sous-crate est le bon mouvement, pas le mock de Tauri.

**Tests d'intégration** — aucun pour l'instant. Quand ils existent, ils dépendent de la stack
complète (Tauri + WebView2) et sont donc lents et liés à la plateforme. Ils valident un
comportement de bout en bout, pas de la logique isolée.

**Corollaire architectural :** la séparation en sous-crates (`chelou-manifest`, `chelou-pcloud`)
n'est pas qu'une commodité d'organisation — c'est ce qui rend les tests unitaires possibles.
Tout nouveau module avec de la logique non-triviale devrait idéalement rejoindre un sous-crate.

## 16. Décidé contre (ne pas réintroduire sans raison)

- Electron.
- Appli purement navigateur (impossible, cf. §3).
- Appariement tab ↔ backing track par similarité de nom.
- Détection d'onset/silence sur le wav pour trouver le count-in (peu fiable ; le calcul dérivé
  du BPM est plus sûr). Gardé en réserve théorique uniquement.
- Affichage du PDF dans la vue synchro.
- Mocks de Tauri dans les tests : on extrait la logique dans un sous-crate à la place (cf. §15).