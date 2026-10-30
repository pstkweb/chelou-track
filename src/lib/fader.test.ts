import { describe, expect, it } from 'vitest';
import { faderDb } from './fader';

describe('faderDb', () => {
  it('formats the fader position in dB with 80 = 0 dB', () => {
    expect(faderDb(0)).toBe('−∞ dB');
    expect(faderDb(80)).toBe('0.0 dB');
    expect(faderDb(40)).toBe('−12.0 dB');
    expect(faderDb(100)).toBe('+3.9 dB');
  });
});
