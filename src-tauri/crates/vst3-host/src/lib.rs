//! Hôte VST3 de la voie guitare (spec §5.6 ; voie A du spike, cf. RESULTATS.md) : chargement des
//! modules par OS, instance de plugin et sa partie temps réel, état. Toutes les méthodes non
//! temps réel s'appellent sur le **thread principal** ; seule la partie temps réel (`GuitarFx`)
//! part sur le thread audio.

pub mod editor;
mod host_objects;
mod module;
mod plugin;
pub mod scan;

pub use editor::Editor;
pub use module::probe;
pub use plugin::Vst3Plugin;
