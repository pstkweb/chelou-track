# Backing Track Silence Debug View — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the leading-silence detection algorithm and a debug screen that visualizes it
(waveform + markers + playback) on real backing track files, so we can validate the approach
before wiring it into production playback.

**Architecture:** A pure TS function (`scanLeadingSilence`) analyzes a decoded `AudioBuffer` for
the end of leading silence using an adaptive RMS threshold. A new React screen
(`DebugSilenceScreen`), reached via a button in `TabScreen.tsx`, loads a backing track through
`wavesurfer.js`, runs the detection on the decoded buffer, and overlays the result as zero-width
Regions on the waveform, with playback controls to confirm the result by ear.

**Tech Stack:** TypeScript, React 19, `wavesurfer.js` (new dependency), `vitest` (new dev
dependency, first test runner in this project).

## Global Constraints

- **Scope for this plan: debug view + detection calculation only.** No Rust changes, no manifest
  persistence, no IPC command, no wiring into `TabScreen.tsx`'s real `onTimeUpdate` sync loop. Spec
  sections §3, §5, §6, §7 (`detectedSilenceMs` model field, `update_backing_track_silence`,
  production `leadInMs` branching) are deferred — see
  `docs/superpowers/specs/2026-07-11-backing-track-silence-detection-design.md`.
- The WebView must never receive a raw pCloud URL or token — reuse the existing `audioUrl(fileId)`
  helper (`src/lib/stream.ts`), never construct pCloud URLs directly.
- `scanLeadingSilence` must stay a pure function (no fetch, no DOM) — this is what makes it
  testable with `vitest` without mocking anything.
- Follow the existing pattern of wrapping imperative libraries in a custom hook (see
  `src/hooks/useAlphaTabPlayer.ts`) rather than calling `wavesurfer.js` directly from a component.
- This is the project's first `vitest` usage — no existing test config to match; keep the setup
  minimal (no config file unless something forces it).

---

### Task 1: `scanLeadingSilence` algorithm + vitest setup

**Files:**
- Modify: `package.json`
- Create: `src/lib/silence-detection.ts`
- Test: `src/lib/silence-detection.test.ts`

**Interfaces:**
- Produces: `scanLeadingSilence(samples: Float32Array, sampleRate: number): number | undefined`
  and exported constants `WINDOW_MS`, `NOISE_FLOOR_WINDOWS`, `THRESHOLD_MULTIPLIER`,
  `ABSOLUTE_FLOOR_DB`, `SCAN_CAP_MS`, `ATTACK_MARGIN_MS` — all consumed by Task 2's
  `useWavesurfer` hook and Task 3's `DebugSilenceScreen`.

- [ ] **Step 1: Install vitest**

Run: `npm install -D vitest`

- [ ] **Step 2: Add the `test` script and wire it into `ci`**

In `package.json`, under `"scripts"`, add a `test` entry and update `ci`:

```json
{
  "scripts": {
    "dev": "tauri dev",
    "build": "tauri build",
    "tauri": "tauri",
    "lint": "biome check .",
    "lint:fix": "biome check --write --unsafe .",
    "format": "biome format --write .",
    "type-check": "tsc --noEmit",
    "test": "vitest run",
    "ci": "biome ci . && tsc --noEmit && vitest run"
  }
}
```

- [ ] **Step 3: Write the failing test file**

Create `src/lib/silence-detection.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { scanLeadingSilence } from './silence-detection';

// 1 sample = 1ms at this rate — keeps the expected-value arithmetic exact and readable.
const SAMPLE_RATE = 1000;

function segment(durationMs: number, amplitude: number): number[] {
  return new Array(durationMs).fill(amplitude);
}

function buildSamples(...segments: number[][]): Float32Array {
  return Float32Array.from(segments.flat());
}

describe('scanLeadingSilence', () => {
  it('detects a clean onset after silence', () => {
    const samples = buildSamples(segment(500, 0), segment(1500, 0.5));
    expect(scanLeadingSilence(samples, SAMPLE_RATE)).toBe(490);
  });

  it('returns undefined when the track is loud from the very first sample', () => {
    // The relative threshold is calibrated from the first ~160ms of audio. If that
    // window is already loud, nothing will look "9x louder" than it, so no onset is
    // found — this is intentional (see design spec §4). Callers fall back to a 0ms
    // offset in that case, which is the correct end result.
    const samples = buildSamples(segment(2000, 0.5));
    expect(scanLeadingSilence(samples, SAMPLE_RATE)).toBeUndefined();
  });

  it('returns undefined when the whole scan window stays silent', () => {
    const samples = buildSamples(segment(2000, 0));
    expect(scanLeadingSilence(samples, SAMPLE_RATE)).toBeUndefined();
  });

  it('ignores an isolated spike and still detects the real onset', () => {
    const samples = buildSamples(segment(500, 0), segment(1500, 0.5));
    // A 2-sample digital click inside the silence, well before the real onset at 500ms.
    samples[300] = 1;
    samples[301] = 1;
    expect(scanLeadingSilence(samples, SAMPLE_RATE)).toBe(490);
  });

  it('ignores steady low-level background noise and still detects the real onset', () => {
    const samples = buildSamples(segment(500, 0.01), segment(1500, 0.5));
    expect(scanLeadingSilence(samples, SAMPLE_RATE)).toBe(490);
  });

  it('returns undefined when the onset falls beyond the scan cap', () => {
    const samples = buildSamples(segment(16_000, 0), segment(1000, 0.5));
    expect(scanLeadingSilence(samples, SAMPLE_RATE)).toBeUndefined();
  });
});
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `npx vitest run`
Expected: FAIL — `src/lib/silence-detection.ts` does not exist yet (`Cannot find module
'./silence-detection'` or equivalent resolution error).

- [ ] **Step 5: Implement `scanLeadingSilence`**

Create `src/lib/silence-detection.ts`:

```ts
// Detects the end of leading silence in a backing track, so playback can anchor the
// count-in on the real start of sound instead of assuming it begins at t=0.
// Design: docs/superpowers/specs/2026-07-11-backing-track-silence-detection-design.md §4

export const WINDOW_MS = 20;
export const NOISE_FLOOR_WINDOWS = 8; // ~160ms used to measure the baseline noise floor
export const THRESHOLD_MULTIPLIER = 9; // onset threshold = noise floor × this factor
export const ABSOLUTE_FLOOR_DB = -50; // floor used when the measured noise floor is ~0
export const SCAN_CAP_MS = 15_000; // never search for an onset beyond this point
export const ATTACK_MARGIN_MS = 10; // step back this much so we don't clip the attack

function dbToLinear(db: number): number {
  return 10 ** (db / 20);
}

function windowRms(samples: Float32Array, start: number, end: number): number {
  let sumSquares = 0;
  for (let i = start; i < end; i++) {
    sumSquares += samples[i] * samples[i];
  }
  const count = end - start;
  return count > 0 ? Math.sqrt(sumSquares / count) : 0;
}

/**
 * Scans the leading portion of a decoded audio channel for the end of silence.
 * Returns `undefined` when no clear onset is found within `SCAN_CAP_MS` — callers
 * should fall back to treating the track as having no leading silence (offset 0).
 */
export function scanLeadingSilence(samples: Float32Array, sampleRate: number): number | undefined {
  const windowSize = Math.round((WINDOW_MS / 1000) * sampleRate);
  const scanCapSamples = Math.min(samples.length, Math.round((SCAN_CAP_MS / 1000) * sampleRate));
  const windowCount = Math.floor(scanCapSamples / windowSize);

  if (windowCount <= NOISE_FLOOR_WINDOWS + 1) {
    return undefined;
  }

  let noiseFloorSum = 0;
  for (let w = 0; w < NOISE_FLOOR_WINDOWS; w++) {
    noiseFloorSum += windowRms(samples, w * windowSize, (w + 1) * windowSize);
  }
  const noiseFloor = noiseFloorSum / NOISE_FLOOR_WINDOWS;
  const threshold = Math.max(noiseFloor * THRESHOLD_MULTIPLIER, dbToLinear(ABSOLUTE_FLOOR_DB));

  for (let w = NOISE_FLOOR_WINDOWS; w < windowCount - 1; w++) {
    const rms = windowRms(samples, w * windowSize, (w + 1) * windowSize);
    if (rms > threshold) {
      const nextRms = windowRms(samples, (w + 1) * windowSize, (w + 2) * windowSize);
      if (nextRms > threshold) {
        const onsetMs = (w * windowSize * 1000) / sampleRate;
        return Math.max(0, onsetMs - ATTACK_MARGIN_MS);
      }
    }
  }

  return undefined;
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `npx vitest run`
Expected: PASS — all 6 tests in `silence-detection.test.ts` green.

- [ ] **Step 7: Type-check and lint**

Run: `npx tsc --noEmit && npx biome check src/lib/silence-detection.ts src/lib/silence-detection.test.ts`
Expected: no errors.

- [ ] **Step 8: Commit**

```bash
git add package.json package-lock.json src/lib/silence-detection.ts src/lib/silence-detection.test.ts
git commit -m "$(cat <<'EOF'
feat: add leading-silence detection algorithm

Pure RMS-threshold scan over a decoded audio channel, with an adaptive
noise floor and debounce against isolated clicks. Introduces vitest as
the project's first JS test runner.
EOF
)"
```

---

### Task 2: `useWavesurfer` hook

**Files:**
- Modify: `package.json`
- Create: `src/hooks/useWavesurfer.ts`

**Interfaces:**
- Consumes: `scanLeadingSilence(samples, sampleRate)` and `SCAN_CAP_MS` from
  `src/lib/silence-detection.ts` (Task 1); `audioUrl(fileId: number): string` from
  `src/lib/stream.ts` (existing).
- Produces: default export
  `useWavesurfer(containerRef: RefObject<HTMLDivElement | null>, fileId: number): { wavesurferRef: RefObject<WaveSurfer | null>; regionsRef: RefObject<RegionsPlugin | null>; silenceMs: number | undefined; ready: boolean }`
  — consumed by Task 3's `DebugSilenceScreen`.

- [ ] **Step 1: Install wavesurfer.js**

Run: `npm install wavesurfer.js`

- [ ] **Step 2: Confirm the Regions plugin import path**

Run (PowerShell): `Get-ChildItem node_modules/wavesurfer.js/dist/plugins/regions*`

Expected: a file matching `regions.esm.js` (or `regions.js` for the CJS build). This plan assumes
`wavesurfer.js/dist/plugins/regions.esm.js` — **if the installed version exposes a different path
or export map entry, use that path instead** in Step 3 below (check
`node_modules/wavesurfer.js/package.json` `"exports"` field if the plain path 404s on import).

- [ ] **Step 3: Write the hook**

Create `src/hooks/useWavesurfer.ts`:

```ts
import { type RefObject, useEffect, useRef, useState } from 'react';
import WaveSurfer from 'wavesurfer.js';
import RegionsPlugin from 'wavesurfer.js/dist/plugins/regions.esm.js';
import { scanLeadingSilence } from '@/lib/silence-detection';
import { audioUrl } from '@/lib/stream';

export default function useWavesurfer(
  containerRef: RefObject<HTMLDivElement | null>,
  fileId: number,
) {
  const wavesurferRef = useRef<WaveSurfer | null>(null);
  const regionsRef = useRef<RegionsPlugin | null>(null);
  const [silenceMs, setSilenceMs] = useState<number | undefined>(undefined);
  const [ready, setReady] = useState(false);

  // biome-ignore lint/correctness/useExhaustiveDependencies: containerRef is a stable ref
  useEffect(() => {
    if (!containerRef.current) return;

    setReady(false);
    setSilenceMs(undefined);

    const regions = RegionsPlugin.create();
    const instance = WaveSurfer.create({
      container: containerRef.current,
      url: audioUrl(fileId),
      waveColor: '#7c8591',
      progressColor: '#4f8cff',
      height: 120,
      plugins: [regions],
    });

    wavesurferRef.current = instance;
    regionsRef.current = regions;

    instance.on('ready', () => {
      const decoded = instance.getDecodedData();
      const detected = decoded
        ? scanLeadingSilence(decoded.getChannelData(0), decoded.sampleRate)
        : undefined;

      if (detected !== undefined) {
        regions.addRegion({
          start: detected / 1000,
          color: 'rgba(255, 190, 0, 0.6)',
          content: 'Silence détecté',
        });
      }

      setSilenceMs(detected);
      setReady(true);
    });

    return () => {
      instance.destroy();
      wavesurferRef.current = null;
      regionsRef.current = null;
    };
  }, [fileId]);

  return { wavesurferRef, regionsRef, silenceMs, ready };
}
```

- [ ] **Step 4: Type-check**

Run: `npx tsc --noEmit`
Expected: no errors. If the Regions import path from Step 2 doesn't resolve, TypeScript will
fail here first — fix the import path before moving on.

- [ ] **Step 5: Lint**

Run: `npx biome check src/hooks/useWavesurfer.ts`
Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add package.json package-lock.json src/hooks/useWavesurfer.ts
git commit -m "$(cat <<'EOF'
feat: add useWavesurfer hook for waveform debug visualization

Wraps wavesurfer.js + its Regions plugin (same pattern as
useAlphaTabPlayer for AlphaTab), runs scanLeadingSilence on the decoded
buffer once loaded, and marks the detected onset as a region.
EOF
)"
```

---

### Task 3: Debug screen, navigation wiring, and entry point button

**Files:**
- Modify: `src/contexts/NavigationContext.tsx`
- Create: `src/components/templates/DebugSilenceScreen.tsx`
- Modify: `src/components/templates/TabScreen.tsx`
- Modify: `src/components/pages/App.tsx`

**Interfaces:**
- Consumes: `useWavesurfer` (Task 2), `SCAN_CAP_MS` from `src/lib/silence-detection.ts` (Task 1),
  existing `useNavigation()` context, existing `Button`/`IconButton` atoms.
- Produces: `useNavigation().openDebugSilence(lesson: Lesson, chapter: Chapter, tab: TabSet, track: BackingTrack): void`
  and the `{ id: 'debug-silence' }` screen — no other task depends on these, this is the final
  task in the plan.

- [ ] **Step 1: Add the `debug-silence` screen to `NavigationContext.tsx`**

In `src/contexts/NavigationContext.tsx`, update the imports:

```ts
import { createContext, useCallback, useContext, useEffect, useMemo, useReducer } from 'react';
import type { Chapter } from '@/lib/method-view';
import type { BackingTrack, Lesson, Method, TabSet } from '@/types/model';
import { useBreadcrumb } from './BreadcrumbContext';
```

Extend the `Screen` union:

```ts
export type Screen =
  | { id: 'library' }
  | { id: 'method'; method: Method }
  | { id: 'player'; method: Method; lesson: Lesson; chapter: Chapter }
  | { id: 'tab'; method: Method; lesson: Lesson; chapter: Chapter; tab: TabSet }
  | {
      id: 'debug-silence';
      method: Method;
      lesson: Lesson;
      chapter: Chapter;
      tab: TabSet;
      track: BackingTrack;
    }
  | { id: 'documents'; method: Method };
```

Extend the `NavAction` union:

```ts
type NavAction =
  | { type: 'library' }
  | { type: 'method'; method: Method }
  | { type: 'player'; lesson: Lesson; chapter: Chapter }
  | { type: 'tab'; lesson: Lesson; chapter: Chapter; tab: TabSet }
  | { type: 'debug-silence'; lesson: Lesson; chapter: Chapter; tab: TabSet; track: BackingTrack }
  | { type: 'documents'; method: Method };
```

Add the reducer case (right after the existing `case 'tab':` block, before `case 'documents':`):

```ts
    case 'debug-silence':
      if (screen.id === 'tab' || screen.id === 'debug-silence') {
        return {
          id: 'debug-silence',
          method: screen.method,
          lesson: action.lesson,
          chapter: action.chapter,
          tab: action.tab,
          track: action.track,
        };
      }
      return screen;
```

Extend `NavigationContextValue`:

```ts
interface NavigationContextValue {
  screen: Screen;
  goToLibrary: () => void;
  goToMethod: (method: Method) => void;
  openLesson: (lesson: Lesson, chapter: Chapter) => void;
  openTab: (lesson: Lesson, chapter: Chapter, tab: TabSet) => void;
  openDebugSilence: (lesson: Lesson, chapter: Chapter, tab: TabSet, track: BackingTrack) => void;
  listDocuments: (method: Method) => void;
}
```

Add the callback (next to `openTab`'s definition):

```ts
  const openDebugSilence = useCallback(
    (lesson: Lesson, chapter: Chapter, tab: TabSet, track: BackingTrack) =>
      dispatch({ type: 'debug-silence', lesson, chapter, tab, track }),
    [],
  );
```

Add the breadcrumb branch (in the `useEffect` that dispatches breadcrumbs, right after the `else
if (screen.id === 'tab')` block and before the final `else`):

```ts
    } else if (screen.id === 'debug-silence') {
      dispatchBreadcrumb({
        type: 'replace',
        payload: [
          { label: 'Bibliothèque', onClick: goToLibrary },
          { label: screen.method.title, onClick: () => goToMethod(screen.method) },
          { label: screen.lesson.title, onClick: () => openLesson(screen.lesson, screen.chapter) },
          { label: screen.tab.title, onClick: () => openTab(screen.lesson, screen.chapter, screen.tab) },
          { label: 'Debug silence' },
        ],
      });
```

Update the `useMemo` at the bottom to include the new callback:

```ts
  const value = useMemo(
    () => ({
      screen,
      goToLibrary,
      goToMethod,
      openLesson,
      openTab,
      openDebugSilence,
      listDocuments,
    }),
    [screen, goToLibrary, goToMethod, openLesson, openTab, openDebugSilence, listDocuments],
  );
```

- [ ] **Step 2: Type-check the navigation change in isolation**

Run: `npx tsc --noEmit`
Expected: errors about `DebugSilenceScreen` not existing yet are **not** expected at this point —
this step only touches `NavigationContext.tsx`, which doesn't reference the component. Expected:
no errors.

- [ ] **Step 3: Write `DebugSilenceScreen.tsx`**

Create `src/components/templates/DebugSilenceScreen.tsx`:

```tsx
import { Bug, ChevronLeft } from 'lucide-react';
import { useEffect, useRef } from 'react';
import { useNavigation } from '@/contexts/NavigationContext';
import useWavesurfer from '@/hooks/useWavesurfer';
import type { Chapter } from '@/lib/method-view';
import { SCAN_CAP_MS } from '@/lib/silence-detection';
import type { BackingTrack, Lesson, Method, TabSet } from '@/types/model';
import Button from '../atoms/Button';
import IconButton from '../atoms/IconButton';

type DebugSilenceScreenProps = {
  method: Method;
  lesson: Lesson;
  chapter: Chapter;
  tab: TabSet;
  track: BackingTrack;
};

// This screen doesn't load AlphaTab/the score, so there's no real time signature
// available — 4/4 is assumed purely for the visual "effective start" marker.
const ASSUMED_BEATS_PER_BAR = 4;

export default function DebugSilenceScreen({
  method,
  lesson,
  chapter,
  tab,
  track,
}: DebugSilenceScreenProps) {
  const { openTab } = useNavigation();
  const containerRef = useRef<HTMLDivElement>(null);
  const { wavesurferRef, regionsRef, silenceMs, ready } = useWavesurfer(
    containerRef,
    track.audio.fileId,
  );

  const leadInMs =
    silenceMs !== undefined
      ? silenceMs + (method.defaultCountInBars * ASSUMED_BEATS_PER_BAR * 60000) / track.bpm
      : undefined;

  useEffect(() => {
    const regions = regionsRef.current;
    if (!ready || !regions || leadInMs === undefined) return;

    regions.addRegion({
      start: leadInMs / 1000,
      color: 'rgba(0, 200, 100, 0.6)',
      content: 'Début effectif (4/4 approx.)',
    });
    regions.addRegion({
      start: SCAN_CAP_MS / 1000,
      color: 'rgba(200, 60, 60, 0.6)',
      content: 'Fin fenêtre de scan',
    });
  }, [ready, leadInMs, regionsRef]);

  const onPlayPause = () => wavesurferRef.current?.playPause();

  const onPlayFromLeadIn = () => {
    if (leadInMs === undefined || !wavesurferRef.current) return;
    wavesurferRef.current.setTime(leadInMs / 1000);
    wavesurferRef.current.play();
  };

  return (
    <div className="grain relative flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-3.5 border-border border-b bg-bg2 px-[clamp(16px,3vw,32px)] py-3.5">
        <IconButton className="border border-border" onClick={() => openTab(lesson, chapter, tab)}>
          <ChevronLeft size={18} />
        </IconButton>
        <div className="type-ic tab size-9">
          <Bug size={20} />
        </div>
        <div className="min-w-0 flex-1">
          <div className="display text-xl">Debug silence — {track.audio.name}</div>
          <div className="mono text-fg3 text-xs">
            {tab.title} · {method.title}
          </div>
        </div>
      </div>

      <div className="scroll flex flex-1 flex-col gap-6 p-[clamp(16px,3vw,40px)]">
        <div className="card bg-surface p-[clamp(16px,2.5vw,30px)]">
          <div ref={containerRef} className="w-full" />
          {!ready && <div className="mono text-fg3 text-sm">Chargement et analyse…</div>}
        </div>

        <div className="card bg-surface flex flex-wrap items-center gap-[clamp(12px,2vw,24px)] p-[clamp(16px,2.5vw,30px)]">
          <Button variant="primary" onClick={onPlayPause} disabled={!ready}>
            Play / Pause
          </Button>
          <Button onClick={onPlayFromLeadIn} disabled={leadInMs === undefined}>
            Écouter à partir du lead-in
          </Button>

          <div className="mono text-fg3 text-xs">
            <div>bpm : {track.bpm}</div>
            <div>defaultCountInBars : {method.defaultCountInBars}</div>
            <div>silenceMs : {silenceMs ?? '—'}</div>
            <div>
              leadInMs (4/4 approx.) : {leadInMs !== undefined ? Math.round(leadInMs) : '—'}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Wire the screen into `App.tsx`**

In `src/components/pages/App.tsx`, add the import:

```ts
import DebugSilenceScreen from '../templates/DebugSilenceScreen';
```

Add a branch in `AuthenticatedView`, right after the `screen.id === 'tab'` block:

```tsx
  if (screen.id === 'debug-silence') {
    return (
      <DebugSilenceScreen
        key={screen.track.audio.fileId}
        method={screen.method}
        lesson={screen.lesson}
        chapter={screen.chapter}
        tab={screen.tab}
        track={screen.track}
      />
    );
  }
```

- [ ] **Step 5: Add the entry point button in `TabScreen.tsx`**

In `src/components/templates/TabScreen.tsx`, add `Bug` to the `lucide-react` import:

```ts
import {
  Bug,
  ChevronLeft,
  Metronome,
  Music,
  Pause,
  Play,
  RefreshCcw,
  SkipBack,
  SkipForward,
  Video,
} from 'lucide-react';
```

Destructure `openDebugSilence` from `useNavigation()`:

```ts
  const { openLesson, openDebugSilence } = useNavigation();
```

In the footer bar, inside the `flex items-center gap-1` group that already holds the loop and
metronome `IconButton`s, add a third button (only rendered once a track is selected):

```tsx
            <div className="flex items-center gap-1">
              <IconButton
                onClick={() => setLoop((l) => !l)}
                title="Répéter"
                className={cn('bg-transparent text-fg3', loop && 'bg-chip! text-accent!')}
              >
                <RefreshCcw size={19} />
              </IconButton>
              <IconButton
                onClick={() => setMetro((m) => !m)}
                title="Métronome"
                className={cn('bg-transparent text-fg3', metro && 'bg-chip! text-accent!')}
              >
                <Metronome size={19} />
              </IconButton>
              {backingTrackSpeed && (
                <IconButton
                  onClick={() => openDebugSilence(lesson, chapter, tab, backingTrackSpeed)}
                  title="Debug silence"
                  className="bg-transparent text-fg3"
                >
                  <Bug size={19} />
                </IconButton>
              )}
            </div>
```

- [ ] **Step 6: Type-check and lint everything**

Run: `npx tsc --noEmit && npx biome check src/`
Expected: no errors.

- [ ] **Step 7: Manual verification**

Run: `npm run dev`

In the app: open a method → a lesson that has a tab with at least one backing group → click the
new bug icon next to the metronome button. Confirm:
- The waveform for the full backing track file renders.
- After a moment, an orange "Silence détecté" marker and a green "Début effectif (4/4 approx.)"
  marker appear (and a red "Fin fenêtre de scan" marker near the 15s mark if the file is long
  enough to show it).
- The numeric readout (`silenceMs`, `leadInMs`, `bpm`, `defaultCountInBars`) matches the markers.
- Play/Pause plays the full track.
- "Écouter à partir du lead-in" jumps playback to the green marker and you can confirm by ear that
  it lands on (or very close to) the first real beat after the count-in.
- The back button returns to the tab screen for the same lesson.

If the detected markers are clearly wrong on a real file (e.g. off by more than a beat or two),
note it — the constants in `src/lib/silence-detection.ts` (`THRESHOLD_MULTIPLIER`,
`ATTACK_MARGIN_MS`, etc.) are the tuning knobs, not the overall algorithm shape.

- [ ] **Step 8: Commit**

```bash
git add src/contexts/NavigationContext.tsx src/components/templates/DebugSilenceScreen.tsx src/components/templates/TabScreen.tsx src/components/pages/App.tsx
git commit -m "$(cat <<'EOF'
feat: add silence-detection debug view

Reachable from TabScreen via a new "Debug silence" button. Loads the
selected backing track through wavesurfer.js, runs the leading-silence
scan on the decoded buffer, and overlays the result as waveform
regions, with playback controls to validate by ear.
EOF
)"
```
