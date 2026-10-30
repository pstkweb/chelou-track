//! Logique audio pure du moteur natif (zéro dépendance système, cf. ARCHITECTURE.md §15) :
//! détection du silence de tête, décodage, lecteur temps réel, voie guitare,
//! réglages audio persistés.

pub mod decode;
pub mod gain;
pub mod guitar;
pub mod input;
pub mod player;
pub mod settings;
pub mod silence;
