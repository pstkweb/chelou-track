// Gain temps réel : rampe sans clic et conversion du fader de l'UI (ARCHITECTURE.md §7).

/// Durée d'une rampe de gain, en frames : évite le clic d'un changement de volume brutal.
const RAMP_FRAMES: f32 = 256.0;

/// Gain qui rejoint sa cible en ligne droite sur `RAMP_FRAMES` frames, quel que soit le découpage
/// en blocs du callback : le pas est fixé une fois, au changement de cible.
pub struct GainRamp {
    current: f32,
    target: f32,
    step: f32,
}

impl GainRamp {
    pub fn new(gain: f32) -> Self {
        GainRamp {
            current: gain,
            target: gain,
            step: 0.0,
        }
    }

    pub fn set_target(&mut self, gain: f32) {
        if gain != self.target {
            self.target = gain;
            self.step = (gain - self.current) / RAMP_FRAMES;
        }
    }

    /// Saute directement à la cible (aucun son ne sort : pas de clic possible).
    pub fn settle(&mut self) {
        self.current = self.target;
    }

    /// Gain de la frame suivante.
    pub fn tick(&mut self) -> f32 {
        if self.current != self.target {
            self.current = if (self.target - self.current).abs() <= self.step.abs() {
                self.target
            } else {
                self.current + self.step
            };
        }
        self.current
    }
}

/// Fader de l'UI (0-100, 80 = 0 dB, `dB = 40·log10(v/80)`) → gain linéaire `(v/80)²`.
pub fn fader_gain(fader: f32) -> f32 {
    if fader <= 0.0 {
        return 0.0;
    }
    let x = fader / 80.0;
    x * x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramp_is_linear_across_small_blocks() {
        let mut ramp = GainRamp::new(1.0);
        ramp.set_target(0.0);
        let values: Vec<f32> = (0..256).map(|_| ramp.tick()).collect();
        // 4 blocs de 64 frames ou une seule boucle : la rampe ne dépend pas du découpage.
        assert!(
            (values[127] - 0.5).abs() < 1e-6,
            "frame 128 : {}",
            values[127]
        );
        assert_eq!(values[255], 0.0);
    }

    #[test]
    fn ramp_reaches_target_exactly() {
        let mut ramp = GainRamp::new(0.3);
        ramp.set_target(0.7);
        for _ in 0..300 {
            ramp.tick();
        }
        assert_eq!(ramp.tick(), 0.7);
    }

    #[test]
    fn fader_gain_unity_at_80() {
        assert_eq!(fader_gain(80.0), 1.0);
        assert_eq!(fader_gain(0.0), 0.0);
        assert_eq!(fader_gain(40.0), 0.25);
        assert_eq!(fader_gain(-5.0), 0.0);
    }
}
