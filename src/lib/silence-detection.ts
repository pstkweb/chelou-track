// Detects the end of leading silence in a backing track, so playback can anchor the
// count-in on the real start of sound instead of assuming it begins at t=0.
// Design: docs/superpowers/specs/2026-07-11-backing-track-silence-detection-design.md §4
//
// Revised after testing on real files (see conversation / commit history): the original
// approach (RMS per window, noise floor = mean of the first 160ms) failed whenever a track
// has little or no true silence before the count-in — the calibration window then measures
// the count-in itself instead of silence, inflating the floor past the count-in's own level.
// It also underrepresented short percussive hits, since RMS dilutes a brief transient across
// a whole window. Peak amplitude + a low percentile over a wider window fixes both: verified
// against two real files (160bpm: the old approach detected the full band entrance 1.5s late,
// instead of the count-in's first click; 80bpm, which already worked, is unaffected).

import { audioUrl } from '@/lib/stream';
import type { Provider } from '@/types/model';

export const WINDOW_MS = 20;
export const NOISE_FLOOR_WINDOW_MS = 2000; // scan this much of the lead-in to estimate the floor
export const NOISE_FLOOR_PERCENTILE = 10; // low percentile of window peaks = floor estimate
export const THRESHOLD_MULTIPLIER = 4; // onset threshold = noise floor × this factor
export const ABSOLUTE_FLOOR_DB = -50; // floor used when the measured noise floor is ~0
export const SCAN_CAP_MS = 15_000; // never search for an onset beyond this point
export const ATTACK_MARGIN_MS = 10; // step back this much so we don't clip the attack

function dbToLinear(db: number): number {
  return 10 ** (db / 20);
}

function windowPeak(samples: Float32Array, start: number, end: number): number {
  let peak = 0;
  for (let i = start; i < end; i++) {
    const abs = Math.abs(samples[i] as number);
    if (abs > peak) peak = abs;
  }
  return peak;
}

function percentile(sortedValues: number[], p: number): number {
  const index = Math.floor((p / 100) * (sortedValues.length - 1));
  return sortedValues[index] as number;
}

export interface SilenceAnalysis {
  onsetMs: number | undefined;
  noiseFloor: number;
  threshold: number;
  /** Peak amplitude of every window actually evaluated during the scan, for diagnostics. */
  windows: { ms: number; peak: number }[];
}

/**
 * Same scan as `scanLeadingSilence`, but returns the intermediate values (noise
 * floor, threshold, per-window peak trace) instead of just the result — so a caller
 * investigating a misdetection can see *why* a given window did or didn't cross
 * the threshold, without re-implementing the scan.
 */
export function analyzeLeadingSilence(samples: Float32Array, sampleRate: number): SilenceAnalysis {
  const windowSize = Math.round((WINDOW_MS / 1000) * sampleRate);
  const scanCapSamples = Math.min(samples.length, Math.round((SCAN_CAP_MS / 1000) * sampleRate));
  const windowCount = Math.floor(scanCapSamples / windowSize);

  if (windowCount < 2) {
    return { onsetMs: undefined, noiseFloor: 0, threshold: 0, windows: [] };
  }

  // Estimate the floor from a low percentile of window peaks over the leading portion of
  // the file, rather than its mean over a short fixed prefix: a percentile shrugs off both
  // a contaminating hit right at the start (unlike a mean) and a single near-zero outlier
  // sample (unlike a plain minimum), while still tracking the quiet gaps between hits.
  const floorWindowCount = Math.min(windowCount, Math.round(NOISE_FLOOR_WINDOW_MS / WINDOW_MS));
  const floorPeaks: number[] = [];
  for (let w = 0; w < floorWindowCount; w++) {
    floorPeaks.push(windowPeak(samples, w * windowSize, (w + 1) * windowSize));
  }
  floorPeaks.sort((a, b) => a - b);
  const noiseFloor = percentile(floorPeaks, NOISE_FLOOR_PERCENTILE);
  const threshold = Math.max(noiseFloor * THRESHOLD_MULTIPLIER, dbToLinear(ABSOLUTE_FLOOR_DB));

  const windows: { ms: number; peak: number }[] = [];
  let onsetMs: number | undefined;

  for (let w = 0; w < windowCount - 1; w++) {
    const peak = windowPeak(samples, w * windowSize, (w + 1) * windowSize);
    windows.push({ ms: (w * windowSize * 1000) / sampleRate, peak });

    if (onsetMs === undefined && peak > threshold) {
      const nextPeak = windowPeak(samples, (w + 1) * windowSize, (w + 2) * windowSize);
      if (nextPeak > threshold) {
        const rawOnsetMs = (w * windowSize * 1000) / sampleRate;
        onsetMs = Math.max(0, rawOnsetMs - ATTACK_MARGIN_MS);
      }
    }
  }

  return { onsetMs, noiseFloor, threshold, windows };
}

/**
 * Scans the leading portion of a decoded audio channel for the end of silence.
 * Returns `undefined` when no clear onset is found within `SCAN_CAP_MS`.
 */
export function scanLeadingSilence(samples: Float32Array, sampleRate: number): number | undefined {
  return analyzeLeadingSilence(samples, sampleRate).onsetMs;
}

/**
 * Fetches a backing track and runs `scanLeadingSilence` on its decoded audio.
 * This is the only place in production code that decodes a full backing track file
 * purely for analysis (playback itself uses a plain `<audio>` element, cf. TabScreen.tsx) —
 * accepted cost for now, see design spec §5.
 */
export async function detectLeadingSilence(
  provider: Provider,
  fileId: string,
): Promise<number | undefined> {
  const bytes = await fetch(audioUrl(provider, fileId)).then((r) => r.arrayBuffer());
  const ctx = new AudioContext();
  try {
    const buffer = await ctx.decodeAudioData(bytes);
    return scanLeadingSilence(buffer.getChannelData(0), buffer.sampleRate);
  } finally {
    ctx.close();
  }
}
