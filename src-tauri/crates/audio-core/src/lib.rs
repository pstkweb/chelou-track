//! Logique audio pure du moteur natif (zéro dépendance système, cf. ARCHITECTURE.md §15) :
//! détection du silence de tête, décodage, lecteur temps réel, voie guitare,
//! réglages audio persistés.

pub mod decode;
pub mod gain;
pub mod guitar;
pub mod input;
pub mod metronome;
pub mod mix;
pub mod player;
pub mod plugin_state;
pub mod plugins;
pub mod settings;
pub mod silence;

/// Taille max d'un bloc de mixage, en frames : un callback plus long est traité en plusieurs
/// fois. C'est aussi le `maxSamplesPerBlock` annoncé au plugin VST3 (la taille réelle des
/// callbacks varie selon le pilote).
pub const MAX_BLOCK: usize = 4096;
