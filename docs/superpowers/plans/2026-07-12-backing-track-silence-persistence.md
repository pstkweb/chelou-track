# Backing Track Lead-In Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the leading-silence detection actually usable: run it automatically from
`TabScreen.tsx` (not the throwaway debug screen) whenever a backing track is selected, and persist
the final computed lead-in directly into the existing `leadInMsOverride` field so it's cached
across sessions. Also remove `syncPoints`, an unused manual-override mechanism superseded by this
work.

**Architecture:** `TabScreen.tsx` already has everything `DebugSilenceScreen.tsx` needed to
approximate a lead-in (bpm, `defaultCountInBars`) plus one thing the debug screen didn't: the
*real* time signature from the loaded AlphaTab score. A new `beatsPerBarRef` exposed by
`useAlphaTabPlayer` (captured from `scoreLoaded`) replaces the debug screen's hardcoded 4/4
assumption. `TabScreen.tsx` runs `detectLeadingSilence` (a new fetch+decode wrapper around the
existing pure `analyzeLeadingSilence`) in the background when a track without a cached
`leadInMsOverride` is selected, computes the final value with the real `beatsPerBar`, and persists
it via a new Tauri command — mirroring the existing `update_lesson_resume` persistence pattern
exactly. `DebugSilenceScreen.tsx` is untouched by this plan (still just visualizes, never writes).

**Tech Stack:** Rust (`chelou-manifest` + `chelou-pcloud` sub-crates, main `src-tauri` crate),
TypeScript/React.

## Global Constraints

- **The debug screen (`DebugSilenceScreen.tsx`) must not be touched by this plan** — no
  persistence call there. It's temporary tooling; the real trigger point is `TabScreen.tsx`, which
  is what real usage actually exercises.
- **No change to `onTimeUpdate` / the playback sync loop in this plan** — this plan only makes
  `leadInMsOverride` get populated automatically. Actually consuming it to offset the AlphaTab
  cursor during playback is separate, not-yet-requested work.
- **Mirror discipline:** `crates/manifest/src/lib.rs` (Rust) and `types/model.ts` (TS) must stay in
  sync — camelCase in TS, `#[serde(rename = "...")]` snake_case in Rust (per CLAUDE.md
  "Conventions").
- **Testable logic lives in the sub-crate:** new persistence logic goes in `chelou-manifest`,
  tested via `cargo test -p chelou-manifest` (no Tauri/mocking) — per ARCHITECTURE.md §15. The
  Tauri command itself is a thin pass-through, verified with `cargo check` only.
- **`docs/ARCHITECTURE.md` is the source of truth for the data model** — it must be updated
  wherever it documents `syncPoints`, since that's a real, deliberate model change, not just an
  implementation detail.

---

### Task 1: Rust — remove `syncPoints`, add lead-in-override persistence + tests

**Files:**
- Modify: `src-tauri/crates/manifest/src/lib.rs`
- Modify: `src-tauri/crates/pcloud/src/scanner.rs`
- Modify: `docs/ARCHITECTURE.md`

**Interfaces:**
- Produces: `ManifestStore::update_backing_track_lead_in_override(&self, method_id: &str, lesson_id: &str, file_id: u64, lead_in_ms: f64) -> Result<()>`
  — consumed by Task 2's Tauri command.

- [ ] **Step 1: Remove `SyncPoint` and the `sync_points` field**

In `src-tauri/crates/manifest/src/lib.rs`:

Remove the `SyncPoint` struct entirely:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPoint {
    #[serde(rename = "audioMs")]
    pub audio_ms: f64,
    pub tick: u32,
}
```

Change `BackingTrack` to:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackingTrack {
    pub audio: FileRef,
    pub bpm: u32,
    /// Final lead-in offset (ms) used to anchor tablature sync to this track's real start.
    /// Populated automatically by TabScreen's background detection the first time a track
    /// is selected, or manually if a track needs correcting. Once set, nothing recomputes it.
    #[serde(rename = "leadInMsOverride", skip_serializing_if = "Option::is_none")]
    pub lead_in_ms_override: Option<f64>,
}
```

- [ ] **Step 2: Fix the now-broken construction site in `chelou-pcloud`**

In `src-tauri/crates/pcloud/src/scanner.rs`, in the `backing_track` function, remove the
`sync_points: None,` line so the struct literal matches the new shape:

```rust
fn backing_track(entry: &Entry) -> BackingTrack {
    BackingTrack {
        audio: FileRef {
            file_id: entry.fileid,
            name: entry.name.clone(),
        },
        bpm: parse_bpm(&entry.name),
        lead_in_ms_override: None,
    }
}
```

(Keep the surrounding fields as they already are — only the `sync_points` line is removed.)

- [ ] **Step 3: Update the `chelou-manifest` test fixture**

In `src-tauri/crates/manifest/src/lib.rs`, in `sample_lesson` (inside `#[cfg(test)] mod tests`),
remove the `sync_points: None,` line from the `BackingTrack` literal:

```rust
            backing_groups: vec![BackingGroup {
                label: "partie distorsion".into(),
                tracks: vec![BackingTrack {
                    audio: FileRef {
                        file_id: 3,
                        name: "Backing track partie distorsion (120bpm).wav".into(),
                    },
                    bpm: 120,
                    lead_in_ms_override: None,
                }],
            }],
```

- [ ] **Step 4: Update the serialization contract test**

In `serializes_camelcase_for_ts` (same file), remove this assertion (the field no longer exists):

```rust
        assert!(
            track0.get("syncPoints").is_none(),
            "must be absent when None"
        );
```

- [ ] **Step 5: Run tests to verify the removal compiles cleanly**

Run: `cd src-tauri && cargo test -p chelou-manifest -p chelou-pcloud`
Expected: PASS — confirms the field removal alone (before adding new behavior) doesn't break
anything in either crate.

- [ ] **Step 6: Write the failing tests for `update_backing_track_lead_in_override`**

Add to `#[cfg(test)] mod tests` in `src-tauri/crates/manifest/src/lib.rs`:

```rust
    #[test]
    fn update_backing_track_lead_in_override_updates_top_level_lesson() {
        let dir = std::env::temp_dir().join(format!("chelou-manifest-test-toplevel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = ManifestStore::new(dir.clone());
        store.save(&sample_method()).unwrap();

        store
            .update_backing_track_lead_in_override("test-method", "lesson-1", 3, 1490.0)
            .unwrap();

        let reloaded = store.load_all().unwrap();
        let loaded = reloaded.iter().find(|m| m.id == "test-method").unwrap();
        let SectionItem::Lesson(lesson) = &loaded.items[0] else {
            panic!("expected lesson at index 0");
        };
        assert_eq!(
            lesson.backing_groups[0].tracks[0].lead_in_ms_override,
            Some(1490.0)
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_backing_track_lead_in_override_updates_lesson_nested_in_section() {
        let dir = std::env::temp_dir().join(format!("chelou-manifest-test-nested-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = ManifestStore::new(dir.clone());
        store.save(&sample_method()).unwrap();

        // lesson-2 lives inside the "CHAP 1 Intro" section — exercises the recursive lookup.
        store
            .update_backing_track_lead_in_override("test-method", "lesson-2", 3, 2990.0)
            .unwrap();

        let reloaded = store.load_all().unwrap();
        let loaded = reloaded.iter().find(|m| m.id == "test-method").unwrap();
        let SectionItem::Section(section) = &loaded.items[1] else {
            panic!("expected section at index 1");
        };
        let SectionItem::Lesson(lesson) = &section.items[0] else {
            panic!("expected lesson inside section");
        };
        assert_eq!(
            lesson.backing_groups[0].tracks[0].lead_in_ms_override,
            Some(2990.0)
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_backing_track_lead_in_override_is_noop_for_unknown_lesson() {
        let dir = std::env::temp_dir().join(format!("chelou-manifest-test-unknown-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = ManifestStore::new(dir.clone());
        store.save(&sample_method()).unwrap();

        // Must not error even though "does-not-exist" isn't a real lesson id.
        store
            .update_backing_track_lead_in_override("test-method", "does-not-exist", 3, 999.0)
            .unwrap();

        let reloaded = store.load_all().unwrap();
        let loaded = reloaded.iter().find(|m| m.id == "test-method").unwrap();
        let SectionItem::Lesson(lesson) = &loaded.items[0] else {
            panic!("expected lesson at index 0");
        };
        assert_eq!(lesson.backing_groups[0].tracks[0].lead_in_ms_override, None);

        std::fs::remove_dir_all(&dir).ok();
    }
```

- [ ] **Step 7: Run tests to verify they fail**

Run: `cd src-tauri && cargo test -p chelou-manifest`
Expected: FAIL — `update_backing_track_lead_in_override` does not exist yet on `ManifestStore`
(compile error).

- [ ] **Step 8: Implement the method + recursive lookup helper**

In `src-tauri/crates/manifest/src/lib.rs`, add the free function near `items_have_lessons` (top of
the file, outside `impl` blocks):

```rust
/// Finds a `Lesson` by id anywhere in the (possibly nested) items tree.
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

Add the method to `impl ManifestStore` (near `update_lesson_resume`):

```rust
    /// Cache the final lead-in offset (ms) for a backing track, computed by the leading-
    /// silence detection (real beatsPerBar from AlphaTab, done in TabScreen.tsx). No-op if
    /// the lesson or the track's file_id isn't found — never errors for that reason, only
    /// for I/O failures.
    pub fn update_backing_track_lead_in_override(
        &self,
        method_id: &str,
        lesson_id: &str,
        file_id: u64,
        lead_in_ms: f64,
    ) -> Result<()> {
        self.update_method(method_id, |m| {
            if let Some(lesson) = find_lesson_mut(&mut m.items, lesson_id) {
                for group in &mut lesson.backing_groups {
                    for track in &mut group.tracks {
                        if track.audio.file_id == file_id {
                            track.lead_in_ms_override = Some(lead_in_ms);
                        }
                    }
                }
            }
        })
    }
```

- [ ] **Step 9: Run tests to verify they pass**

Run: `cd src-tauri && cargo test -p chelou-manifest -p chelou-pcloud`
Expected: PASS — all tests green (2 pre-existing manifest tests minus the removed syncPoints
assertion, plus 3 new ones; chelou-pcloud tests unaffected by the field removal since Step 2
already fixed the only construction site).

- [ ] **Step 10: Lint**

Run: `cd src-tauri && cargo clippy -p chelou-manifest -p chelou-pcloud`
Expected: no warnings.

- [ ] **Step 11: Update `docs/ARCHITECTURE.md`**

Three edits:

1. In §7 "Fallbacks" (near the end), remove this line entirely:
   ```
   - Tempo variable interne (non attendu) → `syncPoints` manuels.
   ```

2. In §8 "Modèle de données", change the `BackingTrack` interface from:
   ```ts
   interface BackingTrack {
     audio: FileRef;
     bpm: number;                    // parsé depuis "(NNNbpm)"
     leadInMsOverride?: number;      // sinon dérivé de bpm + defaultCountInBars
     syncPoints?: SyncPoint[];       // exception : tempo variable interne
   }
   ```
   to:
   ```ts
   interface BackingTrack {
     audio: FileRef;
     bpm: number;                    // parsé depuis "(NNNbpm)"
     leadInMsOverride?: number;      // détecté auto (silence de tête + count-in) ou réglé à la main
   }
   ```
   and remove the now-unused `interface SyncPoint { audioMs: number; tick: number; }` line right
   after the model block.

3. In §12 "Actions manuelles résiduelles", change:
   ```
   - Caler un track récalcitrant (override `leadInMs` ou `syncPoints`).
   ```
   to:
   ```
   - Caler un track récalcitrant (override manuel de `leadInMsOverride`).
   ```

- [ ] **Step 12: Commit**

```bash
git add src-tauri/crates/manifest/src/lib.rs src-tauri/crates/pcloud/src/scanner.rs docs/ARCHITECTURE.md
git commit -m "$(cat <<'EOF'
feat: persist detected lead-in into leadInMsOverride, drop syncPoints

syncPoints was an unused manual-override mechanism for internal tempo
variation, never exercised. leadInMsOverride now doubles as the target
for both manual correction and automatic detection (TabScreen.tsx,
next commit) — same field, same "wins over everything" semantics
either way, so no need to track which path set it.

find_lesson_mut mirrors update_lesson_resume's job but needs a real
tree walk: BackingTrack lives inside a Lesson that can be nested in
Sections, unlike the flat progress map resumeMs uses.
EOF
)"
```

---

### Task 2: Tauri command surface

**Files:**
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `ManifestStore::update_backing_track_lead_in_override(&self, method_id: &str, lesson_id: &str, file_id: u64, lead_in_ms: f64) -> Result<()>`
  (Task 1).
- Produces: Tauri command `update_backing_track_lead_in_override(state, method_id: String, lesson_id: String, file_id: u64, lead_in_ms: f64) -> Result<(), String>`
  — consumed by Task 3's `invoke()` call.

- [ ] **Step 1: Add the command**

In `src-tauri/src/commands/mod.rs`, add after `update_lesson_resume`:

```rust
#[tauri::command]
pub async fn update_backing_track_lead_in_override(
    state: State<'_, AppState>,
    method_id: String,
    lesson_id: String,
    file_id: u64,
    lead_in_ms: f64,
) -> Result<(), String> {
    state
        .manifest
        .update_backing_track_lead_in_override(&method_id, &lesson_id, file_id, lead_in_ms)
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 2: Register the command**

In `src-tauri/src/lib.rs`, add `commands::update_backing_track_lead_in_override,` to the
`tauri::generate_handler![...]` list, after `commands::update_lesson_resume,`.

- [ ] **Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: no errors.

- [ ] **Step 4: Lint**

Run: `cd src-tauri && cargo clippy`
Expected: no warnings introduced by this change.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: expose update_backing_track_lead_in_override as a Tauri command
EOF
)"
```

---

### Task 3: TS foundations — detection wrapper, IPC call, real beatsPerBar

**Files:**
- Modify: `src/types/model.ts`
- Modify: `src/tests/manifest-contract.ts`
- Modify: `src/lib/silence-detection.ts`
- Modify: `src/lib/ipc.ts`
- Modify: `src/hooks/useAlphaTabPlayer.ts`

**Interfaces:**
- Consumes: `scanLeadingSilence(samples, sampleRate)` (already exists); `audioUrl(fileId)` from
  `src/lib/stream.ts` (already exists).
- Produces:
  - `detectLeadingSilence(fileId: number): Promise<number | undefined>` (in `silence-detection.ts`)
  - `updateBackingTrackLeadInOverride(methodId: string, lessonId: string, fileId: number, leadInMs: number): Promise<void>` (in `ipc.ts`)
  - `useAlphaTabPlayer(...)` now returns `{ alphaTabRef: RefObject<AlphaTabApi | null>, beatsPerBarRef: RefObject<number> }` instead of a bare ref
  — all three consumed by Task 4's `TabScreen.tsx` wiring.

- [ ] **Step 1: Remove `SyncPoint` from the TS model**

In `src/types/model.ts`, change `BackingTrack` from:

```ts
export interface BackingTrack {
  audio: FileRef;
  bpm: number;
  leadInMsOverride?: number; // if absent, derived from bpm + beatsPerBar + defaultCountInBars
  syncPoints?: SyncPoint[]; // for exceptional variable-tempo tracks
}
```

to:

```ts
export interface BackingTrack {
  audio: FileRef;
  bpm: number;
  // Final lead-in offset (ms). If absent, derived from bpm + beatsPerBar + defaultCountInBars.
  // Populated automatically by TabScreen's background detection, or set manually to correct
  // a track — same field either way, whichever set it last wins.
  leadInMsOverride?: number;
}
```

and remove the now-unused `SyncPoint` interface at the bottom of the file:

```ts
export interface SyncPoint {
  audioMs: number;
  tick: number;
}
```

- [ ] **Step 2: Update the TS contract fixture**

In `src/tests/manifest-contract.ts`, remove the `SyncPoint` import (in the `import type { ... }`
block) and remove the `SAMPLE_SYNC_POINT` export block at the bottom of the file:

```ts
// SyncPoint shape (used in leadInMsOverride override paths)
export const SAMPLE_SYNC_POINT = {
  audioMs: 1234.5,
  tick: 3840,
} satisfies SyncPoint;
```

Also update the comment on the `leadInMsOverride`/`syncPoints` absence check (search for
`syncPoints absent`) to drop the now-removed field from the comment text.

- [ ] **Step 3: Type-check to confirm the removal is clean**

Run: `npx tsc --noEmit`
Expected: no errors.

- [ ] **Step 4: Add `detectLeadingSilence` to `silence-detection.ts`**

In `src/lib/silence-detection.ts`, add the import and the new function at the end of the file:

```ts
import { audioUrl } from '@/lib/stream';
```

```ts
/**
 * Fetches a backing track and runs `scanLeadingSilence` on its decoded audio.
 * This is the only place in production code that decodes a full backing track file
 * purely for analysis (playback itself uses a plain `<audio>` element, cf. TabScreen.tsx) —
 * accepted cost for now, see design spec §5.
 */
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

- [ ] **Step 5: Add the IPC wrapper**

In `src/lib/ipc.ts`, add after `updateLessonResume`:

```ts
/**
 * Persist the final computed lead-in (ms) for a backing track into leadInMsOverride.
 * Write-once by convention — callers should only invoke this when
 * `track.leadInMsOverride` is not already set.
 */
export async function updateBackingTrackLeadInOverride(
  methodId: string,
  lessonId: string,
  fileId: number,
  leadInMs: number,
): Promise<void> {
  await invoke('update_backing_track_lead_in_override', { methodId, lessonId, fileId, leadInMs });
}
```

- [ ] **Step 6: Capture the real `beatsPerBar` in `useAlphaTabPlayer`**

Read the current file first: `src/hooks/useAlphaTabPlayer.ts`. Change the hook to also track
`beatsPerBarRef` and return both refs instead of a bare ref:

```ts
export default function useAlphaTabPlayer(
  tabElmt: RefObject<HTMLDivElement | null>,
  audioElmt: RefObject<HTMLAudioElement | null>,
  tabFile: number | undefined,
) {
  const alphaTabRef = useRef<AlphaTabApi>(null);
  const beatsPerBarRef = useRef(4);

  // biome-ignore lint/correctness/useExhaustiveDependencies: refs stables, lues une fois au montage
  useEffect(() => {
    const audio = audioElmt.current;

    if (!tabFile || !tabElmt.current) {
      return;
    }

    const instance = new AlphaTabApi(tabElmt.current, {
      core: {
        file: docUrl(tabFile),
        fontDirectory: '/font/',
      },
      display: {
        resources: alphaTabResources(),
      },
      notation: {
        elements: new Map([
          [NotationElement.ScoreTitle, false],
          [NotationElement.ScoreSubTitle, false],
          [NotationElement.ScoreArtist, false],
          [NotationElement.ScoreAlbum, false],
          [NotationElement.ScoreWords, false],
          [NotationElement.ScoreMusic, false],
          [NotationElement.ScoreWordsAndMusic, false],
          [NotationElement.ScoreCopyright, false],
        ]),
      },
      player: {
        playerMode: 'EnabledExternalMedia',
      },
    });
    alphaTabRef.current = instance;

    instance.scoreLoaded.on((score) => {
      beatsPerBarRef.current = score.masterBars[0]?.timeSignatureNumerator ?? 4;
    });

    if (audio) {
      // le handler externe ne peut être branché qu'une fois le player interne prêt
      // (instance.player reste null tant que _player.instance n'est pas initialisé)
      instance.playerReady.on(() => {
        const player = instance.player;

        if (!player) {
          return;
        }

        (player.output as unknown as synth.IExternalMediaSynthOutput).handler = {
          get backingTrackDuration() {
            const duration = audio.duration;

            return Number.isFinite(duration) ? duration * 1000 : 0;
          },
          get playbackRate() {
            return audio.playbackRate ?? 1;
          },
          set playbackRate(value) {
            audio.playbackRate = value;
          },
          get masterVolume() {
            return audio.volume ?? 100;
          },
          set masterVolume(value) {
            audio.volume = value;
          },
          seekTo(time) {
            audio.currentTime = (time * audio.playbackRate) / 1000;
          },
          play() {
            audio.play();
          },
          pause() {
            audio.pause();
          },
        } as synth.IExternalMediaHandler;
      });
    }

    // suit les changements de thème (attribut data-mode / classe .dark sur <html>)
    const themeObserver = new MutationObserver(() => {
      instance.settings.fillFromJson({ display: { resources: alphaTabResources() } });
      instance.updateSettings();
      instance.render();
    });
    themeObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-mode', 'class'],
    });

    return () => {
      themeObserver.disconnect();
      instance.destroy();
    };
  }, [tabFile]);

  return { alphaTabRef, beatsPerBarRef };
}
```

(Only two things changed from the current file: the new `beatsPerBarRef` declaration, the new
`instance.scoreLoaded.on(...)` block, and the return statement at the bottom — everything else is
unchanged, shown here in full only so the diff is unambiguous.)

- [ ] **Step 7: Type-check and lint**

Run: `npx tsc --noEmit && npx biome check src/`
Expected: **errors in `TabScreen.tsx`** — it still calls
`const alphaTabRef = useAlphaTabPlayer(...)` and uses `alphaTabRef.current` directly, which no
longer matches the new `{ alphaTabRef, beatsPerBarRef }` return shape. This is expected — Task 4
fixes the call site. Confirm the errors are localized to `TabScreen.tsx` and nothing else.

- [ ] **Step 8: Commit**

```bash
git add src/types/model.ts src/tests/manifest-contract.ts src/lib/silence-detection.ts src/lib/ipc.ts src/hooks/useAlphaTabPlayer.ts
git commit -m "$(cat <<'EOF'
feat: add detection wrapper, IPC call, and real beatsPerBar capture

detectLeadingSilence fetches+decodes a track and runs the existing
pure scanLeadingSilence on it — the production counterpart to what
DebugSilenceScreen does via wavesurfer.js, without pulling that
dependency into the real playback path.

useAlphaTabPlayer now captures the score's real time signature via
scoreLoaded, replacing the debug screen's hardcoded 4/4 approximation
once TabScreen.tsx is wired up (next commit). This intentionally
leaves TabScreen.tsx non-compiling until then — its call site is the
next task.

Also finishes the syncPoints removal on the TS side (Task 1 handled
the Rust side).
EOF
)"
```

---

### Task 4: Wire background detection + persistence into `TabScreen.tsx`

**Files:**
- Modify: `src/components/templates/TabScreen.tsx`

**Interfaces:**
- Consumes: `useAlphaTabPlayer` returning `{ alphaTabRef, beatsPerBarRef }` (Task 3);
  `detectLeadingSilence(fileId): Promise<number | undefined>` (Task 3);
  `updateBackingTrackLeadInOverride(methodId, lessonId, fileId, leadInMs): Promise<void>` (Task 3);
  `BackingTrack.leadInMsOverride?: number` (Task 1/3).
- Produces: nothing new — this is the final task in the plan. `onTimeUpdate` is intentionally left
  untouched (see Global Constraints).

- [ ] **Step 1: Fix the `useAlphaTabPlayer` call site**

In `src/components/templates/TabScreen.tsx`, change:

```ts
  const alphaTabRef = useAlphaTabPlayer(tabElmt, audioElmt, tab.files[0]?.file.fileId);
```

to:

```ts
  const { alphaTabRef, beatsPerBarRef } = useAlphaTabPlayer(tabElmt, audioElmt, tab.files[0]?.file.fileId);
```

(Every other use of `alphaTabRef` elsewhere in this file is unchanged — it's still the same ref,
just destructured now instead of being the hook's sole return value.)

- [ ] **Step 2: Add the imports**

Add to the existing import block in `TabScreen.tsx`:

```ts
import { useEffect, useRef, useState } from 'react';
```

(adds `useEffect` to the existing `useRef, useState` import)

```ts
import { detectLeadingSilence } from '@/lib/silence-detection';
import { updateBackingTrackLeadInOverride } from '@/lib/ipc';
```

- [ ] **Step 3: Add the background detection + persistence effect**

Add this effect after the existing hook calls (right after the `useMetronome` call, before
`onTimeUpdate`):

```tsx
  // Runs detection in the background whenever a track without a cached lead-in is
  // selected, and persists the result — this is what actually populates
  // leadInMsOverride for real usage (the debug screen never writes it).
  useEffect(() => {
    if (!backingTrackSpeed || backingTrackSpeed.leadInMsOverride !== undefined) {
      return;
    }

    let cancelled = false;
    const track = backingTrackSpeed;

    detectLeadingSilence(track.audio.fileId).then((silenceMs) => {
      if (cancelled || silenceMs === undefined) {
        return;
      }
      const leadInMs =
        silenceMs + (method.defaultCountInBars * beatsPerBarRef.current * 60000) / track.bpm;
      updateBackingTrackLeadInOverride(method.id, lesson.id, track.audio.fileId, leadInMs);
    });

    return () => {
      cancelled = true;
    };
  }, [backingTrackSpeed, method.id, method.defaultCountInBars, lesson.id, beatsPerBarRef]);
```

The `cancelled` guard matters here specifically because `backingTrackSpeed` can change multiple
times within the same mounted `TabScreen` instance (user clicking through tempo options) — unlike
`DebugSilenceScreen`, which fully remounts per track via its React `key`. Without it, switching
tracks mid-analysis could persist a stale result under the wrong track after the user has already
moved on.

- [ ] **Step 4: Type-check and lint**

Run: `npx tsc --noEmit && npx biome check src/`
Expected: no errors — this is what resolves the errors Task 3 Step 7 intentionally left open.

- [ ] **Step 5: Manual verification**

Run: `npm run dev`

In the app: open a lesson's tab screen with a backing track that has never been analyzed (fresh
manifest, no `leadInMsOverride` set). Wait a few seconds (background fetch + decode), then:
- Navigate away and back to the same tab screen (or reopen the app). Confirm the detection doesn't
  re-run a second time for that track — the manifest should already have `leadInMsOverride` from
  the first run, and `backingTrackSpeed.leadInMsOverride !== undefined` should skip Step 3's effect
  body entirely on the second load.
- Locate the method's JSON file in the Tauri app-data directory and confirm `leadInMsOverride` is
  present and numeric on the right `BackingTrack` entry.
- Confirm switching between tempo options in the backing-track picker doesn't cause console errors
  or duplicate/incorrect writes (tests the `cancelled` guard indirectly — hard to verify precisely
  without instrumentation, but nothing should look wrong).
- Confirm playback itself is unaffected — `onTimeUpdate` wasn't touched, so the cursor should
  behave exactly as before this plan.

- [ ] **Step 6: Commit**

```bash
git add src/components/templates/TabScreen.tsx
git commit -m "$(cat <<'EOF'
feat: run leading-silence detection from TabScreen, not just debug

Background-detects and persists leadInMsOverride using the track's
real beatsPerBar (captured from AlphaTab's scoreLoaded) whenever a
track without a cached value is selected during normal use — this is
what makes the detection work land in the manifest for people other
than whoever opens the debug screen.

onTimeUpdate is untouched: this only makes leadInMsOverride get
populated, not yet consumed during playback.
EOF
)"
```
