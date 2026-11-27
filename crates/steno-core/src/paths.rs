//! Emplacements des fichiers. `STENO_HOME` remplace tout (tests, Linux, dev).

use std::path::PathBuf;

/// Dossier des données : `~/Library/Application Support/Steno` sur macOS,
/// `~/.local/share/Steno` sous Linux (`$XDG_DATA_HOME/Steno`).
pub fn home() -> PathBuf {
    if let Ok(h) = std::env::var("STENO_HOME") {
        if !h.is_empty() {
            return PathBuf::from(h);
        }
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Steno")
}

pub fn db_path() -> PathBuf {
    home().join("steno.db")
}

/// Tampons audio temporaires : `~/Library/Caches/Steno` sur macOS, `~/.cache/Steno` sous Linux.
pub fn cache_dir() -> PathBuf {
    if std::env::var("STENO_HOME").map(|h| !h.is_empty()).unwrap_or(false) {
        return home().join("cache");
    }
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Steno")
}

/// Dossiers d'exécution des agents (un par réunion, stable pour la reprise de session).
pub fn runs_dir() -> PathBuf {
    home().join("runs")
}

/// Identifiant de réunion utilisable comme nom de dossier : 1 à 128 caractères parmi
/// `A-Z a-z 0-9 _ -` (jamais `.`, `..` ni `/`).
pub fn valid_meeting_id(id: &str) -> bool {
    (1..=128).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Dossier propre à une réunion, enfant direct de `base` (`runs`, `cache`) ; `None` si
/// l'identifiant ne peut pas désigner un tel dossier.
pub fn meeting_subdir(base: &std::path::Path, id: &str) -> Option<PathBuf> {
    if !valid_meeting_id(id) {
        return None;
    }
    let dir = base.join(id);
    (dir.parent() == Some(base) && dir.file_name().map(|n| n == id).unwrap_or(false)).then_some(dir)
}

pub fn backups_dir() -> PathBuf {
    home().join("backups")
}

pub fn logs_dir() -> PathBuf {
    home().join("logs")
}

/// Modèles audio du sidecar Linux (Parakeet, Silero…) : `STENO_MODELS_DIR`, sinon
/// `$STENO_HOME/models` si `STENO_HOME` est fixé, sinon `~/.local/share/Steno/models`.
/// (Sur macOS, le sidecar Swift range ses modèles lui-même, via FluidAudio.)
pub fn models_dir() -> PathBuf {
    models_dir_from(
        std::env::var("STENO_MODELS_DIR").ok().as_deref(),
        std::env::var("STENO_HOME").ok().as_deref(),
        dirs::data_dir(),
    )
}

fn models_dir_from(models: Option<&str>, steno_home: Option<&str>, data: Option<PathBuf>) -> PathBuf {
    if let Some(m) = models.filter(|m| !m.is_empty()) {
        return PathBuf::from(m);
    }
    if let Some(h) = steno_home.filter(|h| !h.is_empty()) {
        return PathBuf::from(h).join("models");
    }
    data.unwrap_or_else(|| PathBuf::from(".")).join("Steno").join("models")
}

/// Reprend les données de l'ancien nom de l'app (« Scribe ») : dossiers des données et des
/// tampons, et base `scribe.db` → `steno.db` (avec ses fichiers WAL). Sans effet si les
/// nouveaux emplacements existent déjà ou si `STENO_HOME` est fixé.
pub fn migrate_legacy() -> std::io::Result<()> {
    if std::env::var("STENO_HOME").map(|h| !h.is_empty()).unwrap_or(false) {
        return Ok(());
    }
    for (base, new) in [(dirs::data_dir(), home()), (dirs::cache_dir(), cache_dir())] {
        let Some(old) = base.map(|b| b.join("Scribe")) else { continue };
        if old.is_dir() && !new.exists() {
            std::fs::rename(&old, &new)?;
        }
    }
    migrate_db_file(&home())
}

fn migrate_db_file(dir: &std::path::Path) -> std::io::Result<()> {
    if dir.join("steno.db").exists() || !dir.join("scribe.db").exists() {
        return Ok(());
    }
    for suffix in ["", "-wal", "-shm"] {
        let old = dir.join(format!("scribe.db{suffix}"));
        if old.exists() {
            std::fs::rename(&old, dir.join(format!("steno.db{suffix}")))?;
        }
    }
    Ok(())
}

pub fn ensure_dirs() -> std::io::Result<()> {
    if let Err(e) = migrate_legacy() {
        eprintln!("reprise des données de l'ancien nom impossible : {e}");
    }
    for d in [home(), cache_dir(), runs_dir(), backups_dir(), logs_dir()] {
        create_private_dir(&d)?;
    }
    Ok(())
}

/// Crée un dossier (et ses parents) lisible par l'utilisateur seul (0700 sous Unix) ; un
/// dossier existant est ramené à 0700 quand c'est possible. Transcriptions, invites et
/// journaux y sont rangés.
pub fn create_private_dir(dir: &std::path::Path) -> std::io::Result<()> {
    if dir.as_os_str().is_empty() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// Écrit un fichier lisible par l'utilisateur seul (0600 sous Unix), y compris s'il existait.
pub fn write_private(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        opts.mode(0o600);
        let mut f = opts.open(path)?;
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        f.write_all(data)
    }
    #[cfg(not(unix))]
    {
        opts.open(path)?.write_all(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dossier_des_modeles() {
        let data = Some(PathBuf::from("/home/u/.local/share"));
        assert_eq!(models_dir_from(Some("/m"), Some("/h"), data.clone()), PathBuf::from("/m"));
        assert_eq!(models_dir_from(Some(""), Some("/h"), data.clone()), PathBuf::from("/h/models"));
        assert_eq!(models_dir_from(None, None, data), PathBuf::from("/home/u/.local/share/Steno/models"));
    }

    #[test]
    fn reprise_de_la_base_de_l_ancien_nom() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("scribe.db"), b"db").unwrap();
        std::fs::write(d.path().join("scribe.db-wal"), b"wal").unwrap();
        migrate_db_file(d.path()).unwrap();
        assert_eq!(std::fs::read(d.path().join("steno.db")).unwrap(), b"db");
        assert_eq!(std::fs::read(d.path().join("steno.db-wal")).unwrap(), b"wal");
        assert!(!d.path().join("scribe.db").exists());
        // Déjà migrée : rien ne bouge.
        std::fs::write(d.path().join("scribe.db"), b"ancienne").unwrap();
        migrate_db_file(d.path()).unwrap();
        assert_eq!(std::fs::read(d.path().join("steno.db")).unwrap(), b"db");
    }

    #[test]
    fn identifiants_de_reunion_sans_traversee() {
        let base = std::path::Path::new("/donnees/runs");
        for id in ["2026-11-27-1030-daily-orion", "_global", "_test", "m1", "a_b-C9"] {
            assert_eq!(meeting_subdir(base, id), Some(base.join(id)), "{id}");
        }
        for id in ["", ".", "..", "../..", "../steno.db", "a/b", "/etc", "a\\b", "réunion", "a b", "x\0y", "x".repeat(129).as_str()] {
            assert!(meeting_subdir(base, id).is_none(), "{id:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn dossiers_et_fichiers_prives() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("runs/reunion");
        create_private_dir(&dir).unwrap();
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        create_private_dir(&dir).unwrap();
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700, "dossier existant resserré");
        let f = dir.join("prompt.md");
        std::fs::write(&f, "ancien").unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_private(&f, b"transcription").unwrap();
        assert_eq!(std::fs::metadata(&f).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::read(&f).unwrap(), b"transcription");
    }
}
