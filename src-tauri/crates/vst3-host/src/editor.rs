// Interface native du plugin dans une fenêtre de premier niveau (spec §5.6), créée et détruite
// sur le thread principal. Barre de titre Windows standard, aux couleurs de l'app : icône de
// l'exécutable, barre sombre en thème sombre. Pas de boucle de messages ici : celle de l'app (Tauri) pompe déjà les
// messages Win32 de ce thread.
#![allow(non_snake_case)]

#[cfg(not(windows))]
use crate::Vst3Plugin;
#[cfg(not(windows))]
use anyhow::Result;

/// Fenêtre de l'éditeur. **Thread principal uniquement** ; à fermer avant de libérer le plugin.
pub struct Editor {
    #[cfg(windows)]
    hwnd: windows::Win32::Foundation::HWND,
}

#[cfg(not(windows))]
impl Editor {
    pub fn open(_plugin: &Vst3Plugin, _title: &str, _dark: bool) -> Result<Editor> {
        anyhow::bail!("L'interface du plugin n'est disponible que sur Windows pour l'instant")
    }

    pub fn is_open(&self) -> bool {
        false
    }

    pub fn focus(&self) {}

    pub fn close(self) {}
}

#[cfg(windows)]
pub use win::pump_messages;

#[cfg(windows)]
mod win {
    use std::cell::Cell;

    use anyhow::{anyhow, bail, Result};
    use std::sync::OnceLock;
    use vst3::Steinberg::Vst::IEditControllerTrait;
    use vst3::Steinberg::*;
    use vst3::{Class, ComPtr, ComRef, ComWrapper};
    use windows::core::{w, BOOL, PCWSTR};

    use windows::Win32::Foundation::*;
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
    use windows::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW};
    use windows::Win32::UI::Shell::ExtractIconExW;
    use windows::Win32::UI::WindowsAndMessaging::*;

    use super::Editor;
    use crate::Vst3Plugin;

    const STYLE: WINDOW_STYLE =
        WINDOW_STYLE(WS_OVERLAPPED.0 | WS_CAPTION.0 | WS_SYSMENU.0 | WS_MINIMIZEBOX.0);

    fn outer_size(r: &ViewRect) -> (i32, i32) {
        let mut rc = RECT {
            left: 0,
            top: 0,
            right: r.right - r.left,
            bottom: r.bottom - r.top,
        };
        // SAFETY: calcul pur sur un RECT local.
        unsafe {
            let _ = AdjustWindowRectEx(&mut rc, STYLE, false, WINDOW_EX_STYLE(0));
        }
        (rc.right - rc.left, rc.bottom - rc.top)
    }

    /// `IPlugFrame` : le plugin demande un redimensionnement.
    struct PlugFrame {
        hwnd: Cell<isize>,
    }

    impl Class for PlugFrame {
        type Interfaces = (IPlugFrame,);
    }

    impl IPlugFrameTrait for PlugFrame {
        unsafe fn resizeView(&self, view: *mut IPlugView, size: *mut ViewRect) -> tresult {
            if size.is_null() {
                return kInvalidArgument;
            }
            let (w, h) = outer_size(&*size);
            let _ = SetWindowPos(
                HWND(self.hwnd.get() as *mut _),
                None,
                0,
                0,
                w,
                h,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            if let Some(v) = ComRef::<IPlugView>::from_raw(view) {
                v.onSize(size);
            }
            kResultOk
        }
    }

    /// Vue attachée à la fenêtre (`GWLP_USERDATA`), libérée à `WM_NCDESTROY`.
    struct ViewState {
        view: ComPtr<IPlugView>,
        /// Gardé vivant tant que la vue peut l'appeler.
        _frame: ComWrapper<PlugFrame>,
        attached: Cell<bool>,
    }

    impl ViewState {
        /// Détache la vue (idempotent) : toujours **avant** de détruire la fenêtre parente.
        fn detach(&self) {
            if self.attached.replace(false) {
                // SAFETY: vue attachée par `Editor::open`, sur ce thread.
                unsafe {
                    self.view.removed();
                    self.view.setFrame(std::ptr::null_mut());
                }
            }
        }
    }

    /// Icônes (grande, petite) de l'exécutable de l'app, extraites une fois (handles bruts : vivent
    /// jusqu'à la fin du processus).
    fn app_icons() -> (isize, isize) {
        static ICONS: OnceLock<(isize, isize)> = OnceLock::new();
        *ICONS.get_or_init(|| {
            let mut path = [0u16; 1024];
            let mut large = HICON::default();
            let mut small = HICON::default();
            // SAFETY: tampon local ; `ExtractIconExW` écrit au plus une icône de chaque taille.
            unsafe {
                let len = GetModuleFileNameW(None, &mut path) as usize;
                if len > 0 && len < path.len() {
                    ExtractIconExW(
                        PCWSTR(path.as_ptr()),
                        0,
                        Some(&mut large),
                        Some(&mut small),
                        1,
                    );
                }
            }
            (large.0 as isize, small.0 as isize)
        })
    }

    /// Icône de l'app et barre de titre sombre (Windows 11 ; sans effet ailleurs).
    fn style_frame(hwnd: HWND, dark: bool) {
        let (large, small) = app_icons();
        // SAFETY: messages et attribut DWM standard sur une fenêtre de ce thread.
        unsafe {
            if large != 0 {
                SendMessageW(
                    hwnd,
                    WM_SETICON,
                    Some(WPARAM(ICON_BIG as usize)),
                    Some(LPARAM(large)),
                );
            }
            if small != 0 {
                SendMessageW(
                    hwnd,
                    WM_SETICON,
                    Some(WPARAM(ICON_SMALL as usize)),
                    Some(LPARAM(small)),
                );
            }
            let value = BOOL::from(dark);
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &value as *const BOOL as *const _,
                std::mem::size_of::<BOOL>() as u32,
            );
        }
    }

    fn state(hwnd: HWND) -> *const ViewState {
        // SAFETY: lecture du pointeur rangé par `Editor::open` (0 après `WM_NCDESTROY`).
        unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const ViewState }
    }

    /// Détache la vue puis détruit la fenêtre.
    fn close_window(hwnd: HWND) {
        let st = state(hwnd);
        if !st.is_null() {
            // SAFETY: pointeur vivant jusqu'à `WM_NCDESTROY`.
            unsafe { (*st).detach() };
        }
        // SAFETY: fenêtre créée sur ce thread.
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }

    unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        match msg {
            WM_CLOSE => {
                close_window(hwnd);
                LRESULT(0)
            }
            WM_NCDESTROY => {
                let st = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) as *mut ViewState;
                if !st.is_null() {
                    let st = Box::from_raw(st);
                    st.detach();
                }
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }

    impl Editor {
        /// `dark` : thème sombre de l'app (barre de titre sombre).
        pub fn open(plugin: &Vst3Plugin, title: &str, dark: bool) -> Result<Editor> {
            // SAFETY: API IPlugView + Win32, sur le thread principal (celui du plugin).
            unsafe {
                let view_raw = plugin.controller().createView(Vst::ViewType::kEditor);
                let view = ComPtr::<IPlugView>::from_raw(view_raw)
                    .ok_or_else(|| anyhow!("Ce plugin n'a pas d'interface"))?;
                if view.isPlatformTypeSupported(kPlatformTypeHWND) != kResultTrue {
                    bail!("Interface du plugin non prise en charge (HWND)");
                }
                let mut rect: ViewRect = std::mem::zeroed();
                view.getSize(&mut rect);
                let (w, h) = outer_size(&rect);

                let hinst = GetModuleHandleW(None)?;
                let class = w!("ChelouTrackPluginEditor");
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(wndproc),
                    hInstance: hinst.into(),
                    lpszClassName: class,
                    hCursor: LoadCursorW(None, IDC_ARROW)?,
                    ..Default::default()
                };
                RegisterClassW(&wc); // échoue sans conséquence si déjà enregistrée
                let wtitle: Vec<u16> = title.encode_utf16().chain([0]).collect();
                let hwnd = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    class,
                    PCWSTR(wtitle.as_ptr()),
                    STYLE,
                    CW_USEDEFAULT,
                    CW_USEDEFAULT,
                    w,
                    h,
                    None,
                    None,
                    Some(hinst.into()),
                    None,
                )?;

                let frame = ComWrapper::new(PlugFrame {
                    hwnd: Cell::new(hwnd.0 as isize),
                });
                view.setFrame(frame.as_com_ref::<IPlugFrame>().unwrap().as_ptr());
                let st = Box::new(ViewState {
                    view,
                    _frame: frame,
                    attached: Cell::new(false),
                });
                if st.view.attached(hwnd.0, kPlatformTypeHWND) != kResultOk {
                    st.view.setFrame(std::ptr::null_mut());
                    let _ = DestroyWindow(hwnd);
                    bail!("Impossible d'afficher l'interface du plugin");
                }
                st.attached.set(true);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(st) as isize);
                style_frame(hwnd, dark);
                let _ = ShowWindow(hwnd, SW_SHOW);
                Ok(Editor { hwnd })
            }
        }

        pub fn is_open(&self) -> bool {
            // SAFETY: simple test de validité du handle.
            unsafe { IsWindow(Some(self.hwnd)).as_bool() }
        }

        /// Ramène la fenêtre au premier plan.
        pub fn focus(&self) {
            // SAFETY: fenêtre de ce thread.
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(self.hwnd);
            }
        }

        pub fn close(self) {}
    }

    impl Drop for Editor {
        fn drop(&mut self) {
            if self.is_open() {
                close_window(self.hwnd);
            }
        }
    }

    /// Pompe les messages en attente du thread (pour les exemples ; l'app a la boucle de Tauri).
    pub fn pump_messages() {
        let mut msg = MSG::default();
        // SAFETY: boucle de messages standard du thread courant.
        unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}
