//! Logique audio pure du moteur natif (zéro dépendance système, cf. ARCHITECTURE.md §15) :
//! détection du silence de tête, décodage, lecteur temps réel.

pub mod decode;
pub mod player;
pub mod silence;
