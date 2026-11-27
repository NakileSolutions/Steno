//! Mises à jour : détection des nouvelles versions publiées sur GitHub et installation depuis
//! les sources.
//!
//! Sténo est compilé sur le Mac de chaque utilisateur (`make install`, signature ad hoc) : une
//! mise à jour suit le même chemin. On lit la dernière version publiée (API GitHub, sans
//! authentification ni donnée envoyée), puis, à la demande ou automatiquement, on amène le
//! clone des sources sur le tag de cette version et on relance `make install`. L'app installée
//! est remplacée à la fin seulement : un échec de compilation laisse la version courante en
//! place. Le redémarrage reste à l'initiative de l'utilisateur (jamais pendant une réunion).

use crate::core::Inner;
use crate::events::CoreEvent;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

/// Dépôt public dont on suit les versions.
pub const REPO: &str = "NakileSolutions/Steno";
/// Version de ce binaire.
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// État des mises à jour, tel qu'affiché par l'interface (`steno://update`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct UpdateStatus {
    pub current: String,
    /// Dernière version publiée (sans le « v »), si connue.
    pub latest: Option<String>,
    /// Une version plus récente que `current` est publiée.
    pub available: bool,
    /// Page de la version (notes), notes en Markdown, date de publication.
    pub url: Option<String>,
    pub notes: Option<String>,
    pub published_at: Option<String>,
    pub checked_at: Option<String>,
    /// `idle | checking | installing | installed | error`.
    pub state: String,
    /// Dernier message (erreur, ou raison pour laquelle l'installation automatique est impossible).
    pub message: Option<String>,
    /// Dossier des sources utilisé pour l'installation, s'il est utilisable.
    pub source_dir: Option<String>,
    /// L'installation depuis l'app est possible (sources trouvées et propres).
    pub can_install: bool,
    /// Journal de la dernière installation.
    pub log_path: Option<String>,
}

/// Compare deux versions `x.y.z` (suffixes ignorés). `true` si `a` est plus récente que `b`.
pub fn newer(a: &str, b: &str) -> bool {
    fn parts(v: &str) -> Vec<u64> {
        v.trim()
            .trim_start_matches('v')
            .split(['-', '+'])
            .next()
            .unwrap_or("")
            .split('.')
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    }
    let (a, b) = (parts(a), parts(b));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    html_url: Option<String>,
    body: Option<String>,
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// Interprète la réponse de `GET /repos/{REPO}/releases/latest`.
fn parse_release(json: &str) -> Result<Option<Release>> {
    let r: Release = serde_json::from_str(json).context("réponse GitHub illisible")?;
    Ok((!r.draft && !r.prerelease).then_some(r))
}

/// Dernière version publiée. `Ok(None)` : aucune version publiée.
fn fetch_latest() -> Result<Option<Release>> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let out = Command::new("/usr/bin/curl")
        .args(["-sS", "--max-time", "20", "-w", "\n%{http_code}"])
        .args(["-H", "Accept: application/vnd.github+json", "-H", "User-Agent: steno-updater"])
        .arg(&url)
        .output()
        .context("curl introuvable")?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let (body, code) = text.rsplit_once('\n').unwrap_or((text.as_str(), ""));
    match code.trim() {
        "200" => parse_release(body),
        "404" => Ok(None),
        "" => bail!("GitHub injoignable : {}", String::from_utf8_lossy(&out.stderr).trim()),
        c => bail!("GitHub a répondu {c}"),
    }
}

/// Dossier des sources : `STENO_SOURCE_DIR`, sinon le clone dans lequel ce binaire a été compilé.
pub fn source_dir() -> Option<PathBuf> {
    let dir = std::env::var("STENO_SOURCE_DIR")
        .ok()
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."));
    let dir = dir.canonicalize().ok()?;
    (dir.join(".git").exists() && dir.join("Makefile").exists()).then_some(dir)
}

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().context("git introuvable")?;
    if !out.status.success() {
        bail!("git {} : {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Raison pour laquelle les sources ne peuvent pas être mises à jour automatiquement.
pub fn install_blocker(dir: Option<&Path>) -> Option<String> {
    let Some(dir) = dir else {
        return Some("dossier des sources introuvable (Sténo a-t-il été installé avec `make install` ?)".into());
    };
    match git(dir, &["status", "--porcelain", "--untracked-files=no"]) {
        Ok(s) if !s.is_empty() => Some("les sources contiennent des modifications locales : mettez à jour à la main".into()),
        Ok(_) => match git(dir, &["symbolic-ref", "-q", "--short", "HEAD"]) {
            Ok(b) if b != "main" => Some(format!("les sources sont sur la branche « {b} » : mettez à jour à la main")),
            _ => None,
        },
        Err(e) => Some(format!("sources illisibles : {e:#}")),
    }
}

impl Inner {
    pub fn update_status(&self) -> UpdateStatus {
        self.update.lock().unwrap().clone()
    }

    fn set_update(&self, f: impl FnOnce(&mut UpdateStatus)) {
        let st = {
            let mut u = self.update.lock().unwrap();
            f(&mut u);
            u.clone()
        };
        self.emit(CoreEvent::Update(st));
    }
}

/// État initial (avant toute vérification).
pub fn initial_status() -> UpdateStatus {
    let dir = source_dir();
    let blocker = install_blocker(dir.as_deref());
    UpdateStatus {
        current: CURRENT.into(),
        state: "idle".into(),
        source_dir: dir.map(|d| d.display().to_string()),
        can_install: blocker.is_none(),
        message: blocker,
        ..Default::default()
    }
}

/// Vérifie la dernière version publiée ; notifie une seule fois par nouvelle version.
pub async fn check(inner: &Inner) -> Result<UpdateStatus> {
    if matches!(inner.update_status().state.as_str(), "installing") {
        return Ok(inner.update_status());
    }
    inner.set_update(|u| u.state = "checking".into());
    let res = tokio::task::spawn_blocking(fetch_latest).await.context("vérification interrompue")?;
    let dir = source_dir();
    let blocker = install_blocker(dir.as_deref());
    match res {
        Ok(rel) => {
            let latest = rel.as_ref().map(|r| r.tag_name.trim_start_matches('v').to_string());
            let available = latest.as_deref().map(|l| newer(l, CURRENT)).unwrap_or(false);
            let was_installed = inner.update_status().state == "installed";
            inner.set_update(|u| {
                u.latest = latest.clone();
                u.available = available;
                u.url = rel.as_ref().and_then(|r| r.html_url.clone());
                u.notes = rel.as_ref().and_then(|r| r.body.clone());
                u.published_at = rel.as_ref().and_then(|r| r.published_at.clone());
                u.checked_at = Some(crate::now_iso());
                u.state = if was_installed { "installed".into() } else { "idle".into() };
                u.source_dir = dir.as_ref().map(|d| d.display().to_string());
                u.can_install = blocker.is_none();
                u.message = blocker.clone();
            });
            if let (true, Some(v)) = (available, latest.as_deref()) {
                let seen = inner.db.get_runtime("update_notified").ok().flatten().map(|(v, _)| v);
                if seen.as_deref() != Some(v) && !was_installed {
                    let _ = inner.db.set_runtime("update_notified", v);
                    inner.emit(CoreEvent::Notify {
                        title: if inner.lang() == "en" { format!("Sténo {v} is available") } else { format!("Sténo {v} est disponible") },
                        body: inner.tr("Cliquez pour voir les nouveautés et installer la mise à jour.", "Click to see what’s new and install the update."),
                        route: Some("/settings?section=updates".into()),
                    });
                }
            }
        }
        Err(e) => {
            let msg = format!("{e:#}");
            inner.set_update(|u| {
                u.state = "error".into();
                u.message = Some(format!("Vérification impossible : {msg}"));
                u.checked_at = Some(crate::now_iso());
            });
            return Err(e);
        }
    }
    Ok(inner.update_status())
}

/// Installe la dernière version publiée depuis les sources (quelques minutes).
pub async fn install(inner: Arc<Inner>) -> Result<()> {
    let st = inner.update_status();
    if st.state == "installing" {
        bail!("une installation est déjà en cours");
    }
    let Some(version) = st.latest.clone().filter(|_| st.available) else {
        bail!("aucune nouvelle version à installer");
    };
    if inner.db.live_meeting()?.is_some() {
        bail!("une réunion est en cours : installez la mise à jour après la réunion");
    }
    let dir = source_dir().context("dossier des sources introuvable")?;
    if let Some(why) = install_blocker(Some(&dir)) {
        bail!("{why}");
    }
    let log = crate::paths::logs_dir().join("update.log");
    inner.set_update(|u| {
        u.state = "installing".into();
        u.message = Some(format!("Installation de Sténo {version}… (quelques minutes)"));
        u.log_path = Some(log.display().to_string());
    });
    let tag = format!("v{version}");
    let i = inner.clone();
    let res = tokio::task::spawn_blocking(move || run_install(&dir, &tag, &log)).await.context("installation interrompue")?;
    match res {
        Ok(()) => {
            i.set_update(|u| {
                u.state = "installed".into();
                u.message = Some(format!("Sténo {version} est installé : redémarrez pour l'utiliser."));
            });
            i.emit(CoreEvent::Notify {
                title: if i.lang() == "en" { format!("Sténo {version} is installed") } else { format!("Sténo {version} est installé") },
                body: i.tr("Redémarrez Sténo pour utiliser la nouvelle version.", "Restart Sténo to use the new version."),
                route: Some("/settings?section=updates".into()),
            });
            Ok(())
        }
        Err(e) => {
            let msg = format!("{e:#}");
            i.set_update(|u| {
                u.state = "error".into();
                u.message = Some(format!("Installation en échec : {msg}"));
            });
            Err(e)
        }
    }
}

/// Amène les sources sur le tag, puis `make install` ; tout est journalisé dans `log`.
fn run_install(dir: &Path, tag: &str, log: &Path) -> Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(log)?;
    writeln!(f, "\n=== {} : installation de {tag} depuis {} ===", crate::now_iso(), dir.display())?;
    git(dir, &["fetch", "--tags", "--force", "origin"])?;
    git(dir, &["rev-parse", "--verify", &format!("{tag}^{{commit}}")]).with_context(|| format!("tag {tag} introuvable"))?;
    match git(dir, &["symbolic-ref", "-q", "--short", "HEAD"]) {
        Ok(b) if b == "main" => {
            git(dir, &["merge", "--ff-only", tag]).context("les sources ont divergé de la version publiée")?;
        }
        _ => {
            git(dir, &["checkout", "-q", tag])?;
        }
    }
    writeln!(f, "sources sur {tag} ; make install…")?;
    let out = Command::new("make")
        .arg("install")
        .current_dir(dir)
        .env("PATH", build_path())
        .stdout(f.try_clone()?)
        .stderr(f.try_clone()?)
        .status()
        .context("make introuvable (outils de développement Xcode installés ?)")?;
    if !out.success() {
        bail!("`make install` a échoué (voir {})", log.display());
    }
    writeln!(f, "=== terminé ===")?;
    Ok(())
}

/// PATH de la compilation : celui du processus, complété des emplacements habituels des outils
/// (rustup, Cargo, Homebrew, Node). Lancée depuis l'app ou un terminal minimal, `make` doit
/// trouver `cargo`, `rustc`, `npm` et `swift` comme lors de l'installation à la main.
pub fn build_path() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut dirs: Vec<String> = std::env::var("PATH").unwrap_or_default().split(':').filter(|d| !d.is_empty()).map(String::from).collect();
    for extra in [
        format!("{home}/.cargo/bin"),
        "/opt/homebrew/opt/rustup/bin".into(),
        "/opt/homebrew/bin".into(),
        "/usr/local/bin".into(),
        format!("{home}/.volta/bin"),
        "/usr/bin".into(),
        "/bin".into(),
    ] {
        if !dirs.contains(&extra) && Path::new(&extra).is_dir() {
            dirs.push(extra);
        }
    }
    dirs.join(":")
}

/// Vérification au démarrage puis toutes les 6 h ; installation automatique si demandée,
/// hors réunion.
pub fn spawn(inner: Arc<Inner>) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        loop {
            let s = inner.settings().updates;
            if s.check {
                if let Ok(st) = check(&inner).await {
                    let idle = inner.db.live_meeting().ok().flatten().is_none();
                    if s.auto_install && st.available && st.can_install && st.state == "idle" && idle {
                        if let Err(e) = install(inner.clone()).await {
                            tracing::warn!("installation automatique : {e:#}");
                        }
                    }
                }
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(6 * 3600)) => {}
                _ = inner.shutdown.cancelled() => break,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_de_compilation() {
        let p = build_path();
        assert!(p.split(':').any(|d| d == "/usr/bin"));
        let n = p.split(':').count();
        let uniq: std::collections::HashSet<_> = p.split(':').collect();
        assert_eq!(uniq.len(), n, "pas de doublon ajouté");
    }

    #[test]
    fn comparaison_des_versions() {
        assert!(newer("0.2.0", "0.1.0"));
        assert!(newer("v1.0.0", "0.9.9"));
        assert!(newer("0.1.10", "0.1.9"));
        assert!(!newer("0.1.0", "0.1.0"));
        assert!(!newer("0.1.0-beta", "0.1.0"));
        assert!(!newer("0.0.9", "0.1"));
    }

    #[test]
    fn lecture_de_la_derniere_version() {
        let r = parse_release(r#"{"tag_name":"v0.2.0","html_url":"https://github.com/x/y/releases/tag/v0.2.0","body":"- Nouveautés","published_at":"2026-12-01T10:00:00Z"}"#)
            .unwrap()
            .unwrap();
        assert_eq!(r.tag_name, "v0.2.0");
        assert!(parse_release(r#"{"tag_name":"v0.3.0-rc1","prerelease":true}"#).unwrap().is_none());
        assert!(parse_release("pas du json").is_err());
    }

    #[test]
    fn sources_modifiees_ou_autre_branche() {
        let d = tempfile::tempdir().unwrap();
        let g = |args: &[&str]| assert!(Command::new("git").arg("-C").arg(d.path()).args(args).output().unwrap().status.success());
        g(&["init", "-q", "-b", "main"]);
        g(&["-c", "user.email=t@t", "-c", "user.name=t", "-c", "commit.gpgsign=false", "commit", "-q", "--allow-empty", "-m", "x"]);
        std::fs::write(d.path().join("Makefile"), "all:\n").unwrap();
        assert!(install_blocker(Some(d.path())).is_none(), "fichier non suivi : sans effet");
        g(&["add", "Makefile"]);
        assert!(install_blocker(Some(d.path())).unwrap().contains("modifications locales"));
        g(&["-c", "user.email=t@t", "-c", "user.name=t", "-c", "commit.gpgsign=false", "commit", "-q", "-m", "m"]);
        g(&["checkout", "-q", "-b", "essai"]);
        assert!(install_blocker(Some(d.path())).unwrap().contains("essai"));
        assert!(install_blocker(None).is_some());
    }
}
