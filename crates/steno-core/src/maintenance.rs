//! Maintenance périodique : battement de cœur, sauvegarde quotidienne, rétention.

use crate::core::Inner;
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;

pub fn spawn(inner: Arc<Inner>) {
    // Battement de cœur (la CLI sait ainsi que l'app traite la file de tâches). A single
    // process owns it: see `Db::beat_heartbeat`.
    let i = inner.clone();
    tokio::spawn(async move {
        let pid = std::process::id();
        loop {
            let _ = i.db.beat_heartbeat(pid);
            if let Err(e) = i.reload_settings_if_changed() {
                tracing::warn!("settings reload: {e:#}");
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                _ = i.shutdown.cancelled() => {
                    let _ = i.db.release_heartbeat(pid);
                    break;
                }
            }
        }
    });
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            if let Err(e) = run_once(&inner) {
                tracing::warn!("maintenance : {e:#}");
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(6 * 3600)) => {}
                _ = inner.shutdown.cancelled() => break,
            }
        }
    });
}

/// Sauvegarde du jour (`VACUUM INTO`), 7 dernières conservées.
pub fn daily_backup(inner: &Inner) -> Result<()> {
    if inner.db.path().is_none() {
        return Ok(());
    }
    let dir = crate::paths::backups_dir();
    crate::paths::create_private_dir(&dir)?;
    let name = format!("steno-{}.db", chrono::Local::now().format("%Y-%m-%d"));
    let dest = dir.join(&name);
    if !dest.exists() {
        inner.db.backup_to(&dest)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o600));
        }
    }
    let mut all: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).map(|n| n.starts_with("steno-") && n.ends_with(".db")).unwrap_or(false))
        .collect();
    all.sort();
    while all.len() > 7 {
        let p = all.remove(0);
        let _ = std::fs::remove_file(p);
    }
    Ok(())
}

pub fn run_once(inner: &Inner) -> Result<()> {
    daily_backup(inner)?;
    let s = inner.settings();
    if let Some(days) = s.privacy.transcript_retention_days.filter(|d| *d > 0) {
        let before = (chrono::Local::now() - chrono::Duration::days(days as i64)).to_rfc3339();
        let n = inner.db.purge_transcripts_before(&before)?;
        if n > 0 {
            tracing::info!("rétention : {n} phrases de transcription supprimées");
        }
    }
    // Audio : supprimer les tampons plus vieux que la rétention (0 = dès que traités).
    let cache = crate::paths::cache_dir();
    if let Ok(rd) = std::fs::read_dir(&cache) {
        let max_age = Duration::from_secs(s.privacy.audio_retention_days as u64 * 86400);
        let live = inner.db.live_meeting()?.map(|m| m.id);
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if Some(&name) == live.as_ref() {
                continue;
            }
            let pending = inner.db.has_job("report", &name, &["pending", "running"]).unwrap_or(false)
                || inner.db.has_job("diarize", &name, &["pending", "running"]).unwrap_or(false);
            if pending {
                continue;
            }
            let age = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).unwrap_or_default();
            if age >= max_age {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    Ok(())
}
