// Mix de la vue tablature (spec §5.8) : volumes et mutes du backing et du clic, grille du
// métronome, crêtes pour les vumètres. État partagé en atomiques, possédé par le moteur : il
// survit aux lecteurs recréés à une autre fréquence, que chaque `BackingPlayer` lit à chaque bloc.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::Arc;

use crate::gain::fader_gain;
pub use crate::metronome::MetronomeParams;

struct MixShared {
    /// Gains linéaires (bits d'un `f32`), 0 si muet.
    backing_gain: AtomicU32,
    click_gain: AtomicU32,
    click_muted: AtomicBool,
    /// Grille du métronome (`bpm` à 0 : aucune) ; `f64` en bits.
    bpm: AtomicU64,
    anchor_ms: AtomicU64,
    beats_per_bar: AtomicU32,
    /// Crêtes depuis la dernière lecture (cf. `guitar.rs`).
    backing_peak: AtomicU32,
    click_peak: AtomicU32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MixLevels {
    pub backing: f32,
    pub click: f32,
}

/// Réglages du mix et lecture des crêtes, côté non temps réel comme côté lecteur.
#[derive(Clone)]
pub struct MixControl {
    shared: Arc<MixShared>,
}

/// Défauts : backing et clic à 0 dB, clic actif, pas de grille.
pub fn mix_state() -> MixControl {
    MixControl {
        shared: Arc::new(MixShared {
            backing_gain: AtomicU32::new(1.0f32.to_bits()),
            click_gain: AtomicU32::new(1.0f32.to_bits()),
            click_muted: AtomicBool::new(false),
            bpm: AtomicU64::new(0),
            anchor_ms: AtomicU64::new(0),
            beats_per_bar: AtomicU32::new(4),
            backing_peak: AtomicU32::new(0),
            click_peak: AtomicU32::new(0),
        }),
    }
}

fn gain_bits(fader: f32, muted: bool) -> u32 {
    let gain = if muted {
        0.0
    } else {
        fader_gain(fader.clamp(0.0, 100.0))
    };
    gain.to_bits()
}

impl MixControl {
    pub fn set_backing(&self, fader: f32, muted: bool) {
        self.shared
            .backing_gain
            .store(gain_bits(fader, muted), Relaxed);
    }

    /// Clic coupé = métronome éteint (maquette `TabMixer`).
    pub fn set_click(&self, fader: f32, muted: bool) {
        self.shared
            .click_gain
            .store(gain_bits(fader, false), Relaxed);
        self.shared.click_muted.store(muted, Relaxed);
    }

    /// Grille du track en cours (`None` : tempo inconnu). Les champs sont écrits l'un après
    /// l'autre : un bloc peut voir un mélange de l'ancienne et de la nouvelle grille, au pire un
    /// clic décalé au moment du changement.
    pub fn set_metronome(&self, params: Option<MetronomeParams>) {
        let s = &self.shared;
        match params {
            Some(p) => {
                s.anchor_ms.store(p.anchor_ms.to_bits(), Relaxed);
                s.beats_per_bar.store(p.beats_per_bar, Relaxed);
                s.bpm.store(p.bpm.to_bits(), Relaxed);
            }
            None => s.bpm.store(0, Relaxed),
        }
    }

    /// Crêtes (0-1) depuis la lecture précédente, remises à zéro.
    pub fn levels(&self) -> MixLevels {
        let take = |peak: &AtomicU32| f32::from_bits(peak.swap(0, Relaxed));
        MixLevels {
            backing: take(&self.shared.backing_peak),
            click: take(&self.shared.click_peak),
        }
    }

    pub(crate) fn backing_gain(&self) -> f32 {
        f32::from_bits(self.shared.backing_gain.load(Relaxed))
    }

    pub(crate) fn click_gain(&self) -> f32 {
        f32::from_bits(self.shared.click_gain.load(Relaxed))
    }

    /// Grille à jouer : `None` si le clic est coupé ou le tempo inconnu.
    pub(crate) fn metronome(&self) -> Option<MetronomeParams> {
        let s = &self.shared;
        let bpm = f64::from_bits(s.bpm.load(Relaxed));
        (!s.click_muted.load(Relaxed) && bpm > 0.0).then(|| MetronomeParams {
            bpm,
            anchor_ms: f64::from_bits(s.anchor_ms.load(Relaxed)),
            beats_per_bar: s.beats_per_bar.load(Relaxed),
        })
    }

    pub(crate) fn publish_peaks(&self, backing: f32, click: f32) {
        self.shared
            .backing_peak
            .fetch_max(backing.to_bits(), Relaxed);
        self.shared.click_peak.fetch_max(click.to_bits(), Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::Pcm;
    use crate::player::backing_player;
    use std::sync::Arc;

    const SR: u32 = 48000;

    fn ones(frames: usize) -> Arc<Pcm> {
        Arc::new(Pcm {
            samples: vec![1.0; frames * 2],
            sample_rate: SR,
        })
    }

    fn grid() -> Option<MetronomeParams> {
        Some(MetronomeParams {
            bpm: 120.0,
            anchor_ms: 0.0,
            beats_per_bar: 4,
        })
    }

    #[test]
    fn backing_fader_and_mute_apply() {
        let mix = mix_state();
        let (mut ctl, mut player) = backing_player(SR, &mix);
        ctl.load(ones(4000));
        ctl.play();
        mix.set_backing(40.0, false);
        let mut out = vec![0.0; 1024];
        player.render(&mut out); // rampe
        let mut out = vec![0.0; 16];
        player.render(&mut out);
        assert!((out[0] - 0.25).abs() < 1e-6, "{}", out[0]);
        mix.set_backing(40.0, true);
        let mut out = vec![0.0; 1024];
        player.render(&mut out);
        let mut out = vec![0.0; 16];
        player.render(&mut out);
        assert_eq!(out[0], 0.0);
    }

    #[test]
    fn mix_survives_a_new_player() {
        let mix = mix_state();
        mix.set_backing(40.0, false);
        mix.set_metronome(grid());
        let (_old, _) = backing_player(SR, &mix);
        // Nouvelle fréquence de sortie : nouveau lecteur, mêmes réglages.
        let (mut ctl, mut player) = backing_player(SR, &mix);
        ctl.load(Arc::new(Pcm {
            samples: vec![0.0; 96000],
            sample_rate: SR,
        }));
        ctl.play();
        let mut out = vec![0.0; 2 * 100];
        player.render(&mut out);
        assert!(out[2] != 0.0, "métronome réglé avant le lecteur");
        mix.set_metronome(None); // seul le backing dans la suite
        let (mut ctl, mut player) = backing_player(SR, &mix);
        ctl.load(ones(100));
        ctl.play();
        let mut out = vec![0.0; 2 * 10];
        player.render(&mut out);
        assert!(
            (out[0] - 0.25).abs() < 1e-6,
            "fader réglé avant le lecteur : {}",
            out[0]
        );
    }

    #[test]
    fn muted_click_is_silent() {
        let mix = mix_state();
        mix.set_metronome(grid());
        mix.set_click(80.0, true);
        let (mut ctl, mut player) = backing_player(SR, &mix);
        ctl.load(Arc::new(Pcm {
            samples: vec![0.0; 4000],
            sample_rate: SR,
        }));
        ctl.play();
        let mut out = vec![0.0; 2 * 1000];
        player.render(&mut out);
        assert!(out.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn levels_report_backing_and_click_peaks() {
        let mix = mix_state();
        mix.set_backing(40.0, false);
        mix.set_metronome(grid());
        let (mut ctl, mut player) = backing_player(SR, &mix);
        let mut samples = vec![0.5; 4000];
        samples[0] = 0.0;
        ctl.load(Arc::new(Pcm {
            samples,
            sample_rate: SR,
        }));
        ctl.play();
        let mut out = vec![0.0; 2 * 1000];
        player.render(&mut out);
        let levels = mix.levels();
        assert!(levels.backing > 0.0 && levels.backing <= 0.5);
        assert!(levels.click > 0.05);
        assert_eq!(
            mix.levels(),
            MixLevels {
                backing: 0.0,
                click: 0.0
            }
        );
    }
}
