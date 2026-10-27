import { describe, expect, it } from 'vitest';
import type { BackingInfo } from './ipc';
import { leadInFromSilence } from './lead-in';

const info = (fileId: string, silenceMs: number | null): BackingInfo => ({
  fileId,
  durationMs: 60_000,
  silenceMs,
  sampleRate: 48_000,
});

describe('leadInFromSilence', () => {
  it('adds the count-in duration to the detected silence', () => {
    // 1 mesure de 4 temps à 120 bpm = 2000 ms
    expect(leadInFromSilence('a', info('a', 500), 1, 4, 120)).toBe(2500);
  });

  it('ignores info that belongs to another track', () => {
    // Le track a changé entre-temps : persister ce lead-in le figerait sur le mauvais fichier.
    expect(leadInFromSilence('b', info('a', 500), 1, 4, 120)).toBeUndefined();
  });

  it('is undefined when no onset was found', () => {
    expect(leadInFromSilence('a', info('a', null), 1, 4, 120)).toBeUndefined();
  });

  it('is undefined while the track is not loaded', () => {
    expect(leadInFromSilence('a', undefined, 1, 4, 120)).toBeUndefined();
  });
});
