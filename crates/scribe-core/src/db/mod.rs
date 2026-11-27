//! Accès SQLite : ouverture, pragmas, migrations, et requêtes (dans `queries.rs`).

mod queries;

pub use queries::NewSegment;
pub use queries::utc_in as queries_utc_in;

use anyhow::{Context, Result};
use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const MIGRATIONS: &[&str] = &[
    include_str!("migrations/001_init.sql"),
    include_str!("migrations/002_call_app.sql"),
    include_str!("migrations/003_context.sql"),
    include_str!("migrations/004_job_not_before.sql"),
];

fn migrations() -> Migrations<'static> {
    Migrations::new(MIGRATIONS.iter().map(|sql| M::up(sql)).collect())
}

/// Connexion partagée. Les requêtes sont courtes : un verrou suffit.
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
    path: Option<PathBuf>,
}

impl std::fmt::Debug for Db {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Db").field("path", &self.path).finish()
    }
}

fn configure(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "busy_timeout", 5000)?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(())
}

impl Db {
    /// Ouvre (ou crée) la base au chemin donné et applique les migrations.
    pub fn open(path: &Path) -> Result<Db> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("impossible de créer {}", dir.display()))?;
        }
        let existed = path.exists();
        let mut conn = Connection::open(path)
            .with_context(|| format!("impossible d'ouvrir la base {}", path.display()))?;
        configure(&conn)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        let migs = migrations();
        if existed {
            let current: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
            if current > 0 && (current as usize) < MIGRATIONS.len() {
                // Sauvegarde juste avant une migration.
                let backup = crate::paths::backups_dir().join(format!(
                    "avant-migration-v{}-{}.db",
                    current,
                    chrono::Local::now().format("%Y%m%d-%H%M%S")
                ));
                let _ = std::fs::create_dir_all(crate::paths::backups_dir());
                conn.execute("VACUUM INTO ?1", [backup.to_string_lossy().to_string()])
                    .context("sauvegarde avant migration")?;
            }
        }
        migs.to_latest(&mut conn).context("migration de la base")?;
        Ok(Db {
            conn: Arc::new(Mutex::new(conn)),
            path: Some(path.to_path_buf()),
        })
    }

    /// Ouvre la base par défaut (`paths::db_path()`).
    pub fn open_default() -> Result<Db> {
        Db::open(&crate::paths::db_path())
    }

    /// Base en mémoire (tests).
    pub fn open_in_memory() -> Result<Db> {
        let mut conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrations().to_latest(&mut conn)?;
        Ok(Db {
            conn: Arc::new(Mutex::new(conn)),
            path: None,
        })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Exécute une fonction avec la connexion.
    pub fn with<R>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<R>) -> Result<R> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        Ok(f(&conn)?)
    }

    /// Exécute une fonction dans une transaction.
    pub fn tx<R>(&self, f: impl FnOnce(&rusqlite::Transaction) -> rusqlite::Result<R>) -> Result<R> {
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let tx = conn.transaction()?;
        let r = f(&tx)?;
        tx.commit()?;
        Ok(r)
    }

    /// Copie cohérente de la base (`VACUUM INTO`).
    pub fn backup_to(&self, dest: &Path) -> Result<()> {
        if dest.exists() {
            std::fs::remove_file(dest)?;
        }
        self.with(|c| c.execute("VACUUM INTO ?1", [dest.to_string_lossy().to_string()]))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_valides() {
        migrations().validate().unwrap();
    }

    #[test]
    fn ouverture_fichier_et_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("scribe.db");
        let db = Db::open(&p).unwrap();
        db.with(|c| c.execute("INSERT INTO settings(key, value) VALUES ('a', '1')", []))
            .unwrap();
        drop(db);
        let db = Db::open(&p).unwrap();
        let v: String = db
            .with(|c| c.query_row("SELECT value FROM settings WHERE key='a'", [], |r| r.get(0)))
            .unwrap();
        assert_eq!(v, "1");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }
}
