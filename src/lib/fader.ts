/**
 * Position d'un fader (0-100, 80 = 0 dB) en décibels affichables : `dB = 40·log10(v/80)`, le
 * moteur appliquant un gain linéaire `(v/80)²` (cf. `chelou_audio_core::gain::fader_gain`).
 */
export function faderDb(value: number): string {
  if (value <= 0) {
    return '−∞ dB';
  }
  const db = 40 * Math.log10(value / 80);
  const sign = db > 0.05 ? '+' : db < -0.05 ? '−' : '';

  return `${sign}${Math.abs(db).toFixed(1)} dB`;
}
