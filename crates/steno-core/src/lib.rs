//! Cœur de Sténo : base SQLite, réglages, file de tâches, adaptateurs d'agent,
//! détection du mot d'appel, compte rendu et corrélation.
//!
//! Aucun nom de fournisseur d'IA ne doit apparaître hors de `agents/`.

pub mod agents;
pub mod api;
pub mod core;
pub mod context;
pub mod correlate;
pub mod db;
pub mod doctor;
pub mod echo;
pub mod events;
pub mod export;
pub mod instructions;
pub mod i18n;
pub mod jobs;
pub mod live;
pub mod maintenance;
pub mod model;
pub mod paths;
pub mod permissions;
pub mod planning;
pub mod policy;
pub mod prompts;
pub mod report;
pub mod settings;
pub mod sidecar;
pub mod text;
pub mod updates;
pub mod wake;

pub use crate::core::Core;
pub use db::Db;

/// Horodatage courant au format ISO 8601 avec fuseau local.
pub fn now_iso() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}
