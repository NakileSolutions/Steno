//! Emplacements des fichiers. `SCRIBE_HOME` remplace tout (tests, Linux, dev).

use std::path::PathBuf;

/// Dossier des données : `~/Library/Application Support/Scribe` sur macOS.
pub fn home() -> PathBuf {
    if let Ok(h) = std::env::var("SCRIBE_HOME") {
        if !h.is_empty() {
            return PathBuf::from(h);
        }
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Scribe")
}

pub fn db_path() -> PathBuf {
    home().join("scribe.db")
}

/// Tampons audio temporaires : `~/Library/Caches/Scribe`.
pub fn cache_dir() -> PathBuf {
    if std::env::var("SCRIBE_HOME").map(|h| !h.is_empty()).unwrap_or(false) {
        return home().join("cache");
    }
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Scribe")
}

/// Dossiers d'exécution des agents (un par réunion, stable pour la reprise de session).
pub fn runs_dir() -> PathBuf {
    home().join("runs")
}

pub fn backups_dir() -> PathBuf {
    home().join("backups")
}

pub fn logs_dir() -> PathBuf {
    home().join("logs")
}

pub fn ensure_dirs() -> std::io::Result<()> {
    for d in [home(), cache_dir(), runs_dir(), backups_dir(), logs_dir()] {
        std::fs::create_dir_all(&d)?;
    }
    Ok(())
}
