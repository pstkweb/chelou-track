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
    // The floor is a low percentile of window peaks over the leading portion of the
    // file. If that whole portion is uniformly loud, the floor equals the signal
    // itself, so nothing looks "N times louder" than it and no onset is found —
    // this is intentional. Callers fall back to a 0ms offset in that case, which is
    // the correct end result.
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

  it('detects an early quiet click instead of skipping to a later loud section', () => {
    // Regression test for the real bug this revision fixes: a count-in with almost
    // no leading silence, followed by quiet clicks, then the full (much louder) mix.
    // The old mean-over-160ms floor calibration measured the first click itself as
    // "the floor", raising the threshold high enough that both clicks were skipped
    // and only the loud section was detected — 1.5s late on the real file that
    // exposed this. Mirrors that shape at 1/40th scale (40ms lead-in, ~60ms quiet
    // clicks 380ms apart, loud section afterwards).
    const samples = buildSamples(
      segment(40, 0), // near-nonexistent lead-in silence
      segment(60, 0.06), // first count-in click
      segment(320, 0), // gap
      segment(60, 0.06), // second count-in click
      segment(320, 0), // gap
      segment(1200, 0.5), // the actual song, much louder than the clicks
    );
    expect(scanLeadingSilence(samples, SAMPLE_RATE)).toBe(30);
  });
});
