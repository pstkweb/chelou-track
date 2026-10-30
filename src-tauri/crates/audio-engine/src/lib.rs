//! Moteur audio natif : possède la carte son et y joue le `BackingPlayer` et la voie guitare
//! (ARCHITECTURE.md §7). Dépend de `cpal` (ALSA sur Linux) : pas de tests unitaires du flux ici,
//! la logique testable vit dans `chelou-audio-core`.
//!
//! Deux modes, choisis par `Engine::apply` :
//! - **sortie seule** (`None`) : sortie par défaut de l'hôte par défaut (WASAPI partagé sur
//!   Windows), fréquence et buffer imposés par le système. Si elle disparaît, le thread moteur
//!   retente chaque seconde d'ouvrir la nouvelle sortie par défaut ;
//! - **duplex** (`Some(DuplexConfig)`) : flux d'entrée + flux de sortie sur la même carte, reliés
//!   par un ring `rtrb` mono (schéma validé par le spike). Un échec à l'ouverture ou une perte de
//!   la carte replie en sortie seule avec l'état `Error` ; pas de réessai automatique de la carte
//!   (« Redémarrer » = `apply` avec la même configuration).
//!
//! Tout ce qui touche aux flux se passe sur le thread moteur (`cpal::Stream` n'est pas `Send`
//! partout) ; `Engine` lui envoie des évènements.

mod devices;

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use chelou_audio_core::guitar::{guitar_lane, GuitarControl, GuitarFx, GuitarLane};
use chelou_audio_core::input::InputFeed;
use chelou_audio_core::player::{backing_player, BackingPlayer, PlayerControl};
use chelou_audio_core::settings::{explain_open_error, DuplexConfig};
use chelou_audio_core::MAX_BLOCK;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, I24};

pub use devices::{list_devices, DeviceInfo, HostDevices};

/// Sortie forcée au silence après l'ouverture : absorbe l'à-coup de démarrage (non compté).
const WARMUP_MS: u32 = 100;
const RETRY_DELAY: Duration = Duration::from_secs(1);
/// Ring entrée → sortie, en échantillons mono.
const RING_CAPACITY: usize = 8192;
/// Surveillance des callbacks : certains pilotes USB cessent simplement d'appeler le callback
/// quand la carte est débranchée, sans erreur. Sans callback pendant `WATCHDOG_STALLS` périodes,
/// le flux est considéré perdu.
const WATCHDOG_PERIOD: Duration = Duration::from_millis(500);
const WATCHDOG_STALLS: u32 = 2;

#[derive(Debug, Clone, PartialEq)]
pub enum EngineState {
    /// Sortie seule, sans carte configurée (ou carte arrêtée par l'utilisateur).
    OutputOnly,
    /// Carte ouverte en duplex : son direct actif.
    Running,
    /// Carte en échec (sortie seule de repli), ou plus aucune sortie (réessai en cours).
    Error(String),
}

enum Event {
    /// Erreur fatale d'un flux ; ignorée si `generation` désigne des flux déjà remplacés.
    StreamError {
        generation: u64,
        message: String,
    },
    Apply(Option<DuplexConfig>, mpsc::Sender<Result<()>>),
    /// Inventaire des cartes, fait sur le thread moteur : en ASIO, un seul pilote peut être
    /// chargé à la fois, une énumération concurrente d'une ouverture la ferait échouer.
    Devices(mpsc::Sender<Vec<HostDevices>>),
}

struct Inner {
    control: Mutex<PlayerControl>,
    guitar: Mutex<GuitarControl>,
    /// Voie guitare jouée par le callback de sortie (cf. `SharedPlayer`) ; `Engine::install_fx` y
    /// installe le plugin.
    lane: SharedLane,
    /// Fréquence à laquelle l'effet a été retiré de la voie (flux rouverts à une autre
    /// fréquence), 0 sinon : son propriétaire doit le recharger.
    fx_cleared_at: AtomicU32,
    state: Mutex<EngineState>,
    xruns: Arc<AtomicU64>,
    /// Une sortie joue (duplex ou sortie seule, même de repli) : la backing track est audible.
    output_open: AtomicBool,
    /// Carte ASIO ouverte : le pilote chargé masque les autres cartes ASIO de l'inventaire.
    open_asio: Mutex<Option<DeviceInfo>>,
    events: Mutex<mpsc::Sender<Event>>,
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

    pub fn guitar(&self) -> MutexGuard<'_, GuitarControl> {
        self.inner.guitar.lock().unwrap()
    }

    /// Installe l'effet de la voie guitare, préparé pour `rate`, et rend le précédent, à libérer
    /// par l'appelant. Flux rouverts entre-temps à une autre fréquence : `fx` est refusé et rendu
    /// (cf. `GuitarLane::install_fx`). Le verrou est pris ici, côté appelant : le callback
    /// (`try_lock`) saute au pire la guitare d'un bloc.
    pub fn install_fx(
        &self,
        fx: Box<dyn GuitarFx>,
        rate: u32,
    ) -> Result<Option<Box<dyn GuitarFx>>, Box<dyn GuitarFx>> {
        self.inner.lane.lock().unwrap().install_fx(fx, rate)
    }

    /// Retire l'effet de la voie guitare (son direct) et le rend, à libérer par l'appelant.
    pub fn take_fx(&self) -> Option<Box<dyn GuitarFx>> {
        self.inner.lane.lock().unwrap().take_fx()
    }

    /// Fréquence à laquelle l'effet a été retiré de la voie depuis le dernier appel (flux
    /// rouverts à une autre fréquence), 0 sinon ; remise à 0 par la lecture.
    pub fn fx_cleared_at(&self) -> u32 {
        self.inner.fx_cleared_at.swap(0, Relaxed)
    }

    pub fn state(&self) -> EngineState {
        self.inner.state.lock().unwrap().clone()
    }

    /// Coupures comptées depuis la dernière remise à zéro (ring vide ou plein, callback trop long).
    pub fn xruns(&self) -> u64 {
        self.inner.xruns.load(Relaxed)
    }

    /// `false` : plus aucune sortie (réessai en cours), la backing track est muette.
    pub fn has_output(&self) -> bool {
        self.inner.output_open.load(Relaxed)
    }

    pub fn reset_xruns(&self) {
        self.inner.xruns.store(0, Relaxed);
    }

    /// Reconstruit les flux sur le thread moteur et attend le résultat. En cas d'échec de la
    /// carte, le moteur est en sortie seule et l'état porte l'erreur.
    pub fn apply(&self, config: Option<DuplexConfig>) -> Result<()> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.inner
            .events
            .lock()
            .unwrap()
            .send(Event::Apply(config, reply_tx))
            .map_err(|_| anyhow!("thread audio arrêté"))?;
        reply_rx.recv().context("thread audio arrêté")?
    }

    /// `list_devices` fait par le thread moteur (cf. `Event::Devices`), complété de la carte
    /// ASIO ouverte (son pilote chargé empêche cpal d'énumérer les cartes ASIO).
    pub fn devices(&self) -> Vec<HostDevices> {
        let (reply_tx, reply_rx) = mpsc::channel();
        let sent = self
            .inner
            .events
            .lock()
            .unwrap()
            .send(Event::Devices(reply_tx));
        let mut list = match sent {
            Ok(()) => reply_rx.recv().unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        if let Some(open) = self.inner.open_asio.lock().unwrap().clone() {
            match list.iter_mut().find(|h| h.host == "asio") {
                Some(asio) if asio.devices.iter().any(|d| d.name == open.name) => {}
                Some(asio) => asio.devices.insert(0, open),
                None => list.insert(
                    0,
                    HostDevices {
                        host: "asio".into(),
                        devices: vec![open],
                    },
                ),
            }
        }
        list
    }
}

/// Lance le thread moteur et applique `config` (`None` : sortie seule). Un échec du duplex
/// replie en sortie seule (reflété dans l'état) ; échoue seulement si aucune sortie ne s'ouvre.
pub fn start(config: Option<DuplexConfig>) -> Result<Engine> {
    let (ready_tx, ready_rx) = mpsc::channel::<Result<Arc<Inner>>>();
    std::thread::Builder::new()
        .name("chelou-audio".into())
        .spawn(move || engine_thread(config, ready_tx))
        .context("lancement du thread audio")?;
    let inner = ready_rx.recv().context("thread audio arrêté")??;
    Ok(Engine { inner })
}

/// Le callback de sortie est l'unique utilisateur du lecteur et de la voie guitare ; le `Mutex`
/// les rend partageables (`Sync`) entre le thread moteur, qui les crée, et le thread du flux,
/// qui les joue. Une reconfiguration détruit les anciens flux avant d'en construire de nouveaux :
/// deux callbacks ne se disputent jamais le verrou.
type SharedPlayer = Arc<Mutex<BackingPlayer>>;
type SharedLane = Arc<Mutex<GuitarLane>>;

struct Runner {
    inner: Arc<Inner>,
    events: mpsc::Sender<Event>,
    player: SharedPlayer,
    rate: u32,
    /// Vide : plus aucune sortie, réessai périodique.
    streams: Vec<cpal::Stream>,
    /// Incrémenté par chaque callback de sortie (surveillance, cf. `WATCHDOG_PERIOD`).
    heartbeat: Arc<AtomicU64>,
    generation: u64,
    config: Option<DuplexConfig>,
    /// Message d'échec de la carte, gardé tant que le moteur est en sortie seule de repli.
    duplex_error: Option<String>,
}

fn engine_thread(config: Option<DuplexConfig>, ready: mpsc::Sender<Result<Arc<Inner>>>) {
    let (events_tx, events_rx) = mpsc::channel::<Event>();
    let (control, player) = backing_player(0);
    let (guitar, lane) = guitar_lane();
    let inner = Arc::new(Inner {
        control: Mutex::new(control),
        guitar: Mutex::new(guitar),
        lane: Arc::new(Mutex::new(lane)),
        fx_cleared_at: AtomicU32::new(0),
        state: Mutex::new(EngineState::OutputOnly),
        xruns: Arc::new(AtomicU64::new(0)),
        output_open: AtomicBool::new(false),
        open_asio: Mutex::new(None),
        events: Mutex::new(events_tx.clone()),
    });
    let mut runner = Runner {
        inner: inner.clone(),
        events: events_tx,
        player: Arc::new(Mutex::new(player)),
        rate: 0,
        streams: Vec::new(),
        heartbeat: Arc::new(AtomicU64::new(0)),
        generation: 0,
        config,
        duplex_error: None,
    };
    let _ = runner.open();
    if runner.streams.is_empty() {
        let message = match runner.inner.state.lock().unwrap().clone() {
            EngineState::Error(message) => message,
            _ => "Aucune sortie audio disponible".to_string(),
        };
        let _ = ready.send(Err(anyhow!(message)));
        return;
    }
    let _ = ready.send(Ok(inner));
    runner.run(events_rx);
}

impl Runner {
    fn set_state(&self, state: EngineState) {
        self.inner
            .output_open
            .store(!self.streams.is_empty(), Relaxed);
        *self.inner.state.lock().unwrap() = state;
    }

    fn run(&mut self, events: mpsc::Receiver<Event>) {
        let mut last_beat = self.heartbeat.load(Relaxed);
        let mut stalls = 0;
        loop {
            let waiting = if self.streams.is_empty() {
                RETRY_DELAY
            } else {
                WATCHDOG_PERIOD
            };
            let event = match events.recv_timeout(waiting) {
                Ok(event) => event,
                Err(RecvTimeoutError::Timeout) if self.streams.is_empty() => {
                    self.retry_output();
                    continue;
                }
                Err(RecvTimeoutError::Timeout) => {
                    let beat = self.heartbeat.load(Relaxed);
                    stalls = if beat == last_beat { stalls + 1 } else { 0 };
                    last_beat = beat;
                    if stalls >= WATCHDOG_STALLS {
                        stalls = 0;
                        self.on_stream_error("La carte son ne répond plus".into());
                    }
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => return,
            };
            match event {
                Event::Devices(reply) => {
                    let _ = reply.send(list_devices());
                }
                Event::Apply(config, reply) => {
                    self.config = config;
                    let _ = reply.send(self.open());
                }
                Event::StreamError {
                    generation,
                    message,
                } if generation == self.generation => self.on_stream_error(message),
                Event::StreamError { .. } => {}
            }
        }
    }

    fn close(&mut self) {
        self.streams.clear();
        self.generation += 1;
        *self.inner.open_asio.lock().unwrap() = None;
    }

    /// Applique `self.config` : duplex si configuré, sinon (ou en repli) sortie seule.
    fn open(&mut self) -> Result<()> {
        self.close();
        self.duplex_error = None;
        let Some(config) = self.config.clone() else {
            return self.open_output_only();
        };
        match self.open_duplex(&config) {
            Ok(()) => {
                self.set_state(EngineState::Running);
                Ok(())
            }
            Err(e) => {
                let message = format!("{e:#}");
                self.close();
                self.duplex_error = Some(message.clone());
                let _ = self.open_output_only();
                Err(anyhow!(message))
            }
        }
    }

    fn open_output_only(&mut self) -> Result<()> {
        let opened = self.build_default_output();
        let state = match &opened {
            Ok(()) => match &self.duplex_error {
                Some(message) => EngineState::Error(message.clone()),
                None => EngineState::OutputOnly,
            },
            Err(e) => EngineState::Error(format!("{e:#}")),
        };
        self.set_state(state);
        opened
    }

    fn retry_output(&mut self) {
        let _ = self.open_output_only();
    }

    fn on_stream_error(&mut self, message: String) {
        let duplex = self.duplex_error.is_none() && self.config.is_some();
        self.close();
        if duplex {
            // Carte perdue : repli en sortie seule, sans réessayer la carte.
            self.duplex_error = Some(message);
            let _ = self.open_output_only();
        } else {
            // Sortie par défaut perdue : premier essai immédiat (sur un changement de sortie
            // par défaut WASAPI, la remplaçante existe déjà), puis chaque seconde.
            self.set_state(EngineState::Error(message));
            self.retry_output();
        }
    }

    /// Lecteur pour des flux à `rate` : l'actuel à même fréquence (le track et sa position sont
    /// gardés), sinon un nouveau, installé par `adopt` seulement si les flux s'ouvrent — un échec
    /// ne doit pas perdre le track chargé. À une autre fréquence, l'effet de la voie guitare
    /// (préparé pour l'ancienne) est retiré dès maintenant, flux fermés : aucun callback ne le
    /// joue à la mauvaise fréquence.
    fn candidate(&self, rate: u32) -> (Option<PlayerControl>, SharedPlayer) {
        // Toujours, même à fréquence égale : la voie suit la fréquence des flux qui s'ouvrent
        // (un essai raté à une autre fréquence l'a peut-être changée).
        let cleared = self.inner.lane.lock().unwrap().set_stream_rate(rate);
        if cleared.is_some() {
            self.inner.fx_cleared_at.store(rate, Relaxed);
        }
        if rate == self.rate {
            return (None, self.player.clone());
        }
        let (control, player) = backing_player(rate);
        (Some(control), Arc::new(Mutex::new(player)))
    }

    /// Installe le lecteur des flux qui viennent de s'ouvrir. À une autre fréquence, le track
    /// chargé est perdu : la vue le recharge en voyant la nouvelle fréquence.
    fn adopt(&mut self, rate: u32, candidate: (Option<PlayerControl>, SharedPlayer)) {
        if let (Some(control), player) = candidate {
            self.player = player;
            *self.inner.control.lock().unwrap() = control;
            self.rate = rate;
        }
    }

    fn build_default_output(&mut self) -> Result<()> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| anyhow!("Aucune sortie audio disponible"))?;
        let config = device
            .default_output_config()
            .context("configuration de la sortie audio")?;
        let candidate = self.candidate(config.sample_rate());
        let stream = self.build_output(
            &device,
            config.sample_format(),
            &config.config(),
            0,
            candidate.1.clone(),
            None,
        )?;
        stream.play().context("démarrage de la sortie audio")?;
        self.streams.push(stream);
        self.adopt(config.sample_rate(), candidate);
        Ok(())
    }

    fn open_duplex(&mut self, config: &DuplexConfig) -> Result<()> {
        let host = devices::find_host(&config.host)?;
        let in_device = devices::find_device(&host, &config.input_device, true)?;
        let out_device = devices::find_device(&host, &config.output_device, false)?;
        let in_default = in_device
            .default_input_config()
            .context("configuration de l'entrée")?;
        let out_default = out_device
            .default_output_config()
            .context("configuration de la sortie")?;
        // WASAPI partagé : fréquence imposée par le format Windows du périphérique, buffer choisi
        // par le système.
        let shared = config.host == "wasapi";
        let rate = if shared {
            out_default.sample_rate()
        } else {
            config.sample_rate
        };
        if shared && in_default.sample_rate() != rate {
            bail!(
                "Entrée ({} Hz) et sortie ({} Hz) à des fréquences différentes dans Windows",
                in_default.sample_rate(),
                rate
            );
        }
        let buffer_size = if shared {
            cpal::BufferSize::Default
        } else {
            cpal::BufferSize::Fixed(config.buffer)
        };
        let stream_config = |channels| cpal::StreamConfig {
            channels,
            sample_rate: rate,
            buffer_size,
        };
        let in_config = stream_config(in_default.channels().max(config.input_channel + 1));
        let out_config = stream_config(out_default.channels().max(config.output_pair * 2 + 2));

        let candidate = self.candidate(rate);
        let (producer, consumer) = rtrb::RingBuffer::new(RING_CAPACITY);
        // En ASIO, l'entrée est servie juste avant la sortie dans le même cycle : pas d'avance à
        // prendre. Ailleurs, les deux flux sont indépendants.
        let feed = InputFeed::new(consumer, config.host != "asio");
        let player = candidate.1.clone();
        let opened = (|| -> Result<()> {
            let input = self.build_input(
                &in_device,
                in_default.sample_format(),
                &in_config,
                config.input_channel as usize,
                producer,
            )?;
            let output = self.build_output(
                &out_device,
                out_default.sample_format(),
                &out_config,
                config.output_pair as usize * 2,
                player,
                Some(feed),
            )?;
            input.play().context("démarrage de l'entrée")?;
            output.play().context("démarrage de la sortie")?;
            self.streams.push(input);
            self.streams.push(output);
            Ok(())
        })();
        if let Err(e) = opened {
            let rates = devices::describe(&out_device)
                .map(|d| d.sample_rates)
                .unwrap_or_default();
            bail!(open_error_message(&e, rate, &rates));
        }
        self.adopt(rate, candidate);
        if config.host == "asio" {
            *self.inner.open_asio.lock().unwrap() = devices::describe(&out_device);
        }
        Ok(())
    }

    fn error_callback(&self) -> impl FnMut(cpal::Error) + Send + 'static {
        let events = self.events.clone();
        let generation = self.generation;
        move |e| {
            if requires_rebuild(e.kind()) {
                let _ = events.send(Event::StreamError {
                    generation,
                    message: e.to_string(),
                });
            }
        }
    }

    fn build_input(
        &self,
        device: &cpal::Device,
        format: SampleFormat,
        config: &cpal::StreamConfig,
        channel: usize,
        producer: rtrb::Producer<f32>,
    ) -> Result<cpal::Stream> {
        match format {
            SampleFormat::F32 => self.build_input_typed::<f32>(device, config, channel, producer),
            SampleFormat::I16 => self.build_input_typed::<i16>(device, config, channel, producer),
            SampleFormat::I32 => self.build_input_typed::<i32>(device, config, channel, producer),
            SampleFormat::I24 => self.build_input_typed::<I24>(device, config, channel, producer),
            SampleFormat::F64 => self.build_input_typed::<f64>(device, config, channel, producer),
            f => Err(anyhow!("format d'entrée non géré : {f:?}")),
        }
    }

    fn build_input_typed<T>(
        &self,
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        channel: usize,
        mut producer: rtrb::Producer<f32>,
    ) -> Result<cpal::Stream>
    where
        T: SizedSample,
        f32: FromSample<T>,
    {
        let channels = config.channels as usize;
        let xruns = self.inner.xruns.clone();
        let stream = device
            .build_input_stream(
                *config,
                move |data: &[T], _| {
                    let mut overflow = false;
                    for frame in data.chunks_exact(channels) {
                        // Ring plein = la sortie ne consomme plus : on jette plutôt que de bloquer.
                        overflow |= producer.push(f32::from_sample(frame[channel])).is_err();
                    }
                    if overflow {
                        xruns.fetch_add(1, Relaxed);
                    }
                },
                self.error_callback(),
                None,
            )
            .context("ouverture du flux d'entrée")?;
        Ok(stream)
    }

    fn build_output(
        &self,
        device: &cpal::Device,
        format: SampleFormat,
        config: &cpal::StreamConfig,
        first_channel: usize,
        player: SharedPlayer,
        input: Option<InputFeed>,
    ) -> Result<cpal::Stream> {
        let at = (first_channel, player, input);
        match format {
            SampleFormat::F32 => self.build_output_typed::<f32>(device, config, at),
            SampleFormat::I16 => self.build_output_typed::<i16>(device, config, at),
            SampleFormat::I32 => self.build_output_typed::<i32>(device, config, at),
            SampleFormat::I24 => self.build_output_typed::<I24>(device, config, at),
            SampleFormat::F64 => self.build_output_typed::<f64>(device, config, at),
            f => Err(anyhow!("format de sortie non géré : {f:?}")),
        }
    }

    /// Sortie : `input` (son direct, absent en sortie seule) puis backing track, mixés en stéréo
    /// et écrits sur les canaux `first_channel` et `first_channel + 1`.
    fn build_output_typed<T>(
        &self,
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        (first_channel, player, mut input): (usize, SharedPlayer, Option<InputFeed>),
    ) -> Result<cpal::Stream>
    where
        T: SizedSample + FromSample<f32>,
    {
        let channels = config.channels as usize;
        let rate = config.sample_rate as f64;
        let mut warmup = (config.sample_rate * WARMUP_MS / 1000) as usize;
        let lane = self.inner.lane.clone();
        let xruns = self.inner.xruns.clone();
        let heartbeat = self.heartbeat.clone();
        // Pré-alloués : le callback ne fait aucune allocation.
        let mut mix = vec![0.0f32; MAX_BLOCK * 2];
        let mut mono = vec![0.0f32; MAX_BLOCK];
        let stream = device
            .build_output_stream(
                *config,
                move |data: &mut [T], _| {
                    let started = Instant::now();
                    heartbeat.fetch_add(1, Relaxed);
                    let counted = warmup == 0;
                    let mut underrun = false;
                    for block in data.chunks_mut(MAX_BLOCK * channels) {
                        let frames = block.len() / channels;
                        let mix = &mut mix[..frames * 2];
                        let mono = &mut mono[..frames];
                        mix.fill(0.0);
                        if warmup > 0 {
                            // Ouverture : silence, et on vide ce que l'entrée a déjà poussé pour
                            // ne pas démarrer avec de la latence en trop.
                            warmup = warmup.saturating_sub(frames);
                            if let Some(input) = &mut input {
                                input.drain();
                            }
                        } else {
                            if let Some(input) = &mut input {
                                underrun |= input.read(mono);
                                // try_lock n'échoue pas en pratique (cf. `SharedPlayer`) ; s'il
                                // échouait, ce bloc sortirait sans la voie plutôt que d'attendre.
                                if let Ok(mut lane) = lane.try_lock() {
                                    lane.process(mono, mix);
                                }
                            }
                            if let Ok(mut p) = player.try_lock() {
                                p.render(mix);
                            }
                        }
                        for (out, st) in
                            block.chunks_exact_mut(channels).zip(mix.as_chunks::<2>().0)
                        {
                            if channels == 1 {
                                out[0] = T::from_sample((st[0] + st[1]) * 0.5);
                                continue;
                            }
                            for (i, o) in out.iter_mut().enumerate() {
                                *o = T::from_sample(match i.checked_sub(first_channel) {
                                    Some(0) => st[0],
                                    Some(1) => st[1],
                                    _ => 0.0,
                                });
                            }
                        }
                    }
                    let budget = (data.len() / channels) as f64 / rate;
                    if counted && (underrun || started.elapsed().as_secs_f64() > budget) {
                        xruns.fetch_add(1, Relaxed);
                    }
                },
                self.error_callback(),
                None,
            )
            .context("ouverture du flux de sortie")?;
        Ok(stream)
    }
}

/// Message d'échec d'ouverture de la carte montré à l'utilisateur (cf. `explain_open_error`).
fn open_error_message(error: &anyhow::Error, rate: u32, supported_rates: &[u32]) -> String {
    explain_open_error(&format!("{error:#}"), rate, supported_rates)
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
    fn busy_error_is_explained() {
        let err = anyhow!("ouverture du flux d'entrée: Sample rate 48000 Hz is not supported");
        assert_eq!(
            open_error_message(&err, 48000, &[44100, 48000]),
            "Pilote audio déjà utilisé par une autre application"
        );
        let err = anyhow!("périphérique « X » introuvable");
        assert_eq!(
            open_error_message(&err, 48000, &[48000]),
            "périphérique « X » introuvable"
        );
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
