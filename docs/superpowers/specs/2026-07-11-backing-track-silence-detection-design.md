# Détection du silence de tête des backing tracks — design

Date : 2026-07-11
Statut : validé, prêt pour plan d'implémentation

## 1. Problème

Le calcul de `leadInMs` documenté (`docs/ARCHITECTURE.md` §7) suppose que le count-in commence à
`audioMs = 0` :

```
leadInMs = countInBars × beatsPerBar × 60000 / trackBpm
```

En pratique, une partie significative des dizaines/centaines de fichiers `.wav` de backing track
ont un silence de durée variable avant le count-in batterie. La formule ci-dessus reste correcte
pour la *durée* du count-in (tempo constant et connu par fichier), mais fausse le *point de départ*
: le curseur de tablature démarre trop tôt.

Second constat fait pendant le brainstorming (pas lié au problème initial mais découvert en
inspectant `TabScreen.tsx`) : **il n'existe actuellement aucune compensation de lead-in dans le
code**. La lecture pousse `audio.currentTime * 1000` brut à AlphaTab via `updatePosition` dès la
première frame. Ce spec couvre donc à la fois la détection du silence et son branchement effectif.

## 2. Décision révisée (ARCHITECTURE §16)

ARCHITECTURE.md §16 écarte explicitement la « détection d'onset/silence sur le wav pour trouver le
count-in » comme peu fiable, au profit du calcul dérivé du BPM. Cette décision est **partiellement
révisée** ici, avec une justification explicite (silence de tête variable constaté sur les fichiers
réels) :

- Ce qui reste vrai et inchangé : la **durée** du count-in continue d'être dérivée du BPM (fiable,
  tempo constant par fichier).
- Ce qui change : on ajoute un point d'ancrage — l'instant où le silence de tête se termine — obtenu
  par une détection d'amplitude simple à un seul point (pas un pattern rythmique complet comme
  l'onset detection écartée en §16, qui visait à détecter *chaque* frappe du count-in).

Approche alternative (détection d'onset complète sur tout le count-in, plus précise mais plus
fragile — c'est celle explicitement écartée en §16) gardée en réserve si l'approche retenue s'avère
insuffisante en pratique. Non implémentée dans ce spec.

## 3. Modèle de données

Un champ ajouté à `BackingTrack`, miroir TS ↔ Rust :

```ts
// src/types/model.ts
export interface BackingTrack {
  audio: FileRef;
  bpm: number;
  leadInMsOverride?: number;
  detectedSilenceMs?: number; // NOUVEAU — cache du point d'ancrage détecté (ms)
  syncPoints?: SyncPoint[];
}
```

```rust
// src-tauri/crates/manifest/src/lib.rs
pub struct BackingTrack {
    pub audio: FileRef,
    pub bpm: u32,
    #[serde(rename = "leadInMsOverride", skip_serializing_if = "Option::is_none")]
    pub lead_in_ms_override: Option<f64>,
    #[serde(rename = "detectedSilenceMs", skip_serializing_if = "Option::is_none")]
    pub detected_silence_ms: Option<f64>,
    #[serde(rename = "syncPoints", skip_serializing_if = "Option::is_none")]
    pub sync_points: Option<Vec<SyncPoint>>,
}
```

Sémantique :
- Absent = jamais analysé (ou dernière analyse en échec) → traité comme `0` (comportement actuel).
- Présent = résultat mis en cache, réutilisé sans re-scanner.
- `leadInMsOverride` garde la priorité absolue quand défini — filet de secours manuel existant,
  aucun changement à cette règle.

Formule effective (quand pas d'override) :

```
leadInMs = (detectedSilenceMs ?? 0) + countInBars × beatsPerBar × 60000 / trackBpm
```

## 4. Algorithme de détection

**Entrée :** un `Float32Array` (canal 0) + `sampleRate` d'un `AudioBuffer` décodé.

**Fonction pure et isolée**, testable sans dépendance DOM/réseau :

```ts
// src/lib/silence-detection.ts
export const WINDOW_MS = 20;
export const NOISE_FLOOR_WINDOWS = 8;      // ~160ms pour mesurer le plancher de bruit
export const THRESHOLD_MULTIPLIER = 9;     // seuil = noiseFloor × ce facteur
export const ABSOLUTE_FLOOR_DB = -50;      // plancher absolu si noiseFloor ~ 0
export const SCAN_CAP_MS = 15_000;         // ne jamais chercher au-delà
export const ATTACK_MARGIN_MS = 10;        // recul pour ne pas couper l'attaque

export function scanLeadingSilence(
  samples: Float32Array,
  sampleRate: number,
): number | undefined { /* … */ }
```

**Étapes :**
1. Découper les `SCAN_CAP_MS` premières ms en fenêtres de `WINDOW_MS`.
2. Calculer le RMS des `NOISE_FLOOR_WINDOWS` premières fenêtres → `noiseFloor`.
3. `threshold = max(noiseFloor × THRESHOLD_MULTIPLIER, dbToLinear(ABSOLUTE_FLOOR_DB))`.
4. Parcourir les fenêtres suivantes ; retenir la première fenêtre dont le RMS dépasse `threshold`
   **et** dont la fenêtre suivante confirme (debounce anti-pic isolé).
5. Résultat = `index de fenêtre × WINDOW_MS − ATTACK_MARGIN_MS` (jamais négatif).

**Garde-fous (retournent `undefined`, jamais d'erreur bloquante) :**
- Aucune fenêtre ne dépasse le seuil dans la fenêtre de scan.
- Résultat aberrant (silence détecté > ~8s, ou incohérent avec un count-in plausible).

Dans tous les cas d'échec : fallback silencieux à `0` (comportement identique à aujourd'hui, jamais
pire).

## 5. Détection en production (wrapper I/O)

```ts
// src/lib/silence-detection.ts (suite)
export async function detectLeadingSilence(fileId: number): Promise<number | undefined> {
  const bytes = await fetch(audioUrl(fileId)).then((r) => r.arrayBuffer());
  const ctx = new AudioContext();
  try {
    const buffer = await ctx.decodeAudioData(bytes);
    return scanLeadingSilence(buffer.getChannelData(0), buffer.sampleRate);
  } finally {
    ctx.close();
  }
}
```

Coût accepté pour la v1 : un second téléchargement complet du fichier (~30 Mo), une seule fois par
track jamais joué avant (jamais répété ensuite, résultat mis en cache). Cohérent avec le précédent
déjà posé par ARCHITECTURE §11 (ne pas sur-optimiser tant que ce n'est pas un problème réel
constaté) — **à surveiller à l'usage**, la balise `<audio>` de `TabScreen.tsx` fait déjà son propre
streaming pour la lecture, indépendamment de ce fetch d'analyse.

## 6. Persistance (cache manifeste)

Même pattern que `update_lesson_resume` existant. `BackingTrack` vit dans l'arbre `items`
(potentiellement imbriqué dans des `Section`), donc besoin d'un helper récursif pour retrouver la
leçon (contrairement à `progress`, qui est une map plate).

```rust
// src-tauri/crates/manifest/src/lib.rs
impl ManifestStore {
    pub fn update_backing_track_silence(
        &self,
        method_id: &str,
        lesson_id: &str,
        file_id: u64,
        silence_ms: f64,
    ) -> Result<()> {
        self.update_method(method_id, |m| {
            if let Some(lesson) = find_lesson_mut(&mut m.items, lesson_id) {
                for group in &mut lesson.backing_groups {
                    for track in &mut group.tracks {
                        if track.audio.file_id == file_id {
                            track.detected_silence_ms = Some(silence_ms);
                        }
                    }
                }
            }
        })
    }
}

fn find_lesson_mut<'a>(items: &'a mut [SectionItem], lesson_id: &str) -> Option<&'a mut Lesson> {
    for item in items {
        match item {
            SectionItem::Lesson(l) if l.id == lesson_id => return Some(l),
            SectionItem::Section(s) => {
                if let Some(l) = find_lesson_mut(&mut s.items, lesson_id) {
                    return Some(l);
                }
            }
            _ => {}
        }
    }
    None
}
```

**IPC** (`commands/mod.rs` + `lib/ipc.ts`), même forme que `update_lesson_resume` :

```ts
// src/lib/ipc.ts
export async function updateBackingTrackSilence(
  methodId: string,
  lessonId: string,
  fileId: number,
  silenceMs: number,
): Promise<void> {
  await invoke('update_backing_track_silence', { methodId, lessonId, fileId, silenceMs });
}
```

## 7. Branchement production dans `TabScreen.tsx`

**`useAlphaTabPlayer.ts`** — capturer la signature rythmique (rien ne le fait actuellement) :

```ts
instance.scoreLoaded.on((score) => {
  beatsPerBarRef.current = score.masterBars[0]?.timeSignatureNumerator ?? 4;
});
```
Exposé au composant appelant via le retour du hook (ex. `{ alphaTabRef, beatsPerBarRef }`).

**`TabScreen.tsx`** :

```ts
const [detectedSilenceMs, setDetectedSilenceMs] = useState(backingTrackSpeed?.detectedSilenceMs);

useEffect(() => {
  setDetectedSilenceMs(backingTrackSpeed?.detectedSilenceMs);
  if (
    !backingTrackSpeed ||
    backingTrackSpeed.leadInMsOverride != null ||
    backingTrackSpeed.detectedSilenceMs != null
  ) {
    return;
  }
  detectLeadingSilence(backingTrackSpeed.audio.fileId).then((silenceMs) => {
    if (silenceMs === undefined) return; // échec détection, fallback silencieux à 0
    setDetectedSilenceMs(silenceMs);
    updateBackingTrackSilence(method.id, lesson.id, backingTrackSpeed.audio.fileId, silenceMs);
  });
}, [backingTrackSpeed]);

const leadInMs = backingTrackSpeed
  ? (backingTrackSpeed.leadInMsOverride ??
      (detectedSilenceMs ?? 0) +
        (method.defaultCountInBars * beatsPerBarRef.current * 60000) / backingTrackSpeed.bpm)
  : 0;

const onTimeUpdate = () => {
  const audio = audioElmt.current;
  const player = alphaTabRef.current?.player;
  if (!audio || !player) return;

  const audioMs = audio.currentTime * 1000;
  if (audioMs < leadInMs) return; // count-in : curseur figé sur sa position initiale

  (player.output as unknown as synth.IExternalMediaSynthOutput).updatePosition(audioMs - leadInMs);
};
```

Points notables :
- L'analyse démarre en tâche de fond dès qu'un track sans `detectedSilenceMs` connu est sélectionné,
  pas besoin d'attendre le clic play.
- Si l'utilisateur clique play avant la fin de l'analyse : `leadInMs` vaut `0 + count-in` jusqu'à
  résolution, léger ressaut possible mais rare en pratique.
- `leadInMsOverride` court-circuite tout, comme aujourd'hui.

## 8. Tests

Le projet n'a actuellement **aucun test runner JS** (`src/tests/manifest-contract.ts` n'est validé
que par `tsc --noEmit`). On introduit `vitest` pour ce besoin :

- `vitest` en devDependency.
- Script `"test": "vitest run"` dans `package.json`.
- Ajouté à `npm run ci` : `"ci": "biome ci . && tsc --noEmit && vitest run"`.

**`src/lib/silence-detection.test.ts`** — cible uniquement `scanLeadingSilence` (fonction pure),
avec des `Float32Array` synthétiques générés en mémoire :

1. Silence propre + onset net à un index connu → `silenceMs` proche de l'attendu (± marge).
2. Pas de silence du tout (signal dès l'échantillon 0) → `~0`.
3. Silence total sur toute la fenêtre de scan → `undefined`.
4. Pic isolé (clic numérique) avant le vrai onset → ignoré grâce au debounce, vrai onset détecté.
5. Léger bruit de fond constant avant l'onset → ne déclenche pas le seuil (seuil relatif adaptatif),
   seul l'onset réel déclenche.
6. Détection aberrante (silence > plafond plausible) → `undefined` via le garde-fou.

`detectLeadingSilence` (wrapper fetch + `decodeAudioData`) n'est pas testé unitairement — non
isolable sans mocker `fetch`/`AudioContext`, validé manuellement via la page de debug (§9).

## 9. Page de debug

Outil de validation visuelle et auditive de l'algorithme sur de vrais fichiers, construit comme
partie de cette première implémentation (pas un jetable post-hoc).

**Accès :** bouton dans `TabScreen.tsx`, à côté du sélecteur de backing track, actif quand un track
est sélectionné. Ouvre un nouvel écran pour ce track précis, réutilisant le contexte déjà chargé.

**Navigation** (`NavigationContext.tsx`) — nouveau variant de `Screen` :
```ts
| { id: 'debug-silence'; method: Method; lesson: Lesson; chapter: Chapter; tab: TabSet; track: BackingTrack }
```
+ action/reducer/breadcrumb correspondants, sur le modèle des écrans existants.

**Dépendance :** `wavesurfer.js` (lib core, pas de wrapper React tiers — on suit le pattern déjà
établi par `useAlphaTabPlayer.ts` : encapsuler une lib impérative dans un hook custom).

**`useWavesurfer.ts`** (nouveau hook, même structure que `useAlphaTabPlayer`) :
- Instancie `WaveSurfer.create({ container, url: audioUrl(fileId), plugins: [RegionsPlugin.create()] })`
  dans un `useEffect`.
- Sur l'event `ready` : récupère `wavesurfer.getDecodedData()` (évite un second fetch/decode en plus
  de celui de wavesurfer), calcule `scanLeadingSilence(...)` dessus.
- Expose `{ wavesurferRef, silenceMs, leadInMs }` au composant.

**`DebugSilenceScreen.tsx`** :
1. `<div ref={containerRef} />` pour le waveform (fichier complet — wavesurfer gère peaks/rendu).
2. Une fois `ready` + calcul fait : ajoute 3 régions via le plugin Regions, **sans `end`** (marqueurs
   ponctuels, suffisant pour un outil de debug) :
   - `silenceMs` — "Silence détecté"
   - `leadInMs` — "Début effectif (4/4 approx.)" — cette page ne charge pas AlphaTab/le score donc
     pas de vraie signature rythmique disponible ; `beatsPerBar` fixé à `4` pour l'affichage,
     clairement labellisé comme approximation (la vraie valeur utilise le vrai `beatsPerBar` dans
     `TabScreen.tsx`, §7).
   - fin de fenêtre de scan (`SCAN_CAP_MS`)
3. **Lecture** : contrôles play/pause natifs de wavesurfer (`wavesurfer.playPause()`).
4. **Bonus — écouter depuis le lead-in** : bouton dédié →
   `wavesurfer.setTime(leadInMs / 1000); wavesurfer.play()`.
5. Lecture numérique des valeurs (`silenceMs`, `leadInMs`, `bpm`, `defaultCountInBars`) à côté du
   waveform.

Isolé à cette page — aucun changement au chemin de lecture `<audio>` existant de `TabScreen.tsx`
(§7 inchangée).

## 10. Hors scope / réserves

- **Approche B (détection d'onset complète sur tout le count-in)** : gardée en réserve théorique si
  l'approche retenue s'avère insuffisante en pratique (cf. §2). Non implémentée.
- **Pas d'analyse batch à l'import de méthode** : l'analyse reste paresseuse, au premier
  visionnage/sélection d'un track (cf. décision prise pendant le brainstorming — télécharger et
  analyser des centaines de fichiers jamais écoutés contredirait l'objectif « streaming sans
  téléchargement »).
- **Pas de correction manuelle par glisser-déposer des marqueurs** dans la page de debug — les
  marqueurs sont en lecture seule, `leadInMsOverride` (existant) reste l'unique mécanisme de
  correction manuelle en production.
- **Documentation obsolète repérée pendant ce brainstorming, non traitée ici** : `CLAUDE.md` et
  `ARCHITECTURE.md` décrivent un `components/player/SyncView.ts` séparé utilisant
  `AudioContext`/`AudioBufferSourceNode` et `api.tickPosition`. L'implémentation réelle
  (`TabScreen.tsx`) utilise une balise `<audio>` native et `player.output.updatePosition(ms)` (choix
  documenté en commentaire dans le code : `tickPosition` déclenchait un vrai seek à chaque appel).
  Une mise à jour de ces docs pour refléter l'état réel est recommandée, en tâche séparée.

## 11. Résumé des fichiers touchés

| Fichier | Changement |
|---|---|
| `src/types/model.ts` | `+detectedSilenceMs?` sur `BackingTrack` |
| `src-tauri/crates/manifest/src/lib.rs` | `+detected_silence_ms`, `+update_backing_track_silence`, `+find_lesson_mut` |
| `src-tauri/src/commands/mod.rs` | `+update_backing_track_silence` command |
| `src/lib/ipc.ts` | `+updateBackingTrackSilence` |
| `src/lib/silence-detection.ts` (nouveau) | `scanLeadingSilence`, `detectLeadingSilence`, constantes |
| `src/lib/silence-detection.test.ts` (nouveau) | tests vitest |
| `src/hooks/useAlphaTabPlayer.ts` | capture `beatsPerBarRef` via `scoreLoaded` |
| `src/hooks/useWavesurfer.ts` (nouveau) | encapsule wavesurfer.js + Regions |
| `src/components/templates/TabScreen.tsx` | calcul `leadInMs`, garde dans `onTimeUpdate`, bouton debug |
| `src/components/templates/DebugSilenceScreen.tsx` (nouveau) | page de debug |
| `src/contexts/NavigationContext.tsx` | `+debug-silence` screen |
| `package.json` | `+vitest`, `+wavesurfer.js`, script `test`, `ci` mis à jour |
