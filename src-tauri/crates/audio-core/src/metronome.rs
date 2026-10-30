// Métronome natif (spec §5.5) : clic synthétisé une fois par fréquence (portage de `playWoodTick`
// de l'ancien métronome WebView), puis mixé dans le callback sur la position exacte du lecteur —
// à l'échantillon près, quel que soit le découpage en blocs. Aucune allocation dans `render`.

use std::f64::consts::PI;

/// Durée d'un clic (arrêt de l'oscillateur dans `playWoodTick`).
const CLICK_S: f64 = 0.040;
const SWEEP_S: f64 = 0.018;
const NOISE_S: f64 = 0.030;
const NOISE_DECAY_S: f64 = 0.015;
/// Fin des rampes exponentielles (`exponentialRampToValueAtTime` refuse 0).
const FLOOR: f64 = 0.0001;

/// Clics mono pré-calculés à une fréquence donnée.
pub struct ClickSamples {
    pub accent: Vec<f32>,
    pub normal: Vec<f32>,
}

/// Grille du métronome : temps `k` à `anchor_ms + k × 60000 / bpm` (k entier, négatif compris).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MetronomeParams {
    pub bpm: f64,
    pub anchor_ms: f64,
    pub beats_per_bar: u32,
}

/// Biquad (formules RBJ), forme directe I.
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    x: [f64; 2],
    y: [f64; 2],
}

impl Biquad {
    fn new(b: [f64; 3], a0: f64, a: [f64; 2]) -> Biquad {
        Biquad {
            b: b.map(|v| v / a0),
            a: a.map(|v| v / a0),
            x: [0.0; 2],
            y: [0.0; 2],
        }
    }

    fn highpass(freq: f64, q: f64, rate: f64) -> Biquad {
        let w = 2.0 * PI * freq / rate;
        let alpha = w.sin() / (2.0 * q);
        let c = w.cos();
        Biquad::new(
            [(1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0],
            1.0 + alpha,
            [-2.0 * c, 1.0 - alpha],
        )
    }

    /// Passe-bande à gain unitaire au centre (`BiquadFilterNode` « bandpass »).
    fn bandpass(freq: f64, q: f64, rate: f64) -> Biquad {
        let w = 2.0 * PI * freq / rate;
        let alpha = w.sin() / (2.0 * q);
        Biquad::new(
            [alpha, 0.0, -alpha],
            1.0 + alpha,
            [-2.0 * w.cos(), 1.0 - alpha],
        )
    }

    fn tick(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.b[1] * self.x[0] + self.b[2] * self.x[1]
            - self.a[0] * self.y[0]
            - self.a[1] * self.y[1];
        self.x = [x, self.x[0]];
        self.y = [y, self.y[0]];
        y
    }
}

/// Valeur de `from` à `to` en rampe exponentielle sur `duration`, puis tenue.
fn exp_ramp(from: f64, to: f64, t: f64, duration: f64) -> f64 {
    from * (to / from).powf((t / duration).min(1.0))
}

fn triangle(phase: f64) -> f64 {
    if phase < 0.25 {
        4.0 * phase
    } else if phase < 0.75 {
        2.0 - 4.0 * phase
    } else {
        4.0 * phase - 4.0
    }
}

fn synth_click(rate: u32, accent: bool) -> Vec<f32> {
    let fs = rate as f64;
    let (f0, f1, g0, center) = if accent {
        (2700.0, 850.0, 0.7, 1600.0)
    } else {
        (2000.0, 750.0, 0.45, 1400.0)
    };
    let mut highpass = Biquad::highpass(2500.0, std::f64::consts::FRAC_1_SQRT_2, fs);
    let mut bandpass = Biquad::bandpass(center, 5.0, fs);
    // Bruit blanc déterministe (LCG) : le clic est identique d'une ouverture à l'autre.
    let mut seed: u32 = 0x1234_5678;
    let mut phase = 0.0;
    let len = (CLICK_S * fs).round() as usize;
    (0..len)
        .map(|i| {
            let t = i as f64 / fs;
            let osc = triangle(phase) * exp_ramp(g0, FLOOR, t, CLICK_S);
            phase = (phase + exp_ramp(f0, f1, t, SWEEP_S) / fs).fract();
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let white = (seed >> 8) as f64 / (1u32 << 23) as f64 - 1.0;
            let noise = if t < NOISE_S {
                highpass.tick(white) * exp_ramp(0.18, FLOOR, t, NOISE_DECAY_S)
            } else {
                0.0
            };
            bandpass.tick(osc + noise) as f32
        })
        .collect()
}

impl ClickSamples {
    pub fn new(sample_rate: u32) -> ClickSamples {
        ClickSamples {
            accent: synth_click(sample_rate, true),
            normal: synth_click(sample_rate, false),
        }
    }
}

/// Côté callback audio.
pub struct Metronome {
    samples: ClickSamples,
    /// Clic en cours : accent, prochain échantillon.
    active: Option<(bool, usize)>,
    /// Position attendue au prochain bloc ; une autre = seek.
    expected: Option<u64>,
}

impl Metronome {
    pub fn new(samples: ClickSamples) -> Metronome {
        Metronome {
            samples,
            active: None,
            expected: None,
        }
    }

    /// Frame d'attaque du temps `k`.
    fn onset(p: &MetronomeParams, k: i64, rate: u32) -> i64 {
        ((p.anchor_ms + k as f64 * 60000.0 / p.bpm) * rate as f64 / 1000.0).round() as i64
    }

    /// Ajoute à `out` (stéréo entrelacé) les clics qui sonnent sur `[position, position + frames)`
    /// × `gain`, et rend la crête ajoutée. `params = None` (clic coupé, pause) : rien, et le clic
    /// en cours est oublié ; une position qui ne suit pas le bloc précédent (seek) aussi.
    pub fn render(
        &mut self,
        out: &mut [f32],
        position: u64,
        rate: u32,
        params: Option<MetronomeParams>,
        gain: f32,
    ) -> f32 {
        let frames = out.len() / 2;
        let p = match params {
            Some(p) if p.bpm > 0.0 && p.bpm.is_finite() && p.anchor_ms.is_finite() => p,
            _ => {
                self.active = None;
                self.expected = None;
                return 0.0;
            }
        };
        if self.expected != Some(position) {
            self.active = None;
        }
        self.expected = Some(position + frames as u64);

        let mut peak = 0.0f32;
        let start = position as i64;
        let end = start + frames as i64;
        let period_frames = 60000.0 / p.bpm * rate as f64 / 1000.0;
        let anchor_frames = p.anchor_ms * rate as f64 / 1000.0;
        // Premier temps candidat (un de moins par sécurité d'arrondi).
        let mut k = ((start as f64 - anchor_frames) / period_frames).ceil() as i64 - 1;
        let mut cursor = 0usize;
        loop {
            let onset = Self::onset(&p, k, rate);
            let next = if onset >= end {
                frames
            } else {
                (onset - start).max(0) as usize
            };
            // Clic en cours jusqu'au prochain temps (ou la fin du bloc).
            if let Some((accent, idx)) = self.active {
                let click = if accent {
                    &self.samples.accent
                } else {
                    &self.samples.normal
                };
                let n = (next - cursor).min(click.len() - idx);
                for (frame, &s) in out[cursor * 2..(cursor + n) * 2]
                    .as_chunks_mut::<2>()
                    .0
                    .iter_mut()
                    .zip(&click[idx..idx + n])
                {
                    let v = s * gain;
                    frame[0] += v;
                    frame[1] += v;
                    peak = peak.max(v.abs());
                }
                self.active = (idx + n < click.len()).then_some((accent, idx + n));
            }
            if onset >= end {
                break;
            }
            if onset >= start && onset >= 0 {
                let accent = k.rem_euclid(p.beats_per_bar.max(1) as i64) == 0;
                self.active = Some((accent, 0));
            }
            cursor = next;
            k += 1;
        }
        peak
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48000;

    fn params(bpm: f64, anchor_ms: f64) -> Option<MetronomeParams> {
        Some(MetronomeParams {
            bpm,
            anchor_ms,
            beats_per_bar: 4,
        })
    }

    /// Rend `frames` frames depuis `start` en blocs de `block` frames.
    fn render(
        m: &mut Metronome,
        start: u64,
        frames: usize,
        block: usize,
        p: Option<MetronomeParams>,
    ) -> Vec<f32> {
        let mut out = vec![0.0; frames * 2];
        let mut pos = start;
        for chunk in out.chunks_mut(block * 2) {
            m.render(chunk, pos, RATE, p, 1.0);
            pos += (chunk.len() / 2) as u64;
        }
        out
    }

    /// Premier frame non nul à partir de `from`.
    fn onset(out: &[f32], from: usize) -> Option<usize> {
        (from..out.len() / 2).find(|&i| out[2 * i] != 0.0)
    }

    #[test]
    fn click_samples_decay_to_silence() {
        let s = ClickSamples::new(RATE);
        assert_eq!(s.accent.len(), 1920);
        assert_eq!(s.normal.len(), 1920);
        assert!(s.accent.last().unwrap().abs() < 1e-3);
        assert!(s.normal.last().unwrap().abs() < 1e-3);
        let peak = |v: &[f32]| v.iter().fold(0.0f32, |p, x| p.max(x.abs()));
        assert!(peak(&s.accent) > peak(&s.normal));
        assert!(peak(&s.normal) > 0.05);
    }

    #[test]
    fn click_lands_on_the_exact_frame() {
        let mut m = Metronome::new(ClickSamples::new(RATE));
        let out = render(&mut m, 0, 48000, 48000, params(120.0, 0.0));
        assert_eq!(onset(&out, 0), Some(0));
        assert_eq!(out[2 * 23999], 0.0);
        assert_eq!(onset(&out, 2000), Some(24000));
        assert_eq!(out[2 * 24000], out[2 * 24000 + 1], "mono dupliqué");
    }

    #[test]
    fn click_is_independent_of_block_size() {
        let p = params(97.0, 13.0);
        let whole = render(
            &mut Metronome::new(ClickSamples::new(RATE)),
            0,
            96000,
            96000,
            p,
        );
        let small = render(
            &mut Metronome::new(ClickSamples::new(RATE)),
            0,
            96000,
            128,
            p,
        );
        let odd = render(
            &mut Metronome::new(ClickSamples::new(RATE)),
            0,
            96000,
            37,
            p,
        );
        assert_eq!(whole, small);
        assert_eq!(whole, odd);
    }

    #[test]
    fn accent_on_first_beat_of_bar() {
        let s = ClickSamples::new(RATE);
        let mut m = Metronome::new(ClickSamples::new(RATE));
        let out = render(&mut m, 0, 48000, 48000, params(120.0, 0.0));
        assert_eq!(out[0], s.accent[0]);
        assert_eq!(out[2 * 24000 + 2], s.normal[1]);
        assert_eq!(out[2], s.accent[1]);
    }

    #[test]
    fn anchor_offsets_grid() {
        // Ancre à 1000 ms, 120 bpm : temps à 0, 500, 1000 ms ; accent seulement à 1000 ms
        // (k = -2, -1 avant l'ancre).
        let s = ClickSamples::new(RATE);
        let mut m = Metronome::new(ClickSamples::new(RATE));
        let out = render(&mut m, 0, 60000, 60000, params(120.0, 1000.0));
        assert_eq!(onset(&out, 0), Some(0));
        assert_eq!(onset(&out, 2000), Some(24000));
        assert_eq!(onset(&out, 26000), Some(48000));
        assert_eq!(out[2], s.normal[1]);
        assert_eq!(out[2 * 24000 + 2], s.normal[1]);
        assert_eq!(out[2 * 48000 + 2], s.accent[1]);
    }

    #[test]
    fn seek_cuts_the_ringing_click() {
        let mut m = Metronome::new(ClickSamples::new(RATE));
        let p = params(120.0, 0.0);
        render(&mut m, 0, 100, 100, p);
        // Saut à 10 000 (aucun temps entre 10 000 et 10 500) : le clic de 0 ne continue pas.
        let out = render(&mut m, 10000, 500, 500, p);
        assert!(out.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn no_click_while_paused() {
        let mut m = Metronome::new(ClickSamples::new(RATE));
        let out = render(&mut m, 0, 48000, 48000, None);
        assert!(out.iter().all(|&x| x == 0.0));
        let out = render(&mut m, 0, 48000, 48000, params(0.0, 0.0));
        assert!(out.iter().all(|&x| x == 0.0), "bpm nul : pas de grille");
    }

    #[test]
    fn gain_scales_the_click() {
        let s = ClickSamples::new(RATE);
        let mut m = Metronome::new(ClickSamples::new(RATE));
        let mut out = vec![0.0; 200];
        let peak = m.render(&mut out, 0, RATE, params(120.0, 0.0), 0.5);
        assert_eq!(out[2], s.accent[1] * 0.5);
        assert!(peak > 0.0);
    }
}
