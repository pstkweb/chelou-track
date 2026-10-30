// Voie guitare (son direct) côté temps réel : entrée mono ajoutée sur les deux canaux de sortie,
// avec gain en rampe et crêtes pour les vumètres. Aucune allocation ni verrou dans `process`.

use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use std::sync::Arc;

use crate::gain::{fader_gain, GainRamp};

struct Shared {
    /// Gain cible (bits d'un `f32`), 0 si muet.
    gain: AtomicU32,
    /// Crêtes depuis la dernière lecture (bits d'un `f32` positif : l'ordre des bits est celui
    /// des valeurs, d'où `fetch_max`).
    input_peak: AtomicU32,
    output_peak: AtomicU32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuitarLevels {
    pub input: f32,
    pub output: f32,
}

/// Côté non temps réel : volume, mute, lecture des crêtes.
pub struct GuitarControl {
    shared: Arc<Shared>,
    fader: f32,
    muted: bool,
}

/// Côté callback audio.
pub struct GuitarLane {
    shared: Arc<Shared>,
    ramp: GainRamp,
}

pub fn guitar_lane() -> (GuitarControl, GuitarLane) {
    let shared = Arc::new(Shared {
        gain: AtomicU32::new(1.0f32.to_bits()),
        input_peak: AtomicU32::new(0),
        output_peak: AtomicU32::new(0),
    });
    (
        GuitarControl {
            shared: shared.clone(),
            fader: 80.0,
            muted: false,
        },
        GuitarLane {
            shared,
            ramp: GainRamp::new(1.0),
        },
    )
}

impl GuitarControl {
    fn publish(&self) {
        let gain = if self.muted {
            0.0
        } else {
            fader_gain(self.fader)
        };
        self.shared.gain.store(gain.to_bits(), Relaxed);
    }

    pub fn set_fader(&mut self, fader: f32) {
        self.fader = fader.clamp(0.0, 100.0);
        self.publish();
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        self.publish();
    }

    /// Crêtes (0-1) depuis la lecture précédente, remises à zéro.
    pub fn levels(&self) -> GuitarLevels {
        let take = |peak: &AtomicU32| f32::from_bits(peak.swap(0, Relaxed));
        GuitarLevels {
            input: take(&self.shared.input_peak),
            output: take(&self.shared.output_peak),
        }
    }
}

impl GuitarLane {
    /// Ajoute `input` (mono) × gain sur les deux canaux de `out` (stéréo entrelacé).
    pub fn process(&mut self, input: &[f32], out: &mut [f32]) {
        self.ramp
            .set_target(f32::from_bits(self.shared.gain.load(Relaxed)));
        let mut in_peak = 0.0f32;
        let mut out_peak = 0.0f32;
        for (frame, &sample) in out.as_chunks_mut::<2>().0.iter_mut().zip(input) {
            let value = sample * self.ramp.tick();
            frame[0] += value;
            frame[1] += value;
            in_peak = in_peak.max(sample.abs());
            out_peak = out_peak.max(value.abs());
        }
        self.shared.input_peak.fetch_max(in_peak.to_bits(), Relaxed);
        self.shared
            .output_peak
            .fetch_max(out_peak.to_bits(), Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(lane: &mut GuitarLane, input: &[f32]) -> Vec<f32> {
        let mut out = vec![0.0; input.len() * 2];
        lane.process(input, &mut out);
        out
    }

    #[test]
    fn process_adds_input_to_both_channels() {
        let (_ctl, mut lane) = guitar_lane();
        let mut out = vec![0.25; 4];
        lane.process(&[0.5, -0.5], &mut out);
        assert_eq!(out, vec![0.75, 0.75, -0.25, -0.25]);
    }

    #[test]
    fn muted_lane_fades_to_silence() {
        let (mut ctl, mut lane) = guitar_lane();
        ctl.set_muted(true);
        let out = run(&mut lane, &[1.0; 256]);
        assert!(out[0] > 0.9, "pas de saut : {}", out[0]);
        assert_eq!(out[2 * 255], 0.0);
        let out = run(&mut lane, &[1.0; 16]);
        assert!(out.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn levels_report_peaks_and_reset() {
        let (ctl, mut lane) = guitar_lane();
        run(&mut lane, &[0.1, -0.5, 0.3]);
        assert_eq!(ctl.levels().input, 0.5);
        assert_eq!(ctl.levels().input, 0.0);
    }

    #[test]
    fn output_level_follows_gain() {
        let (mut ctl, mut lane) = guitar_lane();
        ctl.set_fader(40.0);
        run(&mut lane, &[0.0; 512]); // laisse la rampe atteindre sa cible
        ctl.levels();
        run(&mut lane, &[0.8; 16]);
        let levels = ctl.levels();
        assert_eq!(levels.input, 0.8);
        assert!((levels.output - 0.8 * 0.25).abs() < 1e-6);
    }
}
