import type { MetronomeParams } from '@/lib/ipc';

/**
 * Grille du métronome envoyée au moteur (le clic y est synthétisé à l'échantillon près) : temps
 * calés sur le début du count-in, `countInBars` mesures avant le lead-in (ARCHITECTURE.md §7) ;
 * ancre 0 tant que le lead-in n'est pas connu. `null` sans tempo.
 */
export function metronomeParams(
  bpm: number | undefined,
  leadInMs: number | undefined,
  countInBars: number,
  beatsPerBar: number,
): MetronomeParams | null {
  if (!bpm || bpm <= 0) {
    return null;
  }
  const anchorMs =
    leadInMs === undefined ? 0 : leadInMs - (countInBars * beatsPerBar * 60000) / bpm;

  return { bpm, anchorMs, beatsPerBar };
}
