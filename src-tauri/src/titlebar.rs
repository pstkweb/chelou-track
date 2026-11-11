//! macOS : feux tricolores natifs centrés dans la barre de titre de l'app (`TitleBar.tsx`,
//! `titleBarStyle: "Overlay"` dans `tauri.macos.conf.json`). Pas de `trafficLightPosition` : tao
//! ne fixe que la hauteur du conteneur et laisse l'ordonnée des boutons à celle d'AppKit, qui
//! varie selon la version de macOS (feux collés en haut sur macOS 26+). AppKit refait la mise en
//! page de la barre à chaque redimensionnement / sortie de plein écran : réappliqué alors.
use objc2_app_kit::{NSWindow, NSWindowButton};
use objc2_foundation::NSPoint;

/// Hauteur de `.titlebar` (`components.css`, `h-9.5`).
const BAR_HEIGHT: f64 = 38.0;
/// Bord gauche du bouton fermer (= `px-3.5` de `.titlebar`).
const LEFT: f64 = 14.0;

/// **Thread principal** (setup, évènements de fenêtre).
pub fn position_traffic_lights<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) {
    let Ok(ptr) = window.ns_window() else {
        return;
    };
    // SAFETY: `ns_window` renvoie la `NSWindow` vivante de cette fenêtre ; appelé sur le thread
    // principal.
    let ns_window = unsafe { &*(ptr as *const NSWindow) };
    let buttons = [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ]
    .map(|b| ns_window.standardWindowButton(b));
    let [Some(close), Some(mini), Some(zoom)] = buttons else {
        return; // plein écran : pas de boutons
    };
    // SAFETY: vues de la barre système, vivantes tant que la fenêtre l'est (thread principal).
    let Some(titlebar) = (unsafe { close.superview() }) else {
        return;
    };
    // SAFETY: idem.
    let Some(container) = (unsafe { titlebar.superview() }) else {
        return;
    };

    // Conteneur de la barre système à la hauteur de la nôtre, collé en haut de la fenêtre.
    let mut rect = container.frame();
    rect.size.height = BAR_HEIGHT;
    rect.origin.y = ns_window.frame().size.height - BAR_HEIGHT;
    container.setFrame(rect);

    // Coordonnées AppKit (origine en bas) : centre des boutons à BAR_HEIGHT / 2 du haut.
    let size = close.frame().size;
    let spacing = mini.frame().origin.x - close.frame().origin.x;
    let y = titlebar.frame().size.height - (BAR_HEIGHT + size.height) / 2.0;
    for (i, button) in [close, mini, zoom].iter().enumerate() {
        button.setFrameOrigin(NSPoint::new(LEFT + i as f64 * spacing, y));
    }
}
