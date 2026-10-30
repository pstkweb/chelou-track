// Module VST3 : binaire chargé, point d'entrée propre à l'OS, fabrique de classes.
#![allow(non_snake_case)]
// Les types des bindings (énumérations, `TChar`) varient selon l'OS : casts gardés partout.
#![allow(clippy::unnecessary_cast)]

use std::ffi::{c_void, CStr};
use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use chelou_audio_core::plugins::{binary_path, ClassInfo};
use vst3::Steinberg::*;
use vst3::{ComPtr, Interface};

type GetFactoryProc = unsafe extern "system" fn() -> *mut IPluginFactory;

pub(crate) struct Module {
    /// Libérée avant la sortie du module (cf. `Drop`).
    factory: Option<ComPtr<IPluginFactory>>,
    #[cfg(target_os = "macos")]
    _bundle: core_foundation::bundle::CFBundle,
    lib: libloading::Library,
}

fn text(buf: &[std::ffi::c_char]) -> String {
    // SAFETY: les chaînes des structures `PClassInfo*` sont terminées par un zéro (SDK VST3).
    unsafe { CStr::from_ptr(buf.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

/// Octets du `TUID` en hex majuscules (ordre mémoire).
pub(crate) fn cid_hex(cid: &TUID) -> String {
    cid.iter().map(|b| format!("{:02X}", *b as u8)).collect()
}

impl Module {
    pub(crate) fn load(bundle: &Path) -> Result<Module> {
        let bin = binary_path(bundle)
            .ok_or_else(|| anyhow!("binaire introuvable dans {}", bundle.display()))?;
        let lib = open(&bin).with_context(|| format!("chargement de {}", bin.display()))?;
        #[cfg(target_os = "macos")]
        let bundle_ref = enter_macos(&lib, bundle)?;
        // SAFETY: symbole d'export standard d'un module VST3.
        let factory = unsafe {
            let get: libloading::Symbol<GetFactoryProc> = lib
                .get(b"GetPluginFactory")
                .context("GetPluginFactory absent")?;
            ComPtr::from_raw(get()).ok_or_else(|| anyhow!("GetPluginFactory a renvoyé null"))?
        };
        Ok(Module {
            factory: Some(factory),
            #[cfg(target_os = "macos")]
            _bundle: bundle_ref,
            lib,
        })
    }

    fn factory(&self) -> &ComPtr<IPluginFactory> {
        self.factory.as_ref().expect("fabrique libérée")
    }

    pub(crate) fn classes(&self) -> Vec<ClassInfo> {
        let factory = self.factory();
        let f2 = factory.cast::<IPluginFactory2>();
        let mut out = Vec::new();
        // SAFETY: appels de la fabrique VST3 sur des structures zéro-initialisées qu'elle remplit.
        unsafe {
            let mut finfo: PFactoryInfo = std::mem::zeroed();
            let factory_vendor = if factory.getFactoryInfo(&mut finfo) == kResultOk {
                text(&finfo.vendor)
            } else {
                String::new()
            };
            let vendor_or = |v: String| {
                if v.is_empty() {
                    factory_vendor.clone()
                } else {
                    v
                }
            };
            for i in 0..factory.countClasses() {
                if let Some(f2) = &f2 {
                    let mut info: PClassInfo2 = std::mem::zeroed();
                    if f2.getClassInfo2(i, &mut info) == kResultOk {
                        out.push(ClassInfo {
                            cid: cid_hex(&info.cid),
                            category: text(&info.category),
                            name: text(&info.name),
                            vendor: vendor_or(text(&info.vendor)),
                            sub_categories: text(&info.subCategories)
                                .split('|')
                                .filter(|s| !s.is_empty())
                                .map(String::from)
                                .collect(),
                        });
                        continue;
                    }
                }
                let mut info: PClassInfo = std::mem::zeroed();
                if factory.getClassInfo(i, &mut info) == kResultOk {
                    out.push(ClassInfo {
                        cid: cid_hex(&info.cid),
                        category: text(&info.category),
                        name: text(&info.name),
                        vendor: factory_vendor.clone(),
                        sub_categories: Vec::new(),
                    });
                }
            }
        }
        out
    }

    /// Instancie la classe `cid` et renvoie son interface `I`.
    pub(crate) fn create<I: Interface>(&self, cid: &TUID) -> Option<ComPtr<I>> {
        let mut obj: *mut c_void = std::ptr::null_mut();
        // SAFETY: `createInstance` écrit un pointeur d'interface `I` (référence comptée) ou rien.
        unsafe {
            let r = self.factory().createInstance(
                cid.as_ptr(),
                I::IID.as_ptr() as *const std::ffi::c_char,
                &mut obj,
            );
            if r != kResultOk {
                return None;
            }
            ComPtr::from_raw(obj as *mut I)
        }
    }

    /// `TUID` de la classe dont le cid hex vaut `cid`.
    pub(crate) fn class_id(&self, cid: &str) -> Option<TUID> {
        let factory = self.factory();
        // SAFETY: cf. `classes`.
        unsafe {
            (0..factory.countClasses()).find_map(|i| {
                let mut info: PClassInfo = std::mem::zeroed();
                (factory.getClassInfo(i, &mut info) == kResultOk && cid_hex(&info.cid) == cid)
                    .then_some(info.cid)
            })
        }
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        self.factory = None;
        // SAFETY: symboles de sortie standard, appelés une fois, après libération de la fabrique.
        unsafe {
            #[cfg(windows)]
            if let Ok(exit) = self
                .lib
                .get::<unsafe extern "system" fn() -> bool>(b"ExitDll")
            {
                exit();
            }
            #[cfg(target_os = "linux")]
            if let Ok(exit) = self
                .lib
                .get::<unsafe extern "C" fn() -> bool>(b"ModuleExit")
            {
                exit();
            }
            #[cfg(target_os = "macos")]
            if let Ok(exit) = self
                .lib
                .get::<unsafe extern "C" fn() -> bool>(b"bundleExit")
            {
                exit();
            }
        }
    }
}

/// Windows : `InitDll` facultatif.
#[cfg(windows)]
fn open(bin: &Path) -> Result<libloading::Library> {
    // SAFETY: charger un plugin exécute son code ; c'est le principe même d'un hôte VST3.
    let lib = unsafe { libloading::Library::new(bin) }?;
    // SAFETY: symbole d'entrée standard d'un module VST3 Windows.
    unsafe {
        if let Ok(init) = lib.get::<unsafe extern "system" fn() -> bool>(b"InitDll") {
            if !init() {
                bail!("InitDll a échoué");
            }
        }
    }
    Ok(lib)
}

/// Linux : `ModuleEntry(handle)` obligatoire, avec le handle `dlopen` du binaire.
#[cfg(target_os = "linux")]
fn open(bin: &Path) -> Result<libloading::Library> {
    use libloading::os::unix::Library;
    // SAFETY: cf. Windows ; `into_raw`/`from_raw` rendent le même handle sans le fermer, et
    // `ModuleEntry` reçoit le handle du module qui l'exporte (SDK VST3, Linux).
    unsafe {
        let handle = Library::new(bin)?.into_raw();
        let lib = Library::from_raw(handle);
        let entry = lib
            .get::<unsafe extern "C" fn(*mut c_void) -> bool>(b"ModuleEntry")
            .context("ModuleEntry absent")?;
        if !entry(handle) {
            bail!("ModuleEntry a échoué");
        }
        Ok(lib.into())
    }
}

/// macOS : le binaire du bundle ; l'entrée (`bundleEntry`) est faite par `enter_macos`.
#[cfg(target_os = "macos")]
fn open(bin: &Path) -> Result<libloading::Library> {
    // SAFETY: cf. Windows.
    Ok(unsafe { libloading::Library::new(bin) }?)
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn open(_bin: &Path) -> Result<libloading::Library> {
    bail!("plateforme non prise en charge")
}

/// macOS : `bundleEntry(CFBundleRef)` obligatoire ; le `CFBundle` vit aussi longtemps que le
/// module. Non testé (aucun Mac disponible).
#[cfg(target_os = "macos")]
fn enter_macos(
    lib: &libloading::Library,
    bundle: &Path,
) -> Result<core_foundation::bundle::CFBundle> {
    use core_foundation::base::TCFType;
    use core_foundation::bundle::{CFBundle, CFBundleRef};
    use core_foundation::url::CFURL;

    let url = CFURL::from_path(bundle, true)
        .ok_or_else(|| anyhow!("chemin de bundle invalide : {}", bundle.display()))?;
    let cf_bundle =
        CFBundle::new(url).ok_or_else(|| anyhow!("bundle illisible : {}", bundle.display()))?;
    // SAFETY: `bundleEntry` reçoit le bundle du module qui l'exporte (SDK VST3, macOS).
    unsafe {
        let entry = lib
            .get::<unsafe extern "C" fn(CFBundleRef) -> bool>(b"bundleEntry")
            .context("bundleEntry absent")?;
        if !entry(cf_bundle.as_concrete_TypeRef()) {
            bail!("bundleEntry a échoué");
        }
    }
    Ok(cf_bundle)
}

/// Classes d'un bundle, en le chargeant (à faire dans un processus enfant : cf. `scan`).
pub fn probe(bundle: &Path) -> Result<Vec<ClassInfo>> {
    Ok(Module::load(bundle)?.classes())
}
