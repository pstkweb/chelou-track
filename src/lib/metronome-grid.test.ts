import { describe, expect, it } from 'vitest';
import { metronomeParams } from './metronome-grid';

describe('metronomeParams', () => {
  it('ancre la grille sur le début du count-in', () => {
    // 120 bpm, 1 mesure de 4 temps = 2000 ms de count-in avant un lead-in de 3000 ms.
    expect(metronomeParams(120, 3000, 1, 4)).toEqual({
      bpm: 120,
      anchorMs: 1000,
      beatsPerBar: 4,
    });
  });

  it('ancre sur 0 tant que le lead-in est inconnu', () => {
    expect(metronomeParams(100, undefined, 1, 3)).toEqual({
      bpm: 100,
      anchorMs: 0,
      beatsPerBar: 3,
    });
  });

  it('pas de grille sans tempo', () => {
    expect(metronomeParams(0, 3000, 1, 4)).toBeNull();
    expect(metronomeParams(undefined, 3000, 1, 4)).toBeNull();
  });
});
