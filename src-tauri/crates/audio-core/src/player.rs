// Lecteur de backing track côté temps réel. `BackingPlayer::render` tourne dans le callback
// audio : aucune allocation, aucun verrou, aucune libération (ARCHITECTURE.md §7). Les ordres
// arrivent par une file lock-free ; un track remplacé repart par une seconde file pour être
// libéré côté `PlayerControl`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::Arc;

use crate::decode::Pcm;

/// Durée de la rampe de gain, en frames : évite le clic d'un changement de volume brutal.
const GAIN_RAMP_FRAMES: f32 = 256.0;

enum Command {
    Load(Arc<Pcm>),
    Unload,
    Play,
    Pause,
    Seek(u64),
    Gain(f32),
}

#[derive(Default)]
struct Shared {
    position: AtomicU64,
    duration: AtomicU64,
    playing: AtomicBool,
    ended: AtomicBool,
    loaded: AtomicBool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerStatus {
    pub position_ms: f64,
    pub duration_ms: f64,
    pub playing: bool,
    pub ended: bool,
    pub loaded: bool,
}

/// Numéro du dernier chargement demandé : un chargement lent qui se termine après un plus
/// récent (ou après un déchargement) ne doit pas remplacer le track en place.
pub struct LoadGeneration(AtomicU64);

impl LoadGeneration {
    pub const fn new() -> Self {
        LoadGeneration(AtomicU64::new(0))
    }

    /// Ouvre une nouvelle génération et invalide toutes les précédentes.
    pub fn next(&self) -> u64 {
        self.0.fetch_add(1, Relaxed) + 1
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.0.load(Relaxed) == generation
    }
}

impl Default for LoadGeneration {
    fn default() -> Self {
        Self::new()
    }
}

/// Côté non temps réel : envoie les ordres, lit le statut, libère les tracks remplacés.
pub struct PlayerControl {
    commands: rtrb::Producer<Command>,
    garbage: rtrb::Consumer<Arc<Pcm>>,
    shared: Arc<Shared>,
    sample_rate: u32,
}

/// Côté callback audio.
pub struct BackingPlayer {
    commands: rtrb::Consumer<Command>,
    garbage: rtrb::Producer<Arc<Pcm>>,
    shared: Arc<Shared>,
    track: Option<Arc<Pcm>>,
    position: usize,
    playing: bool,
    gain: f32,
    target_gain: f32,
}

pub fn backing_player(sample_rate: u32) -> (PlayerControl, BackingPlayer) {
    let (cmd_tx, cmd_rx) = rtrb::RingBuffer::new(64);
    let (gc_tx, gc_rx) = rtrb::RingBuffer::new(64);
    let shared = Arc::new(Shared::default());
    (
        PlayerControl {
            commands: cmd_tx,
            garbage: gc_rx,
            shared: shared.clone(),
            sample_rate,
        },
        BackingPlayer {
            commands: cmd_rx,
            garbage: gc_tx,
            shared,
            track: None,
            position: 0,
            playing: false,
            gain: 1.0,
            target_gain: 1.0,
        },
    )
}

impl PlayerControl {
    fn send(&mut self, cmd: Command) {
        // File pleine = 64 ordres non consommés : le flux est arrêté, l'ordre est sans objet.
        let _ = self.commands.push(cmd);
    }

    /// Refuse (`false`) un track rééchantillonné pour une autre fréquence que celle de la sortie
    /// (elle a changé pendant le chargement) : il jouerait à la mauvaise hauteur et vitesse.
    pub fn load(&mut self, pcm: Arc<Pcm>) -> bool {
        if pcm.sample_rate != self.sample_rate {
            return false;
        }
        self.send(Command::Load(pcm));
        true
    }

    pub fn unload(&mut self) {
        self.send(Command::Unload);
    }

    pub fn play(&mut self) {
        self.send(Command::Play);
    }

    pub fn pause(&mut self) {
        self.send(Command::Pause);
    }

    pub fn seek_ms(&mut self, ms: f64) {
        let frames = (ms.max(0.0) * self.sample_rate as f64 / 1000.0).round() as u64;
        self.send(Command::Seek(frames));
    }

    pub fn set_gain(&mut self, gain: f32) {
        self.send(Command::Gain(gain.clamp(0.0, 4.0)));
    }

    pub fn status(&self) -> PlayerStatus {
        let to_ms = |frames: u64| frames as f64 * 1000.0 / self.sample_rate as f64;
        PlayerStatus {
            position_ms: to_ms(self.shared.position.load(Relaxed)),
            duration_ms: to_ms(self.shared.duration.load(Relaxed)),
            playing: self.shared.playing.load(Relaxed),
            ended: self.shared.ended.load(Relaxed),
            loaded: self.shared.loaded.load(Relaxed),
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Libère, hors thread audio, les tracks que le callback a remplacés.
    pub fn collect_garbage(&mut self) {
        while self.garbage.pop().is_ok() {}
    }
}

impl BackingPlayer {
    fn retire(&mut self, old: Option<Arc<Pcm>>) {
        if let Some(old) = old {
            if let Err(rtrb::PushError::Full(old)) = self.garbage.push(old) {
                // Jamais libérer dans le thread audio : on fuit plutôt (64 tracks non collectés).
                std::mem::forget(old);
            }
        }
    }

    fn apply_commands(&mut self) {
        while let Ok(cmd) = self.commands.pop() {
            match cmd {
                Command::Load(pcm) => {
                    self.shared.duration.store(pcm.frames() as u64, Relaxed);
                    let old = self.track.replace(pcm);
                    self.retire(old);
                    self.position = 0;
                    self.playing = false;
                    self.shared.ended.store(false, Relaxed);
                }
                Command::Unload => {
                    let old = self.track.take();
                    self.retire(old);
                    self.position = 0;
                    self.playing = false;
                    self.shared.duration.store(0, Relaxed);
                }
                Command::Play => {
                    if let Some(track) = &self.track {
                        if self.position >= track.frames() {
                            self.position = 0;
                        }
                        self.playing = true;
                        self.shared.ended.store(false, Relaxed);
                    }
                }
                Command::Pause => self.playing = false,
                Command::Seek(frames) => {
                    let len = self.track.as_ref().map_or(0, |t| t.frames());
                    self.position = (frames as usize).min(len);
                    self.shared.ended.store(false, Relaxed);
                }
                Command::Gain(g) => self.target_gain = g,
            }
        }
    }

    /// Ajoute le track dans `out` (stéréo entrelacé) — d'autres voies y seront mixées.
    pub fn render(&mut self, out: &mut [f32]) {
        self.apply_commands();
        if let (true, Some(track)) = (self.playing, self.track.as_ref()) {
            let len = track.frames();
            let step = (self.target_gain - self.gain) / GAIN_RAMP_FRAMES;
            for frame in out.as_chunks_mut::<2>().0 {
                if self.position >= len {
                    self.playing = false;
                    self.shared.ended.store(true, Relaxed);
                    break;
                }
                if self.gain != self.target_gain {
                    self.gain = if (self.target_gain - self.gain).abs() <= step.abs() {
                        self.target_gain
                    } else {
                        self.gain + step
                    };
                }
                frame[0] += track.samples[2 * self.position] * self.gain;
                frame[1] += track.samples[2 * self.position + 1] * self.gain;
                self.position += 1;
            }
        } else {
            self.gain = self.target_gain;
        }
        self.shared.position.store(self.position as u64, Relaxed);
        self.shared.playing.store(self.playing, Relaxed);
        self.shared.loaded.store(self.track.is_some(), Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::Pcm;
    use std::sync::Arc;

    const SR: u32 = 1000; // 1 frame = 1 ms

    /// Track dont la frame i vaut i/1000 sur les deux canaux.
    fn ramp(frames: usize) -> Arc<Pcm> {
        let samples = (0..frames).flat_map(|i| [i as f32 / 1000.0; 2]).collect();
        Arc::new(Pcm {
            samples,
            sample_rate: SR,
        })
    }

    fn render(player: &mut BackingPlayer, frames: usize, fill: f32) -> Vec<f32> {
        let mut out = vec![fill; frames * 2];
        player.render(&mut out);
        out
    }

    #[test]
    fn render_paused_adds_nothing() {
        let (mut ctl, mut player) = backing_player(SR);
        ctl.load(ramp(1000));
        let out = render(&mut player, 128, 0.0);
        assert!(out.iter().all(|&s| s == 0.0));
        assert_eq!(ctl.status().position_ms, 0.0);
    }

    #[test]
    fn render_plays_and_advances() {
        let (mut ctl, mut player) = backing_player(SR);
        ctl.load(ramp(1000));
        ctl.play();
        let out = render(&mut player, 128, 0.0);
        assert_eq!(out[2 * 10], 0.010);
        assert_eq!(out[2 * 127 + 1], 0.127);
        assert_eq!(ctl.status().position_ms, 128.0);
        assert!(ctl.status().playing);
    }

    #[test]
    fn render_adds_into_buffer() {
        let (mut ctl, mut player) = backing_player(SR);
        ctl.load(ramp(1000));
        ctl.play();
        let out = render(&mut player, 16, 0.25);
        assert_eq!(out[2 * 10], 0.25 + 0.010);
    }

    #[test]
    fn seek_moves_position() {
        let (mut ctl, mut player) = backing_player(SR);
        ctl.load(ramp(1000));
        ctl.seek_ms(500.0);
        ctl.play();
        let out = render(&mut player, 4, 0.0);
        assert_eq!(out[0], 0.5);
        assert_eq!(ctl.status().position_ms, 504.0);
    }

    #[test]
    fn seek_is_clamped_to_track() {
        let (mut ctl, mut player) = backing_player(SR);
        ctl.load(ramp(100));
        ctl.seek_ms(10_000.0);
        render(&mut player, 1, 0.0);
        assert_eq!(ctl.status().position_ms, 100.0);
    }

    #[test]
    fn end_of_track_stops_and_flags_ended() {
        let (mut ctl, mut player) = backing_player(SR);
        ctl.load(ramp(100));
        ctl.play();
        let out = render(&mut player, 128, 0.0);
        assert_eq!(out[2 * 99], 0.099);
        assert!(out[2 * 100..].iter().all(|&s| s == 0.0));
        let st = ctl.status();
        assert!(!st.playing);
        assert!(st.ended);
        assert_eq!(st.position_ms, 100.0);
    }

    #[test]
    fn play_after_end_restarts_from_zero() {
        let (mut ctl, mut player) = backing_player(SR);
        ctl.load(ramp(100));
        ctl.play();
        render(&mut player, 128, 0.0);
        ctl.play();
        let out = render(&mut player, 4, 0.0);
        assert_eq!(out[2], 0.001);
        assert!(!ctl.status().ended);
    }

    #[test]
    fn load_replaces_track_and_resets() {
        let (mut ctl, mut player) = backing_player(SR);
        let first = ramp(1000);
        ctl.load(first.clone());
        ctl.play();
        render(&mut player, 128, 0.0);
        ctl.load(ramp(500));
        render(&mut player, 1, 0.0);
        let st = ctl.status();
        assert_eq!(st.position_ms, 0.0);
        assert!(!st.playing);
        assert_eq!(st.duration_ms, 500.0);
        ctl.collect_garbage();
        assert_eq!(Arc::strong_count(&first), 1);
    }

    #[test]
    fn unload_releases_track() {
        let (mut ctl, mut player) = backing_player(SR);
        let first = ramp(10);
        ctl.load(first.clone());
        render(&mut player, 1, 0.0);
        ctl.unload();
        render(&mut player, 1, 0.0);
        ctl.collect_garbage();
        assert_eq!(Arc::strong_count(&first), 1);
        assert!(!ctl.status().loaded);
    }

    #[test]
    fn gain_ramps_without_jump() {
        let (mut ctl, mut player) = backing_player(SR);
        ctl.load(Arc::new(Pcm {
            samples: vec![1.0; 2000],
            sample_rate: SR,
        }));
        ctl.play();
        render(&mut player, 16, 0.0);
        ctl.set_gain(0.0);
        let out = render(&mut player, 256, 0.0);
        assert!(out[0] > 0.9, "premier échantillon {}", out[0]);
        assert!(out[2 * 255] < 0.01, "dernier échantillon {}", out[2 * 255]);
    }

    #[test]
    fn load_rejects_other_sample_rate() {
        let (mut ctl, mut player) = backing_player(48000);
        let wrong = Arc::new(Pcm {
            samples: vec![0.0; 20],
            sample_rate: 44100,
        });
        assert!(!ctl.load(wrong));
        render(&mut player, 1, 0.0);
        assert!(!ctl.status().loaded);
        let right = Arc::new(Pcm {
            samples: vec![0.0; 20],
            sample_rate: 48000,
        });
        assert!(ctl.load(right));
    }

    #[test]
    fn newer_load_invalidates_older() {
        let generation = LoadGeneration::new();
        let first = generation.next();
        assert!(generation.is_current(first));
        let second = generation.next();
        assert!(!generation.is_current(first));
        assert!(generation.is_current(second));
    }

    #[test]
    fn status_reports_duration_and_loaded() {
        let (mut ctl, mut player) = backing_player(SR);
        assert!(!ctl.status().loaded);
        ctl.load(ramp(250));
        render(&mut player, 1, 0.0);
        let st = ctl.status();
        assert!(st.loaded);
        assert_eq!(st.duration_ms, 250.0);
    }
}
