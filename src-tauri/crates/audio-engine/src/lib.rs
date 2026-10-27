//! Moteur audio natif : possède le périphérique de sortie et y joue le `BackingPlayer`
//! (ARCHITECTURE.md §7). Dépend de `cpal` (ALSA sur Linux) : pas de tests unitaires ici, la
//! logique testable vit dans `chelou-audio-core`.
//!
//! Mode sortie seule : sortie par défaut de l'hôte par défaut (WASAPI partagé sur Windows),
//! fréquence et buffer imposés par le système. Si le périphérique disparaît, le thread moteur
//! retente chaque seconde d'ouvrir la nouvelle sortie par défaut.

use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chelou_audio_core::player::{backing_player, BackingPlayer, PlayerControl};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

/// Taille max d'un bloc de mixage, en frames : un callback plus long est traité en plusieurs fois.
const MIX_FRAMES: usize = 4096;
/// Sortie forcée au silence après l'ouverture : absorbe l'à-coup de démarrage WASAPI.
const WARMUP_MS: u32 = 100;
const RETRY_DELAY: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq)]
pub enum EngineState {
    Running,
    Error(String),
}

struct Inner {
    control: Mutex<PlayerControl>,
    state: Mutex<EngineState>,
}

/// Poignée partagée (`Send + Sync`) — à placer dans l'état de l'application.
pub struct Engine {
    inner: Arc<Inner>,
}

impl Engine {
    pub fn sample_rate(&self) -> u32 {
        self.backing().sample_rate()
    }

    pub fn backing(&self) -> MutexGuard<'_, PlayerControl> {
        self.inner.control.lock().unwrap()
    }

    pub fn state(&self) -> EngineState {
        self.inner.state.lock().unwrap().clone()
    }
}

/// Ouvre la sortie par défaut et lance le thread propriétaire du flux (`cpal::Stream` n'est pas
/// `Send` partout). Échoue si aucune sortie n'est disponible au démarrage.
pub fn start() -> Result<Engine> {
    let (ready_tx, ready_rx) = mpsc::channel::<Result<Arc<Inner>>>();
    std::thread::Builder::new()
        .name("chelou-audio".into())
        .spawn(move || engine_thread(ready_tx))
        .context("lancement du thread audio")?;
    let inner = ready_rx.recv().context("thread audio arrêté")??;
    Ok(Engine { inner })
}

/// Le callback audio est l'unique utilisateur du lecteur ; le `Mutex` le rend partageable
/// (`Sync`) entre le thread moteur, qui le crée, et le thread du flux, qui le joue.
type SharedPlayer = Arc<Mutex<BackingPlayer>>;

fn engine_thread(ready: mpsc::Sender<Result<Arc<Inner>>>) {
    let (err_tx, err_rx) = mpsc::channel::<String>();
    let opened = default_output().and_then(|(device, config)| {
        let (control, player) = backing_player(config.sample_rate());
        let player = Arc::new(Mutex::new(player));
        let stream = build(&device, &config, player.clone(), err_tx.clone())?;
        Ok((stream, player, control, config.sample_rate()))
    });
    let (stream, mut player, control, mut rate) = match opened {
        Ok(opened) => opened,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let inner = Arc::new(Inner {
        control: Mutex::new(control),
        state: Mutex::new(EngineState::Running),
    });
    let _ = ready.send(Ok(inner.clone()));
    let mut stream = Some(stream);

    // Bloque jusqu'à une erreur de flux (périphérique débranché, changé…), puis retente.
    while let Ok(err) = err_rx.recv() {
        drop(stream.take());
        *inner.state.lock().unwrap() = EngineState::Error(err);
        // Premier essai immédiat : sur un changement de sortie par défaut (WASAPI), la
        // remplaçante existe déjà — inutile d'imposer une seconde de silence.
        let mut first_try = true;
        while stream.is_none() {
            if !first_try {
                std::thread::sleep(RETRY_DELAY);
            }
            first_try = false;
            // Les erreurs accumulées concernaient l'ancien flux.
            while err_rx.try_recv().is_ok() {}
            match reopen(&err_tx, &mut player, &mut rate, &inner) {
                Ok(s) => stream = Some(s),
                Err(e) => *inner.state.lock().unwrap() = EngineState::Error(format!("{e:#}")),
            }
        }
        *inner.state.lock().unwrap() = EngineState::Running;
    }
}

/// Rouvre la sortie par défaut. Si sa fréquence a changé, le lecteur est recréé : le track
/// chargé est perdu, la vue le recharge en voyant la nouvelle fréquence.
fn reopen(
    err_tx: &mpsc::Sender<String>,
    player: &mut SharedPlayer,
    rate: &mut u32,
    inner: &Inner,
) -> Result<cpal::Stream> {
    let (device, config) = default_output()?;
    if config.sample_rate() != *rate {
        let (control, p) = backing_player(config.sample_rate());
        *player = Arc::new(Mutex::new(p));
        *inner.control.lock().unwrap() = control;
        *rate = config.sample_rate();
    }
    build(&device, &config, player.clone(), err_tx.clone())
}

/// cpal signale aussi par le callback d'erreur des évènements dont le flux se remet seul
/// (underrun ALSA, surcharge CoreAudio, changement de route suivi, priorité temps réel
/// refusée) : les traiter comme une perte de périphérique transformerait un accroc de 10 ms
/// en plus d'une seconde de silence.
fn requires_rebuild(kind: cpal::ErrorKind) -> bool {
    !matches!(
        kind,
        cpal::ErrorKind::Xrun | cpal::ErrorKind::DeviceChanged | cpal::ErrorKind::RealtimeDenied
    )
}

fn default_output() -> Result<(cpal::Device, cpal::SupportedStreamConfig)> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or_else(|| anyhow!("Aucune sortie audio disponible"))?;
    let config = device
        .default_output_config()
        .context("configuration de la sortie audio")?;
    Ok((device, config))
}

fn build(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    player: SharedPlayer,
    err_tx: mpsc::Sender<String>,
) -> Result<cpal::Stream> {
    let stream = match config.sample_format() {
        SampleFormat::F32 => build_typed::<f32>(device, config, player, err_tx)?,
        SampleFormat::I16 => build_typed::<i16>(device, config, player, err_tx)?,
        SampleFormat::I32 => build_typed::<i32>(device, config, player, err_tx)?,
        f => return Err(anyhow!("format de sortie non géré : {f:?}")),
    };
    stream.play().context("démarrage de la sortie audio")?;
    Ok(stream)
}

fn build_typed<T>(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    player: SharedPlayer,
    err_tx: mpsc::Sender<String>,
) -> Result<cpal::Stream>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels() as usize;
    let mut warmup = (config.sample_rate() * WARMUP_MS / 1000) as usize;
    // Pré-alloué : le callback ne fait aucune allocation.
    let mut mix = vec![0.0f32; MIX_FRAMES * 2];
    let stream = device.build_output_stream(
        config.config(),
        move |data: &mut [T], _| {
            for block in data.chunks_mut(MIX_FRAMES * channels) {
                let frames = block.len() / channels;
                let mix = &mut mix[..frames * 2];
                mix.fill(0.0);
                if warmup > 0 {
                    warmup = warmup.saturating_sub(frames);
                } else if let Ok(mut p) = player.try_lock() {
                    // Seul ce callback verrouille le lecteur (le mutex ne sert qu'à le partager
                    // entre threads via l'Arc ; une réouverture crée un nouveau lecteur au lieu de
                    // verrouiller l'ancien) : try_lock n'échoue donc jamais en pratique. S'il
                    // échouait, ce bloc sortirait en silence plutôt que d'attendre un verrou.
                    p.render(mix);
                }
                for (out, st) in block.chunks_exact_mut(channels).zip(mix.as_chunks::<2>().0) {
                    if channels == 1 {
                        out[0] = T::from_sample((st[0] + st[1]) * 0.5);
                    } else {
                        out[0] = T::from_sample(st[0]);
                        out[1] = T::from_sample(st[1]);
                        for o in &mut out[2..] {
                            *o = T::from_sample(0.0);
                        }
                    }
                }
            }
        },
        move |e| {
            if requires_rebuild(e.kind()) {
                let _ = err_tx.send(e.to_string());
            }
        },
        None,
    )?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cpal::ErrorKind;

    #[test]
    fn recoverable_errors_keep_the_stream() {
        for kind in [
            ErrorKind::Xrun,
            ErrorKind::DeviceChanged,
            ErrorKind::RealtimeDenied,
        ] {
            assert!(!requires_rebuild(kind), "{kind:?}");
        }
    }

    #[test]
    fn lost_device_requires_rebuild() {
        for kind in [
            ErrorKind::DeviceNotAvailable,
            ErrorKind::StreamInvalidated,
            ErrorKind::BackendError,
        ] {
            assert!(requires_rebuild(kind), "{kind:?}");
        }
    }
}
