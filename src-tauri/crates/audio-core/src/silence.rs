// Détection de la fin du silence de tête d'un backing track (ARCHITECTURE.md §7) — portage de
// l'ancien `src/lib/silence-detection.ts`. Pic par fenêtre plutôt que RMS (une attaque brève
// ne se dilue pas sur la fenêtre) et plancher de bruit = percentile bas des fenêtres du début
// plutôt qu'une moyenne sur un préfixe court (qui mesurait le count-in lui-même quand le track
// n'a presque pas de silence avant lui).

pub const WINDOW_MS: f64 = 20.0;
/// Durée du début de fichier balayée pour estimer le plancher de bruit.
pub const NOISE_FLOOR_WINDOW_MS: f64 = 2000.0;
/// Percentile bas des pics de fenêtre = estimation du plancher.
pub const NOISE_FLOOR_PERCENTILE: f64 = 10.0;
/// Seuil d'onset = plancher × ce facteur.
pub const THRESHOLD_MULTIPLIER: f32 = 4.0;
/// Plancher utilisé quand le bruit mesuré est ~0.
pub const ABSOLUTE_FLOOR_DB: f32 = -50.0;
/// On ne cherche jamais d'onset au-delà.
pub const SCAN_CAP_MS: f64 = 15_000.0;
/// Recul pour ne pas couper l'attaque.
pub const ATTACK_MARGIN_MS: f64 = 10.0;

fn window_peak(samples: &[f32], start: usize, end: usize) -> f32 {
    samples[start..end]
        .iter()
        .fold(0.0, |peak, s| peak.max(s.abs()))
}

/// Fin du silence de tête en ms, ou `None` si aucun onset net avant `SCAN_CAP_MS`.
pub fn scan_leading_silence(samples: &[f32], sample_rate: u32) -> Option<f64> {
    let rate = sample_rate as f64;
    let window = (WINDOW_MS / 1000.0 * rate).round() as usize;
    let scan_cap = samples
        .len()
        .min((SCAN_CAP_MS / 1000.0 * rate).round() as usize);
    let window_count = scan_cap.checked_div(window).unwrap_or(0);
    if window_count < 2 {
        return None;
    }

    // Percentile plutôt que moyenne (insensible à un coup au tout début) ou minimum
    // (insensible à un échantillon isolé proche de 0).
    let floor_count = window_count.min((NOISE_FLOOR_WINDOW_MS / WINDOW_MS).round() as usize);
    let mut floor_peaks: Vec<f32> = (0..floor_count)
        .map(|w| window_peak(samples, w * window, (w + 1) * window))
        .collect();
    floor_peaks.sort_by(|a, b| a.total_cmp(b));
    let noise_floor =
        floor_peaks[(NOISE_FLOOR_PERCENTILE / 100.0 * (floor_peaks.len() - 1) as f64) as usize];
    let threshold = (noise_floor * THRESHOLD_MULTIPLIER).max(10f32.powf(ABSOLUTE_FLOOR_DB / 20.0));

    (0..window_count - 1)
        .find(|&w| {
            window_peak(samples, w * window, (w + 1) * window) > threshold
                && window_peak(samples, (w + 1) * window, (w + 2) * window) > threshold
        })
        .map(|w| ((w * window) as f64 * 1000.0 / rate - ATTACK_MARGIN_MS).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    // 1 échantillon = 1 ms à cette fréquence : les valeurs attendues restent exactes et lisibles.
    const SAMPLE_RATE: u32 = 1000;

    fn segment(duration_ms: usize, amplitude: f32) -> Vec<f32> {
        vec![amplitude; duration_ms]
    }

    fn build(segments: &[Vec<f32>]) -> Vec<f32> {
        segments.concat()
    }

    #[test]
    fn detects_a_clean_onset_after_silence() {
        let samples = build(&[segment(500, 0.0), segment(1500, 0.5)]);
        assert_eq!(scan_leading_silence(&samples, SAMPLE_RATE), Some(490.0));
    }

    #[test]
    fn none_when_loud_from_the_first_sample() {
        // Tout le début est fort : le plancher égale le signal, rien n'est « N fois plus fort »,
        // pas d'onset — voulu, l'appelant retombe alors sur 0 ms, le bon résultat.
        let samples = build(&[segment(2000, 0.5)]);
        assert_eq!(scan_leading_silence(&samples, SAMPLE_RATE), None);
    }

    #[test]
    fn none_when_the_whole_window_is_silent() {
        let samples = build(&[segment(2000, 0.0)]);
        assert_eq!(scan_leading_silence(&samples, SAMPLE_RATE), None);
    }

    #[test]
    fn ignores_an_isolated_spike() {
        let mut samples = build(&[segment(500, 0.0), segment(1500, 0.5)]);
        // Clic numérique de 2 échantillons dans le silence, bien avant le vrai onset à 500 ms.
        samples[300] = 1.0;
        samples[301] = 1.0;
        assert_eq!(scan_leading_silence(&samples, SAMPLE_RATE), Some(490.0));
    }

    #[test]
    fn ignores_steady_low_background_noise() {
        let samples = build(&[segment(500, 0.01), segment(1500, 0.5)]);
        assert_eq!(scan_leading_silence(&samples, SAMPLE_RATE), Some(490.0));
    }

    #[test]
    fn none_when_onset_is_beyond_scan_cap() {
        let samples = build(&[segment(16_000, 0.0), segment(1000, 0.5)]);
        assert_eq!(scan_leading_silence(&samples, SAMPLE_RATE), None);
    }

    #[test]
    fn detects_early_quiet_click_before_loud_section() {
        // Régression du vrai bug : count-in presque sans silence, clics faibles, puis le mix
        // complet bien plus fort. Même forme que le fichier réel à 1/40e d'échelle.
        let samples = build(&[
            segment(40, 0.0),
            segment(60, 0.06),
            segment(320, 0.0),
            segment(60, 0.06),
            segment(320, 0.0),
            segment(1200, 0.5),
        ]);
        assert_eq!(scan_leading_silence(&samples, SAMPLE_RATE), Some(30.0));
    }
}
