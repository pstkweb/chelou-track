// Instance de plugin VST3 : séquence d'initialisation validée par le spike, partie temps réel
// (`RtProc`) confiée à la voie guitare, état, démontage ordonné.
#![allow(non_snake_case)]
// Les types des bindings (énumérations, `TChar`) varient selon l'OS : casts gardés partout.
#![allow(clippy::unnecessary_cast)]

use std::mem::ManuallyDrop;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, bail, Result};
use chelou_audio_core::guitar::GuitarFx;
use chelou_audio_core::plugins::is_listed;
use chelou_audio_core::MAX_BLOCK;
use vst3::Steinberg::Vst::*;
use vst3::Steinberg::*;
use vst3::{ComPtr, ComWrapper};

use crate::host_objects::{ComponentHandler, HostApp, MemStream, ParamChanges};
use crate::module::Module;

/// Pré-chauffage : le 1er `process` d'Archetype prend ≈ 10 ms (initialisation paresseuse), plus
/// que le budget d'un callback à 128 / 48 kHz.
const WARMUP_BLOCKS: usize = 8;
const WARMUP_FRAMES: usize = 256;
/// Éditions de l'interface en attente pour le prochain bloc.
const EDIT_QUEUE: usize = 1024;
const PARAM_QUEUES: usize = 64;

fn check(r: tresult, what: &str) -> Result<()> {
    if r == kResultOk {
        Ok(())
    } else {
        bail!("{what} a renvoyé {r}")
    }
}

/// Objets COM du plugin et de l'hôte, libérés avant le module.
struct Instance {
    component: ComPtr<IComponent>,
    processor: ComPtr<IAudioProcessor>,
    controller: ComPtr<IEditController>,
    /// Contrôleur distinct du composant (initialisé et connecté séparément).
    separate: Option<(ComPtr<IConnectionPoint>, ComPtr<IConnectionPoint>)>,
    controller_is_component: bool,
    _handler: ComWrapper<ComponentHandler>,
    _host: ComWrapper<HostApp>,
}

/// Démontage d'un chargement interrompu par une erreur : l'inverse des étapes déjà faites, avant
/// que le module ne soit libéré (un plugin JUCE initialisé peut avoir lancé des minuteries ou des
/// fenêtres de messages qui rappelleraient du code déchargé). Désarmé quand le chargement aboutit.
#[derive(Default)]
struct Undo {
    /// Désarmé : ne démonte rien (le plugin chargé s'en charge, cf. `Drop for Vst3Plugin`).
    disarmed: bool,
    component: Option<ComPtr<IComponent>>,
    /// Contrôleur distinct, initialisé.
    controller: Option<ComPtr<IEditController>>,
    points: Option<(ComPtr<IConnectionPoint>, ComPtr<IConnectionPoint>)>,
    /// Contrôleur auquel le `IComponentHandler` a été donné.
    handler_on: Option<ComPtr<IEditController>>,
}

impl Undo {
    /// Attention : ne pas remplacer `self` (l'ancienne valeur serait détruite, donc démontée).
    fn disarm(&mut self) {
        self.disarmed = true;
    }
}

impl Drop for Undo {
    fn drop(&mut self) {
        if self.disarmed {
            return;
        }
        // SAFETY: inverse des appels réussis de `Vst3Plugin::load`, sur le même thread.
        unsafe {
            if let Some((a, b)) = &self.points {
                a.disconnect(b.as_ptr());
                b.disconnect(a.as_ptr());
            }
            if let Some(c) = &self.handler_on {
                c.setComponentHandler(std::ptr::null_mut());
            }
            if let Some(c) = &self.controller {
                c.terminate();
            }
            if let Some(c) = &self.component {
                c.terminate();
            }
        }
    }
}

/// Plugin chargé. **Thread principal uniquement** (`!Send`) : les plugins JUCE rattachent leur
/// thread de messages au premier thread qui les appelle. Seul `RtProc` (cf. `take_rt`) part sur
/// le thread audio, et doit être libéré **avant** ce plugin.
pub struct Vst3Plugin {
    instance: ManuallyDrop<Instance>,
    module: ManuallyDrop<Module>,
    rt: Option<RtProc>,
    /// Détenu aussi par le `RtProc` remis à la voie : tant qu'il vit, le callback peut appeler
    /// le plugin, qui ne doit alors être ni arrêté ni déchargé.
    rt_alive: Arc<()>,
    latency: u32,
    /// Levé par l'interface du plugin (cf. `ComponentHandler::dirty`).
    dirty: Arc<AtomicBool>,
}

impl Vst3Plugin {
    /// Charge la classe `id` (cid hex) du bundle, à défaut la classe listée nommée `name` (un CID
    /// de `moduleinfo.json` peut être écrit dans un autre ordre d'octets que le `TUID`), la
    /// prépare pour `sample_rate` et `MAX_BLOCK`, puis la pré-chauffe.
    pub fn load(bundle: &Path, id: &str, name: &str, sample_rate: u32) -> Result<Vst3Plugin> {
        let module = Module::load(bundle)?;
        let cid = match module.class_id(id) {
            Some(cid) => cid,
            None => {
                let class = module
                    .classes()
                    .into_iter()
                    .find(|c| is_listed(c) && c.name == name)
                    .ok_or_else(|| anyhow!("« {name} » absent de {}", bundle.display()))?;
                module
                    .class_id(&class.cid)
                    .ok_or_else(|| anyhow!("classe introuvable"))?
            }
        };
        let host = ComWrapper::new(HostApp);
        let host_ctx = host.as_com_ref::<IHostApplication>().unwrap().as_ptr() as *mut FUnknown;
        let (prod, cons) = rtrb::RingBuffer::new(EDIT_QUEUE);
        let dirty = Arc::new(AtomicBool::new(false));
        let handler = ComWrapper::new(ComponentHandler {
            edits: Mutex::new(prod),
            dirty: dirty.clone(),
        });
        // Déclaré après le module : détruit avant lui en cas d'erreur.
        let mut undo = Undo::default();

        // SAFETY: séquence d'initialisation du SDK VST3 (cf. spike), objets tenus par `Instance`.
        unsafe {
            let component: ComPtr<IComponent> = module
                .create(&cid)
                .ok_or_else(|| anyhow!("création du composant"))?;
            check(component.initialize(host_ctx), "component.initialize")?;
            undo.component = Some(component.clone());

            // Contrôleur : le composant lui-même, ou une classe séparée (JUCE, Neural DSP…).
            let (controller, separate, controller_is_component) =
                match component.cast::<IEditController>() {
                    Some(c) => (c, None, true),
                    None => {
                        let mut ccid: TUID = [0; 16];
                        check(
                            component.getControllerClassId(&mut ccid),
                            "getControllerClassId",
                        )?;
                        let c: ComPtr<IEditController> = module
                            .create(&ccid)
                            .ok_or_else(|| anyhow!("création du contrôleur"))?;
                        check(c.initialize(host_ctx), "controller.initialize")?;
                        undo.controller = Some(c.clone());
                        let points = match (
                            component.cast::<IConnectionPoint>(),
                            c.cast::<IConnectionPoint>(),
                        ) {
                            (Some(a), Some(b)) => {
                                a.connect(b.as_ptr());
                                b.connect(a.as_ptr());
                                undo.points = Some((a.clone(), b.clone()));
                                Some((a, b))
                            }
                            _ => None,
                        };
                        (c, points, false)
                    }
                };
            controller
                .setComponentHandler(handler.as_com_ref::<IComponentHandler>().unwrap().as_ptr());
            undo.handler_on = Some(controller.clone());

            // État composant → contrôleur, pour que l'interface reflète le son réel.
            let stream = ComWrapper::new(MemStream::default());
            let sp = stream.as_com_ref::<IBStream>().unwrap().as_ptr();
            if component.getState(sp) == kResultOk {
                stream.seek(0, 0, std::ptr::null_mut());
                controller.setComponentState(sp);
            }

            let processor: ComPtr<IAudioProcessor> = component
                .cast()
                .ok_or_else(|| anyhow!("pas d'IAudioProcessor"))?;

            // Entrée mono si le plugin l'accepte, sinon stéréo (guitare dupliquée).
            let audio = MediaTypes_::kAudio as i32;
            let n_in = component.getBusCount(audio, BusDirections_::kInput as i32);
            let n_out = component.getBusCount(audio, BusDirections_::kOutput as i32);
            if n_out < 1 {
                bail!("le plugin n'a pas de sortie audio");
            }
            let mut ins = vec![SpeakerArr::kMono; n_in.max(1) as usize];
            let mut outs = vec![SpeakerArr::kStereo; n_out as usize];
            if processor.setBusArrangements(ins.as_mut_ptr(), n_in, outs.as_mut_ptr(), n_out)
                != kResultTrue
            {
                ins[0] = SpeakerArr::kStereo;
                processor.setBusArrangements(ins.as_mut_ptr(), n_in, outs.as_mut_ptr(), n_out);
            }
            let mut arr: SpeakerArrangement = 0;
            processor.getBusArrangement(BusDirections_::kInput as i32, 0, &mut arr);
            let in_channels = (arr.count_ones() as i32).clamp(1, 2);

            let mut setup = ProcessSetup {
                processMode: ProcessModes_::kRealtime as i32,
                symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
                maxSamplesPerBlock: MAX_BLOCK as i32,
                sampleRate: sample_rate as f64,
            };
            check(processor.setupProcessing(&mut setup), "setupProcessing")?;
            for i in 0..n_in {
                component.activateBus(audio, BusDirections_::kInput as i32, i, (i == 0) as u8);
            }
            for i in 0..n_out {
                component.activateBus(audio, BusDirections_::kOutput as i32, i, (i == 0) as u8);
            }
            check(component.setActive(1), "setActive")?;
            processor.setProcessing(1);
            // Plus aucune étape ne peut échouer : le démontage revient à `Drop for Vst3Plugin`.
            undo.disarm();
            let latency = processor.getLatencySamples();

            let rt_alive = Arc::new(());
            let mut rt = RtProc {
                processor: processor.clone(),
                edits: cons,
                changes: ComWrapper::new(ParamChanges::new(PARAM_QUEUES)),
                in_l: vec![0.0; MAX_BLOCK],
                in_r: vec![0.0; MAX_BLOCK],
                out_l: vec![0.0; MAX_BLOCK],
                out_r: vec![0.0; MAX_BLOCK],
                in_buses: vec![std::mem::zeroed(); n_in.max(0) as usize],
                out_buses: vec![std::mem::zeroed(); n_out as usize],
                in_channels,
                ctx: std::mem::zeroed(),
                _alive: rt_alive.clone(),
            };
            rt.ctx.sampleRate = sample_rate as f64;
            let silence = [0.0f32; WARMUP_FRAMES];
            let mut out = [0.0f32; WARMUP_FRAMES * 2];
            for _ in 0..WARMUP_BLOCKS {
                rt.process(&silence, &mut out);
            }

            Ok(Vst3Plugin {
                instance: ManuallyDrop::new(Instance {
                    component,
                    processor,
                    controller,
                    separate,
                    controller_is_component,
                    _handler: handler,
                    _host: host,
                }),
                module: ManuallyDrop::new(module),
                rt: Some(rt),
                rt_alive,
                latency,
                dirty,
            })
        }
    }

    /// Un réglage a changé depuis l'appel précédent (son état est à enregistrer).
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::Relaxed)
    }

    /// Latence ajoutée par le plugin, en échantillons.
    pub fn latency_samples(&self) -> u32 {
        self.latency
    }

    /// Partie temps réel, à confier à la voie guitare (une seule fois).
    pub fn take_rt(&mut self) -> Box<dyn GuitarFx> {
        Box::new(self.rt.take().expect("take_rt appelé deux fois"))
    }

    pub fn controller(&self) -> &ComPtr<IEditController> {
        &self.instance.controller
    }

    /// État composant + contrôleur : `[len composant u32 LE][composant][contrôleur]`.
    pub fn save_state(&self) -> Vec<u8> {
        let i = &*self.instance;
        // SAFETY: `getState` écrit dans nos flux mémoire.
        unsafe {
            let comp = ComWrapper::new(MemStream::default());
            i.component
                .getState(comp.as_com_ref::<IBStream>().unwrap().as_ptr());
            let ctrl = ComWrapper::new(MemStream::default());
            i.controller
                .getState(ctrl.as_com_ref::<IBStream>().unwrap().as_ptr());
            let (c, k) = (comp.data.borrow(), ctrl.data.borrow());
            let mut out = (c.len() as u32).to_le_bytes().to_vec();
            out.extend_from_slice(&c);
            out.extend_from_slice(&k);
            out
        }
    }

    /// Restauration : `component.setState` → `controller.setComponentState` → `controller.setState`.
    pub fn restore_state(&self, bytes: &[u8]) -> Result<()> {
        let n = u32::from_le_bytes(
            bytes
                .get(..4)
                .ok_or_else(|| anyhow!("état tronqué"))?
                .try_into()?,
        ) as usize;
        let rest = &bytes[4..];
        if rest.len() < n {
            bail!("état tronqué");
        }
        let (comp, ctrl) = rest.split_at(n);
        let i = &*self.instance;
        // SAFETY: `setState` lit nos flux mémoire.
        unsafe {
            let s = ComWrapper::new(MemStream::from(comp.to_vec()));
            let sp = s.as_com_ref::<IBStream>().unwrap().as_ptr();
            check(i.component.setState(sp), "component.setState")?;
            s.seek(0, 0, std::ptr::null_mut());
            i.controller.setComponentState(sp);
            let k = ComWrapper::new(MemStream::from(ctrl.to_vec()));
            i.controller
                .setState(k.as_com_ref::<IBStream>().unwrap().as_ptr());
        }
        Ok(())
    }
}

impl Drop for Vst3Plugin {
    fn drop(&mut self) {
        self.rt = None;
        if Arc::strong_count(&self.rt_alive) > 1 {
            // Le callback audio peut encore appeler le plugin : l'arrêter ou libérer sa DLL le
            // ferait planter. Mieux vaut fuir (bug de l'appelant, cf. doc de `Vst3Plugin`).
            eprintln!(
                "vst3 : plugin libéré pendant que la voie guitare le joue — fuite volontaire"
            );
            return;
        }
        let i = &*self.instance;
        // SAFETY: démontage dans l'ordre inverse de `load`, plus aucun `process` en cours.
        unsafe {
            i.processor.setProcessing(0);
            i.component.setActive(0);
            if let Some((a, b)) = &i.separate {
                a.disconnect(b.as_ptr());
                b.disconnect(a.as_ptr());
            }
            i.controller.setComponentHandler(std::ptr::null_mut());
            if !i.controller_is_component {
                i.controller.terminate();
            }
            i.component.terminate();
            ManuallyDrop::drop(&mut self.instance);
            ManuallyDrop::drop(&mut self.module);
        }
    }
}

/// Partie temps réel : buffers et files pré-alloués, aucune allocation dans `process`.
pub struct RtProc {
    processor: ComPtr<IAudioProcessor>,
    edits: rtrb::Consumer<(u32, f64)>,
    changes: ComWrapper<ParamChanges>,
    in_l: Vec<f32>,
    in_r: Vec<f32>,
    out_l: Vec<f32>,
    out_r: Vec<f32>,
    in_buses: Vec<AudioBusBuffers>,
    out_buses: Vec<AudioBusBuffers>,
    in_channels: i32,
    ctx: ProcessContext,
    _alive: Arc<()>,
}

// SAFETY: utilisé par un seul thread à la fois (pré-chauffage sur le thread principal, puis le
// callback audio), ce que le SDK VST3 autorise pour `process`.
unsafe impl Send for RtProc {}

impl GuitarFx for RtProc {
    fn process(&mut self, input: &[f32], out: &mut [f32]) {
        let frames = input.len().min(MAX_BLOCK).min(out.len() / 2);
        self.changes.clear();
        while let Ok((id, v)) = self.edits.pop() {
            self.changes.set(id, v);
        }
        self.in_l[..frames].copy_from_slice(&input[..frames]);
        self.in_r[..frames].copy_from_slice(&input[..frames]);

        let mut in_ptrs = [self.in_l.as_mut_ptr(), self.in_r.as_mut_ptr()];
        let mut out_ptrs = [self.out_l.as_mut_ptr(), self.out_r.as_mut_ptr()];
        for (i, b) in self.in_buses.iter_mut().enumerate() {
            b.numChannels = if i == 0 { self.in_channels } else { 0 };
            b.silenceFlags = 0;
            b.__field0.channelBuffers32 = if i == 0 {
                in_ptrs.as_mut_ptr()
            } else {
                std::ptr::null_mut()
            };
        }
        for (i, b) in self.out_buses.iter_mut().enumerate() {
            b.numChannels = if i == 0 { 2 } else { 0 };
            b.silenceFlags = 0;
            b.__field0.channelBuffers32 = if i == 0 {
                out_ptrs.as_mut_ptr()
            } else {
                std::ptr::null_mut()
            };
        }
        let mut data = ProcessData {
            processMode: ProcessModes_::kRealtime as i32,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
            numSamples: frames as i32,
            numInputs: self.in_buses.len() as i32,
            numOutputs: self.out_buses.len() as i32,
            inputs: self.in_buses.as_mut_ptr(),
            outputs: self.out_buses.as_mut_ptr(),
            inputParameterChanges: self
                .changes
                .as_com_ref::<IParameterChanges>()
                .unwrap()
                .as_ptr(),
            outputParameterChanges: std::ptr::null_mut(),
            inputEvents: std::ptr::null_mut(),
            outputEvents: std::ptr::null_mut(),
            processContext: &mut self.ctx,
        };
        // SAFETY: tous les pointeurs désignent des buffers de `MAX_BLOCK` frames vivants.
        unsafe { self.processor.process(&mut data) };
        for (i, frame) in out[..frames * 2]
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .enumerate()
        {
            *frame = [self.out_l[i], self.out_r[i]];
        }
    }
}
