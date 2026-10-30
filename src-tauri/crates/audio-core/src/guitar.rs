// Voie guitare côté temps réel : entrée mono passée dans le plugin d'ampli s'il y en a un (sinon
// son direct, dupliquée sur les deux canaux), ajoutée à la sortie avec gain en rampe, et crêtes
// pour les vumètres. Aucune allocation ni verrou dans `process`.

use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use std::sync::Arc;

use crate::gain::{fader_gain, GainRamp};
use crate::MAX_BLOCK;

/// Effet inséré sur la voie guitare (plugin VST3), appelé depuis le callback audio.
pub trait GuitarFx: Send {
    /// `input` mono (au plus `MAX_BLOCK` frames) ; `out` stéréo entrelacé du même nombre de
    /// frames, **écrasé**.
    fn process(&mut self, input: &[f32], out: &mut [f32]);
}

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
    fx: Option<Box<dyn GuitarFx>>,
    /// Fréquence des flux qui jouent la voie (0 : aucun encore) ; un effet préparé pour une autre
    /// fréquence n'y est jamais installé.
    rate: u32,
    /// Sortie de l'effet, pré-allouée (stéréo, `MAX_BLOCK` frames).
    wet: Vec<f32>,
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
            fx: None,
            rate: 0,
            wet: vec![0.0; MAX_BLOCK * 2],
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
    /// Installe `fx`, préparé pour `rate`, et rend le précédent (libéré par l'appelant, jamais
    /// dans le callback). Les flux tournent à une autre fréquence (rouverts pendant que le plugin
    /// se chargeait) : `fx` est refusé et rendu, l'appelant le recharge.
    pub fn install_fx(
        &mut self,
        fx: Box<dyn GuitarFx>,
        rate: u32,
    ) -> Result<Option<Box<dyn GuitarFx>>, Box<dyn GuitarFx>> {
        if rate != self.rate {
            return Err(fx);
        }
        Ok(self.fx.replace(fx))
    }

    /// Retire l'effet (son direct) et le rend pour être libéré par l'appelant.
    pub fn take_fx(&mut self) -> Option<Box<dyn GuitarFx>> {
        self.fx.take()
    }

    /// Flux sur le point de s'ouvrir à `rate`. Autre fréquence que les précédents : l'effet,
    /// préparé pour l'ancienne, est retiré (rendu pour être libéré hors callback) ; son
    /// propriétaire le recharge à la nouvelle.
    pub fn set_stream_rate(&mut self, rate: u32) -> Option<Box<dyn GuitarFx>> {
        if std::mem::replace(&mut self.rate, rate) == rate {
            None
        } else {
            self.fx.take()
        }
    }

    /// Ajoute `input` (mono), passé dans l'effet s'il y en a un, × gain sur `out` (stéréo
    /// entrelacé, au plus `MAX_BLOCK` frames).
    pub fn process(&mut self, input: &[f32], out: &mut [f32]) {
        self.ramp
            .set_target(f32::from_bits(self.shared.gain.load(Relaxed)));
        let in_peak = input.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
        let mut out_peak = 0.0f32;
        let frames = input.len().min(out.len() / 2);
        if let Some(fx) = &mut self.fx {
            let wet = &mut self.wet[..frames * 2];
            fx.process(&input[..frames], wet);
            for (frame, w) in out
                .as_chunks_mut::<2>()
                .0
                .iter_mut()
                .zip(wet.as_chunks::<2>().0)
            {
                let gain = self.ramp.tick();
                let (l, r) = (w[0] * gain, w[1] * gain);
                frame[0] += l;
                frame[1] += r;
                out_peak = out_peak.max(l.abs()).max(r.abs());
            }
        } else {
            for (frame, &sample) in out.as_chunks_mut::<2>().0.iter_mut().zip(input) {
                let value = sample * self.ramp.tick();
                frame[0] += value;
                frame[1] += value;
                out_peak = out_peak.max(value.abs());
            }
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

    /// Effet factice : écrit une valeur fixe sur chaque canal.
    struct Fixed(f32, f32);
    impl GuitarFx for Fixed {
        fn process(&mut self, _input: &[f32], out: &mut [f32]) {
            for frame in out.as_chunks_mut::<2>().0 {
                *frame = [self.0, self.1];
            }
        }
    }

    /// Installe `fx` sur une voie neuve (fréquence des flux : 0, aucun flux encore).
    fn with_fx(lane: &mut GuitarLane, fx: Fixed) {
        assert!(lane.install_fx(Box::new(fx), 0).is_ok());
    }

    #[test]
    fn fx_output_replaces_direct_sound() {
        let (_ctl, mut lane) = guitar_lane();
        with_fx(&mut lane, Fixed(0.25, -0.25));
        let mut out = vec![0.5; 4];
        lane.process(&[0.9, 0.9], &mut out);
        assert_eq!(out, vec![0.75, 0.25, 0.75, 0.25]);
    }

    #[test]
    fn input_peak_is_measured_before_fx() {
        let (ctl, mut lane) = guitar_lane();
        with_fx(&mut lane, Fixed(0.1, 0.1));
        run(&mut lane, &[0.6, -0.7]);
        let levels = ctl.levels();
        assert_eq!(levels.input, 0.7);
        assert!((levels.output - 0.1).abs() < 1e-6);
    }

    #[test]
    fn output_peak_follows_fx_and_gain() {
        let (mut ctl, mut lane) = guitar_lane();
        with_fx(&mut lane, Fixed(0.8, -0.4));
        ctl.set_fader(40.0);
        run(&mut lane, &[0.0; 512]);
        ctl.levels();
        let out = run(&mut lane, &[0.0; 16]);
        assert!((out[0] - 0.2).abs() < 1e-6);
        assert!((ctl.levels().output - 0.2).abs() < 1e-6);
    }

    #[test]
    fn same_stream_rate_keeps_fx() {
        let (_ctl, mut lane) = guitar_lane();
        assert!(lane.set_stream_rate(48000).is_none());
        assert!(lane.install_fx(Box::new(Fixed(0.0, 0.0)), 48000).is_ok());
        assert!(lane.set_stream_rate(48000).is_none());
        assert!(lane.take_fx().is_some());
    }

    #[test]
    fn stream_rate_change_clears_fx() {
        let (_ctl, mut lane) = guitar_lane();
        lane.set_stream_rate(48000);
        assert!(lane.install_fx(Box::new(Fixed(0.0, 0.0)), 48000).is_ok());
        assert!(lane.set_stream_rate(44100).is_some());
        assert!(lane.take_fx().is_none());
    }

    #[test]
    fn fx_prepared_for_another_rate_is_refused() {
        // Plugin préparé à 48 kHz pendant que les flux ont été rouverts à 44,1 kHz : il ne doit
        // pas être joué, l'appelant le recharge.
        let (_ctl, mut lane) = guitar_lane();
        lane.set_stream_rate(44100);
        assert!(lane.install_fx(Box::new(Fixed(0.0, 0.0)), 48000).is_err());
        assert!(lane.take_fx().is_none());
    }

    #[test]
    fn install_fx_returns_previous() {
        let (_ctl, mut lane) = guitar_lane();
        assert!(matches!(
            lane.install_fx(Box::new(Fixed(0.0, 0.0)), 0),
            Ok(None)
        ));
        assert!(matches!(
            lane.install_fx(Box::new(Fixed(0.0, 0.0)), 0),
            Ok(Some(_))
        ));
        assert!(lane.take_fx().is_some());
        assert!(lane.take_fx().is_none());
    }
}
