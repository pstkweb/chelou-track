// Objets COM fournis par l'hôte au plugin : contexte hôte, messages, flux mémoire, paramètres
// (portés du spike). Les plugins JUCE fabriquent les messages composant ↔ contrôleur par
// `IHostApplication::createInstance` : sans `IMessage`, leur éditeur n'agit pas sur le son.
#![allow(non_snake_case)]
// Les types des bindings (énumérations, `TChar`) varient selon l'OS : casts gardés partout.
#![allow(clippy::unnecessary_cast)]

use std::cell::{Cell, RefCell, UnsafeCell};
use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::Mutex;

use vst3::Steinberg::Vst::*;
use vst3::Steinberg::*;
use vst3::{Class, ComWrapper, Interface};

fn same_iid(tuid: *const TUID, iid: &[u8; 16]) -> bool {
    unsafe { std::slice::from_raw_parts(tuid as *const u8, 16) == &iid[..] }
}

// ---------- IHostApplication ----------

pub struct HostApp;
impl Class for HostApp {
    type Interfaces = (IHostApplication,);
}
impl IHostApplicationTrait for HostApp {
    unsafe fn getName(&self, name: *mut String128) -> tresult {
        let n: Vec<u16> = "Chelou Track".encode_utf16().chain([0]).collect();
        (&mut *name)[..n.len()].copy_from_slice(&n);
        kResultOk
    }
    unsafe fn createInstance(
        &self,
        cid: *mut TUID,
        iid: *mut TUID,
        obj: *mut *mut c_void,
    ) -> tresult {
        // Les plugins JUCE (Neural DSP…) passent par ici pour fabriquer les messages
        // composant ↔ contrôleur : sans IMessage, leur éditeur n'est pas relié au processeur.
        if same_iid(cid, &IMessage::IID) && same_iid(iid, &IMessage::IID) {
            *obj = ComWrapper::new(HostMessage::default())
                .to_com_ptr::<IMessage>()
                .unwrap()
                .into_raw() as *mut c_void;
            return kResultOk;
        }
        if same_iid(cid, &IAttributeList::IID) && same_iid(iid, &IAttributeList::IID) {
            *obj = ComWrapper::new(AttrList::default())
                .to_com_ptr::<IAttributeList>()
                .unwrap()
                .into_raw() as *mut c_void;
            return kResultOk;
        }
        *obj = std::ptr::null_mut();
        kResultFalse
    }
}

// ---------- IMessage / IAttributeList ----------

pub struct HostMessage {
    id: RefCell<CString>,
    attrs: vst3::ComPtr<IAttributeList>,
}
impl Default for HostMessage {
    fn default() -> Self {
        HostMessage {
            id: RefCell::new(CString::default()),
            attrs: ComWrapper::new(AttrList::default()).to_com_ptr().unwrap(),
        }
    }
}
impl Class for HostMessage {
    type Interfaces = (IMessage,);
}
impl IMessageTrait for HostMessage {
    unsafe fn getMessageID(&self) -> FIDString {
        self.id.borrow().as_ptr()
    }
    unsafe fn setMessageID(&self, id: FIDString) {
        *self.id.borrow_mut() = if id.is_null() {
            CString::default()
        } else {
            CStr::from_ptr(id).to_owned()
        };
    }
    unsafe fn getAttributes(&self) -> *mut IAttributeList {
        self.attrs.as_ptr()
    }
}

enum Attr {
    Int(i64),
    Float(f64),
    Str(Vec<u16>),
    Bin(Vec<u8>),
}

#[derive(Default)]
pub struct AttrList(RefCell<HashMap<CString, Attr>>);
impl Class for AttrList {
    type Interfaces = (IAttributeList,);
}
impl AttrList {
    unsafe fn put(&self, id: *const c_char, a: Attr) -> tresult {
        if id.is_null() {
            return kInvalidArgument;
        }
        self.0.borrow_mut().insert(CStr::from_ptr(id).to_owned(), a);
        kResultOk
    }
}
impl IAttributeListTrait for AttrList {
    unsafe fn setInt(&self, id: *const c_char, value: i64) -> tresult {
        self.put(id, Attr::Int(value))
    }
    unsafe fn getInt(&self, id: *const c_char, value: *mut i64) -> tresult {
        match self.0.borrow().get(CStr::from_ptr(id)) {
            Some(Attr::Int(v)) => {
                *value = *v;
                kResultOk
            }
            _ => kResultFalse,
        }
    }
    unsafe fn setFloat(&self, id: *const c_char, value: f64) -> tresult {
        self.put(id, Attr::Float(value))
    }
    unsafe fn getFloat(&self, id: *const c_char, value: *mut f64) -> tresult {
        match self.0.borrow().get(CStr::from_ptr(id)) {
            Some(Attr::Float(v)) => {
                *value = *v;
                kResultOk
            }
            _ => kResultFalse,
        }
    }
    unsafe fn setString(&self, id: *const c_char, s: *const TChar) -> tresult {
        let mut v = Vec::new();
        let mut p = s;
        while !p.is_null() && *p != 0 {
            v.push(*p as u16);
            p = p.add(1);
        }
        self.put(id, Attr::Str(v))
    }
    unsafe fn getString(&self, id: *const c_char, out: *mut TChar, size_bytes: u32) -> tresult {
        match self.0.borrow().get(CStr::from_ptr(id)) {
            Some(Attr::Str(v)) => {
                let cap = (size_bytes as usize / 2).saturating_sub(1);
                let n = v.len().min(cap);
                for (i, c) in v[..n].iter().enumerate() {
                    *out.add(i) = *c as TChar;
                }
                *out.add(n) = 0;
                kResultOk
            }
            _ => kResultFalse,
        }
    }
    unsafe fn setBinary(&self, id: *const c_char, data: *const c_void, size: u32) -> tresult {
        self.put(
            id,
            Attr::Bin(std::slice::from_raw_parts(data as *const u8, size as usize).to_vec()),
        )
    }
    unsafe fn getBinary(
        &self,
        id: *const c_char,
        data: *mut *const c_void,
        size: *mut u32,
    ) -> tresult {
        match self.0.borrow().get(CStr::from_ptr(id)) {
            Some(Attr::Bin(v)) => {
                *data = v.as_ptr() as *const c_void;
                *size = v.len() as u32;
                kResultOk
            }
            _ => kResultFalse,
        }
    }
}

// ---------- IBStream (mémoire) ----------

#[derive(Default)]
pub struct MemStream {
    pub data: RefCell<Vec<u8>>,
    pos: Cell<usize>,
}
impl MemStream {
    pub fn from(bytes: Vec<u8>) -> Self {
        MemStream {
            data: RefCell::new(bytes),
            pos: Cell::new(0),
        }
    }
}
impl Class for MemStream {
    type Interfaces = (IBStream,);
}
impl IBStreamTrait for MemStream {
    unsafe fn read(&self, buf: *mut c_void, n: i32, read: *mut i32) -> tresult {
        let data = self.data.borrow();
        let pos = self.pos.get();
        let k = (n.max(0) as usize).min(data.len().saturating_sub(pos));
        std::ptr::copy_nonoverlapping(data.as_ptr().add(pos), buf as *mut u8, k);
        self.pos.set(pos + k);
        if !read.is_null() {
            *read = k as i32;
        }
        kResultOk
    }
    unsafe fn write(&self, buf: *mut c_void, n: i32, written: *mut i32) -> tresult {
        let mut data = self.data.borrow_mut();
        let pos = self.pos.get();
        let n = n.max(0) as usize;
        if data.len() < pos + n {
            data.resize(pos + n, 0);
        }
        std::ptr::copy_nonoverlapping(buf as *const u8, data.as_mut_ptr().add(pos), n);
        self.pos.set(pos + n);
        if !written.is_null() {
            *written = n as i32;
        }
        kResultOk
    }
    unsafe fn seek(&self, pos: i64, mode: i32, result: *mut i64) -> tresult {
        let base = match mode {
            0 => 0i64,
            1 => self.pos.get() as i64,
            _ => self.data.borrow().len() as i64,
        };
        let p = (base + pos).max(0) as usize;
        self.pos.set(p);
        if !result.is_null() {
            *result = p as i64;
        }
        kResultOk
    }
    unsafe fn tell(&self, pos: *mut i64) -> tresult {
        if !pos.is_null() {
            *pos = self.pos.get() as i64;
        }
        kResultOk
    }
}

// ---------- IComponentHandler : éditions de l'UI → file vers le thread audio ----------

pub struct ComponentHandler {
    pub edits: Mutex<rtrb::Producer<(u32, f64)>>,
}
impl Class for ComponentHandler {
    type Interfaces = (IComponentHandler,);
}
impl IComponentHandlerTrait for ComponentHandler {
    unsafe fn beginEdit(&self, _id: ParamID) -> tresult {
        kResultOk
    }
    unsafe fn performEdit(&self, id: ParamID, value: ParamValue) -> tresult {
        // Seul le thread UI verrouille ce mutex : jamais de contention avec le thread audio.
        let _ = self.edits.lock().unwrap().push((id, value));
        kResultOk
    }
    unsafe fn endEdit(&self, _id: ParamID) -> tresult {
        kResultOk
    }
    unsafe fn restartComponent(&self, _flags: i32) -> tresult {
        // Changement de latence ou de bus après chargement : non suivi (la latence affichée
        // reste celle du chargement).
        kResultOk
    }
}

// ---------- IParameterChanges temps réel (pré-alloué, sans allocation) ----------

const MAX_POINTS: usize = 16;

pub struct ParamQueue {
    id: Cell<u32>,
    count: Cell<usize>,
    points: UnsafeCell<[(i32, f64); MAX_POINTS]>,
}
impl Class for ParamQueue {
    type Interfaces = (IParamValueQueue,);
}
impl IParamValueQueueTrait for ParamQueue {
    unsafe fn getParameterId(&self) -> ParamID {
        self.id.get()
    }
    unsafe fn getPointCount(&self) -> i32 {
        self.count.get() as i32
    }
    unsafe fn getPoint(&self, index: i32, offset: *mut i32, value: *mut f64) -> tresult {
        if index < 0 || index as usize >= self.count.get() {
            return kInvalidArgument;
        }
        let (o, v) = (*self.points.get())[index as usize];
        *offset = o;
        *value = v;
        kResultOk
    }
    unsafe fn addPoint(&self, offset: i32, value: f64, index: *mut i32) -> tresult {
        let n = self.count.get();
        if n >= MAX_POINTS {
            return kResultFalse;
        }
        (*self.points.get())[n] = (offset, value);
        self.count.set(n + 1);
        if !index.is_null() {
            *index = n as i32;
        }
        kResultOk
    }
}

pub struct ParamChanges {
    queues: Vec<ComWrapper<ParamQueue>>,
    ptrs: Vec<*mut IParamValueQueue>,
    used: Cell<usize>,
}
impl ParamChanges {
    pub fn new(cap: usize) -> Self {
        let queues: Vec<_> = (0..cap)
            .map(|_| {
                ComWrapper::new(ParamQueue {
                    id: Cell::new(0),
                    count: Cell::new(0),
                    points: UnsafeCell::new([(0, 0.0); MAX_POINTS]),
                })
            })
            .collect();
        let ptrs = queues
            .iter()
            .map(|q| q.as_com_ref::<IParamValueQueue>().unwrap().as_ptr())
            .collect();
        ParamChanges {
            queues,
            ptrs,
            used: Cell::new(0),
        }
    }
    pub fn clear(&self) {
        self.used.set(0);
    }
    /// Dernière valeur gagne : un point unique à l'offset 0 par paramètre et par bloc.
    pub fn set(&self, id: u32, value: f64) {
        let used = self.used.get();
        let q = match self.queues[..used].iter().find(|q| q.id.get() == id) {
            Some(q) => q,
            None if used < self.queues.len() => {
                let q = &self.queues[used];
                q.id.set(id);
                self.used.set(used + 1);
                q
            }
            None => return,
        };
        unsafe { (*q.points.get())[0] = (0, value) };
        q.count.set(1);
    }
}
impl Class for ParamChanges {
    type Interfaces = (IParameterChanges,);
}
impl IParameterChangesTrait for ParamChanges {
    unsafe fn getParameterCount(&self) -> i32 {
        self.used.get() as i32
    }
    unsafe fn getParameterData(&self, index: i32) -> *mut IParamValueQueue {
        if index < 0 || index as usize >= self.used.get() {
            return std::ptr::null_mut();
        }
        self.ptrs[index as usize]
    }
    unsafe fn addParameterData(
        &self,
        _id: *const ParamID,
        _index: *mut i32,
    ) -> *mut IParamValueQueue {
        // Uniquement utilisé pour les changements de sortie, qu'on ne fournit pas.
        std::ptr::null_mut()
    }
}
