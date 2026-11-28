//! Mises à jour : détection des nouvelles versions publiées sur GitHub et installation depuis
//! les sources.
//!
//! Sténo est compilé sur l'ordinateur de chaque utilisateur (`make install`) : une
//! mise à jour suit le même chemin. On lit la dernière version publiée (API GitHub, sans
//! authentification ni donnée envoyée), puis, à la demande ou automatiquement, on amène le
//! clone des sources sur le tag de cette version et on relance `make install`. L'app installée
//! est remplacée à la fin seulement : un échec de compilation laisse la version courante en
//! place. Le redémarrage reste à l'initiative de l'utilisateur (jamais pendant une réunion).
//!
//! Guarantees before building: strict `X.Y.Z` version, clone tied to the official repository, tag
//! signed by a release key known to this binary (`RELEASE_SIGNERS`) and present on
//! `main`.

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

/// SSH keys allowed to sign version tags (`allowed_signers` format of `ssh-keygen`), shared
/// with the release workflow. To change keys: first release, signed with the old one, a
/// version that adds the new line (RELEASING.md).
pub const RELEASE_SIGNERS: &str = include_str!("../release-signers");

/// `SHA256:…` fingerprints of the keys in an `allowed_signers` file.
pub fn signer_fingerprints(signers: &str) -> Vec<String> {
    use base64::Engine;
    use sha2::Digest;
    let b64 = base64::engine::general_purpose::STANDARD;
    signers
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| {
            let mut words = l.split_whitespace();
            words.find(|w| w.starts_with("ssh-") || w.starts_with("ecdsa-") || w.starts_with("sk-"))?;
            let blob = b64.decode(words.next()?).ok()?;
            let digest = sha2::Sha256::digest(&blob);
            Some(format!("SHA256:{}", base64::engine::general_purpose::STANDARD_NO_PAD.encode(digest)))
        })
        .collect()
}

/// Releasable version number: exactly `X.Y.Z` (digits only, no suffix).
pub fn valid_version(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.len() <= 9 && p.bytes().all(|b| b.is_ascii_digit()))
}

/// Official repository address: GitHub over HTTPS or SSH (trailing `.git` optional), with or
/// without a user (`https://me@github.com/…`) and the usual SSH ports (`ssh.github.com:443`, `:22`).
pub fn official_remote(url: &str) -> bool {
    let u = url.trim();
    let path = if let Some(rest) = u.strip_prefix("https://").or_else(|| u.strip_prefix("ssh://")) {
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h).to_ascii_lowercase();
        let ok = if u.starts_with("https://") {
            host == "github.com"
        } else {
            ["github.com", "github.com:22", "ssh.github.com:443"].contains(&host.as_str())
        };
        if !ok {
            return false;
        }
        path
    } else {
        let Some(path) = u.strip_prefix("git@github.com:") else { return false };
        path
    };
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    path.eq_ignore_ascii_case(REPO)
}

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

/// Compares two `x.y.z` versions. `true` if `a` (published version) is newer than `b`
/// (installed version). A published pre-release (`0.3.0-rc1`) is never offered; a final
/// version wins over the installed pre-release of the same number.
pub fn newer(a: &str, b: &str) -> bool {
    fn split(v: &str) -> (Vec<u64>, bool) {
        let v = v.trim();
        let v = v.strip_prefix('v').unwrap_or(v);
        let pre = v.contains(['-', '+']);
        let nums = v.split(['-', '+']).next().unwrap_or("").split('.').map(|p| p.parse().unwrap_or(0)).collect();
        (nums, pre)
    }
    let ((a, a_pre), (b, b_pre)) = (split(a), split(b));
    if a_pre {
        return false;
    }
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    b_pre
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
    let curl = which::which("curl").or_else(|_| which::which_in("curl", Some(build_path()), std::env::temp_dir()));
    let out = Command::new(curl.context("curl introuvable")?)
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

/// Effective `origin` address (`insteadOf` rewrites applied), if any.
fn origin_url(dir: &Path) -> Option<String> {
    git(dir, &["remote", "get-url", "origin"]).ok()
}

/// Checks that `refs/tags/<tag>` is an annotated tag **named `<tag>` in its header** (an old
/// signed tag under a new name is refused), SSH-signed (last block of the object) by one of
/// the keys in `signers` (`allowed_signers` format). GPG is disabled and the signing key must
/// be among the expected fingerprints: neither the user's keyring nor their configuration is
/// enough to get a tag accepted.
pub fn verify_tag(dir: &Path, tag: &str, signers: &str) -> Result<String> {
    let tag_ref = format!("refs/tags/{tag}");
    if git(dir, &["cat-file", "-t", &tag_ref]).ok().as_deref() != Some("tag") {
        bail!("le tag {} n'est pas signé", tag);
    }
    let raw = git(dir, &["cat-file", "tag", &tag_ref])?;
    let header = raw.split("\n\n").next().unwrap_or("");
    if !header.lines().any(|l| l == format!("tag {tag}")) || !header.lines().any(|l| l == "type commit") {
        bail!("le tag {} ne porte pas ce nom dans son en-tête (tag renommé)", tag);
    }
    let last_block = raw.rfind("-----BEGIN ").map(|i| &raw[i..]).unwrap_or("");
    if !last_block.starts_with("-----BEGIN SSH SIGNATURE-----") || !raw.trim_end().ends_with("-----END SSH SIGNATURE-----") {
        bail!("le tag {} n'est pas signé par une clé de publication de Sténo", tag);
    }
    let expected = signer_fingerprints(signers);
    if expected.is_empty() {
        bail!("aucune clé de publication connue");
    }
    let tmp = std::env::temp_dir().join(format!("steno-signers-{}", uuid::Uuid::new_v4().simple()));
    crate::paths::write_private(&tmp, signers.as_bytes())?;
    let signers_cfg = format!("gpg.ssh.allowedSignersFile={}", tmp.display());
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "gpg.format=ssh", "-c", "gpg.ssh.program=ssh-keygen", "-c", &signers_cfg, "-c", "gpg.ssh.revocationFile="])
        .args(["-c", "gpg.program=/usr/bin/false", "-c", "gpg.openpgp.program=/usr/bin/false", "-c", "gpg.x509.program=/usr/bin/false"])
        .args(["verify-tag", &tag_ref])
        .output()
        .context("git introuvable");
    let _ = std::fs::remove_file(&tmp);
    let out = out?;
    let detail = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if !out.status.success() {
        bail!("signature du tag {} invalide ou inconnue : {}", tag, detail);
    }
    let good = detail.lines().find(|l| l.starts_with("Good \"git\" signature")).unwrap_or("");
    if !expected.iter().any(|fp| good.split_whitespace().any(|w| w == fp)) {
        bail!("signature du tag {} invalide ou inconnue : {}", tag, detail);
    }
    Ok(detail)
}

/// Acceptable version commit: `Cargo.toml` at this version, present on the official
/// repository's `main`, and newer than the current sources (neither a downgrade nor a rebuild
/// of the version already there).
fn check_release_commit(dir: &Path, version: &str, oid: &str) -> Result<()> {
    let cargo = git(dir, &["show", &format!("{oid}:Cargo.toml")])?;
    let declared = toml::from_str::<toml::Value>(&cargo)
        .ok()
        .and_then(|v| v.get("workspace")?.get("package")?.get("version")?.as_str().map(String::from));
    if declared.as_deref() != Some(version) {
        bail!("le code du tag v{} n'est pas à la version {}", version, version);
    }
    git(dir, &["merge-base", "--is-ancestor", oid, "refs/remotes/origin/main"])
        .with_context(|| format!("le tag v{version} n'est pas sur la branche main du dépôt officiel"))?;
    if git(dir, &["merge-base", "--is-ancestor", oid, "HEAD"]).is_ok() {
        bail!("les sources contiennent déjà la version {} : lancez `make install` à la main si besoin", version);
    }
    Ok(())
}

/// Raison pour laquelle les sources ne peuvent pas être mises à jour automatiquement.
pub fn install_blocker(dir: Option<&Path>) -> Option<String> {
    let Some(dir) = dir else {
        return Some("dossier des sources introuvable (Sténo a-t-il été installé avec `make install` ?)".into());
    };
    let missing = missing_tools(&build_path());
    if !missing.is_empty() {
        return Some(format!(
            "outils manquants pour compiler : {} — installez-les, ou mettez à jour à la main (README)",
            missing.join(", ")
        ));
    }
    match origin_url(dir) {
        Some(url) if official_remote(&url) => {}
        Some(url) => return Some(format!("les sources ne suivent pas le dépôt officiel ({url}) : mettez à jour à la main")),
        None => return Some("les sources n'ont pas de dépôt distant « origin » : mettez à jour à la main".into()),
    }
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
        self.localized_update(self.update.lock().unwrap().clone())
    }

    /// Message traduit dans la langue des réglages (l'état stocké reste en français).
    fn localized_update(&self, mut st: UpdateStatus) -> UpdateStatus {
        st.message = st.message.map(|m| crate::i18n_messages::localize(self.lang(), &m));
        st
    }

    fn set_update(&self, f: impl FnOnce(&mut UpdateStatus)) {
        let st = {
            let mut u = self.update.lock().unwrap();
            f(&mut u);
            u.clone()
        };
        self.emit(CoreEvent::Update(self.localized_update(st)));
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
    // Test and switch to "checking" under a single lock: an install that starts at the same
    // moment is never overwritten.
    let installing = {
        let mut u = inner.update.lock().unwrap();
        let installing = u.state == "installing";
        if !installing {
            u.state = "checking".into();
        }
        installing
    };
    if installing {
        return Ok(inner.update_status());
    }
    inner.set_update(|_| {});
    let res = tokio::task::spawn_blocking(fetch_latest).await.context("vérification interrompue")?;
    let dir = source_dir();
    let blocker = install_blocker(dir.as_deref());
    match res {
        Ok(rel) => {
            // Only a strict `vX.Y.Z` tag is an offered version.
            let latest = rel
                .as_ref()
                .map(|r| r.tag_name.strip_prefix('v').unwrap_or(&r.tag_name).to_string())
                .filter(|v| valid_version(v));
            let available = latest.as_deref().map(|l| newer(l, CURRENT)).unwrap_or(false);
            let was_installed = inner.update_status().state == "installed";
            inner.set_update(|u| {
                u.latest = latest.clone();
                u.available = available;
                u.url = rel.as_ref().and_then(|r| r.html_url.clone());
                u.notes = rel.as_ref().and_then(|r| r.body.clone());
                u.published_at = rel.as_ref().and_then(|r| r.published_at.clone());
                u.checked_at = Some(crate::now_iso());
                // An install started during the check keeps its state and its message.
                if u.state != "installing" {
                    u.state = if was_installed { "installed".into() } else { "idle".into() };
                    u.source_dir = dir.as_ref().map(|d| d.display().to_string());
                    u.can_install = blocker.is_none();
                    u.message = blocker.clone();
                }
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
                if u.state != "installing" {
                    u.state = "error".into();
                    u.message = Some(format!("Vérification impossible : {msg}"));
                }
                u.checked_at = Some(crate::now_iso());
            });
            return Err(e);
        }
    }
    Ok(inner.update_status())
}

/// Installe la dernière version publiée depuis les sources (quelques minutes).
pub async fn install(inner: Arc<Inner>) -> Result<()> {
    let log = crate::paths::logs_dir().join("update.log");
    // The state switches to "installing" under the same lock as the test: two simultaneous
    // requests cannot start two installs.
    let (version, previous) = {
        let mut u = inner.update.lock().unwrap();
        if u.state == "installing" {
            bail!("une installation est déjà en cours");
        }
        let Some(version) = u.latest.clone().filter(|_| u.available) else {
            bail!("aucune nouvelle version à installer");
        };
        let previous = (u.state.clone(), u.message.clone(), u.log_path.clone());
        u.state = "installing".into();
        u.message = Some(format!("Installation de Sténo {version}… (quelques minutes)"));
        u.log_path = Some(log.display().to_string());
        (version, previous)
    };
    let ready = (|| -> Result<PathBuf> {
        if !valid_version(&version) {
            bail!("numéro de version invalide : {version}");
        }
        if inner.db.live_meeting()?.is_some() {
            bail!("une réunion est en cours : installez la mise à jour après la réunion");
        }
        let dir = source_dir().context("dossier des sources introuvable")?;
        if let Some(why) = install_blocker(Some(&dir)) {
            bail!("{why}");
        }
        Ok(dir)
    })();
    let dir = match ready {
        Ok(d) => d,
        Err(e) => {
            inner.set_update(|u| (u.state, u.message, u.log_path) = previous);
            return Err(e);
        }
    };
    inner.emit(CoreEvent::Update(inner.update_status()));
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
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(log)?;
    writeln!(f, "\n=== {} : installation de {tag} depuis {} ===", crate::now_iso(), dir.display())?;
    match origin_url(dir) {
        Some(url) if official_remote(&url) => {}
        other => bail!("les sources ne suivent pas le dépôt officiel ({}) : mettez à jour à la main", other.unwrap_or_default()),
    }
    git(dir, &["fetch", "--tags", "--force", "origin"])?;
    let version = tag.strip_prefix('v').unwrap_or(tag);
    let oid = git(dir, &["rev-parse", "--verify", &format!("refs/tags/{tag}^{{commit}}")])
        .with_context(|| format!("tag {tag} introuvable"))?;
    let signature = verify_tag(dir, tag, RELEASE_SIGNERS)?;
    writeln!(f, "{signature}")?;
    check_release_commit(dir, version, &oid)?;
    match git(dir, &["symbolic-ref", "-q", "--short", "HEAD"]) {
        Ok(b) if b == "main" => {
            git(dir, &["merge", "--ff-only", &oid]).context("les sources ont divergé de la version publiée")?;
        }
        _ => {
            git(dir, &["checkout", "-q", &oid])?;
        }
    }
    // Build exactly the signed commit, unmodified.
    if git(dir, &["rev-parse", "HEAD"])? != oid || !git(dir, &["status", "--porcelain", "--untracked-files=no"])?.is_empty() {
        bail!("les sources ne correspondent pas exactement au tag {tag} : installation annulée");
    }
    writeln!(f, "sources sur {tag} ({oid}) ; make install…")?;
    let out = Command::new("make")
        .arg("install")
        .current_dir(dir)
        .env("PATH", build_path())
        .stdout(f.try_clone()?)
        .stderr(f.try_clone()?)
        .status()
        .context(if cfg!(target_os = "macos") {
            "make introuvable (outils de développement Xcode installés ?)"
        } else {
            "make introuvable (paquet build-essential installé ?)"
        })?;
    if !out.success() {
        bail!("`make install` a échoué (voir {})", log.display());
    }
    writeln!(f, "=== terminé ===")?;
    Ok(())
}

/// PATH de la compilation : celui du processus, complété des emplacements habituels des outils
/// (rustup, Cargo, Homebrew, Node). Lancée depuis l'app ou un terminal minimal, `make` doit
/// trouver `cargo`, `rustc`, `npm` (et `swift` sur macOS) comme lors de l'installation à la main.
pub fn build_path() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut dirs: Vec<String> = vec![];
    // PATH du shell de connexion (rustup, asdf, mise, nvm… configurés dans le profil), puis
    // celui du processus.
    for p in [login_shell_path(), std::env::var("PATH").unwrap_or_default()] {
        for d in p.split(':').filter(|d| !d.is_empty()) {
            if !dirs.iter().any(|x| x == d) {
                dirs.push(d.to_string());
            }
        }
    }
    for extra in [
        format!("{home}/.cargo/bin"),
        "/opt/homebrew/opt/rustup/bin".into(),
        "/opt/homebrew/bin".into(),
        "/home/linuxbrew/.linuxbrew/bin".into(),
        "/usr/local/bin".into(),
        format!("{home}/.volta/bin"),
        format!("{home}/.local/bin"),
        "/usr/bin".into(),
        "/bin".into(),
    ] {
        if !dirs.contains(&extra) && Path::new(&extra).is_dir() {
            dirs.push(extra);
        }
    }
    dirs.join(":")
}

/// PATH du shell de connexion de l'utilisateur (lu une fois ; vide en cas d'échec).
fn login_shell_path() -> String {
    static P: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    P.get_or_init(|| {
        let default_shell = if cfg!(target_os = "macos") { "/bin/zsh" } else { "/bin/bash" };
        let shell = std::env::var("SHELL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| default_shell.into());
        let marker = "__STENO_PATH__";
        Command::new(&shell)
            .args(["-ilc", &format!("printf '{marker}%s{marker}' \"$PATH\"")])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()
            .and_then(|o| {
                let out = String::from_utf8_lossy(&o.stdout).to_string();
                let start = out.find(marker)? + marker.len();
                let end = start + out[start..].find(marker)?;
                Some(out[start..end].to_string())
            })
            .unwrap_or_default()
    })
    .clone()
}

/// Outils de compilation introuvables dans `path` (noms lisibles).
pub fn missing_tools(path: &str) -> Vec<&'static str> {
    let mut tools: Vec<(&str, &str)> = vec![("cargo", "Rust (cargo)"), ("npm", "Node.js (npm)"), ("make", "make")];
    if cfg!(target_os = "macos") {
        tools.push(("swift", "Swift (outils de développement Xcode)"));
    } else {
        // Bibliothèques système de l'app (WebKitGTK, GTK…) trouvées par pkg-config ; le C
        // (cc) vient de build-essential.
        tools.push(("pkg-config", "pkg-config"));
        tools.push(("cc", "cc (build-essential)"));
    }
    let cwd = std::env::temp_dir();
    tools
        .into_iter()
        .filter(|(bin, _)| which::which_in(bin, Some(path), &cwd).is_err())
        .map(|(_, label)| label)
        .collect()
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
    fn outils_manquants() {
        assert_eq!(missing_tools("/dossier/inexistant").len(), if cfg!(target_os = "macos") { 4 } else { 5 });
        assert!(missing_tools(&build_path()).iter().all(|t| !t.starts_with("make")), "make est présent sur la machine de test");
    }

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
        assert!(!newer("0.3.0-rc1", "0.2.2"), "a published pre-release is never offered");
        assert!(newer("0.2.0", "0.2.0-rc1"), "final version after the installed pre-release");
        assert!(!newer("0.1.9", "0.2.0-rc1"));
    }

    #[test]
    fn strict_version_numbers() {
        for v in ["0.2.2", "1.0.0", "10.20.30"] {
            assert!(valid_version(v), "{v}");
        }
        for v in ["0.2", "0.2.2.1", "0.3.0-rc1", "0.2.2+build", "-0.2.2", "0.2.x", "0..2", "v0.2.2", "0.2.2 ", "0.2.2;x", "１.2.3"] {
            assert!(!valid_version(v), "{v:?}");
        }
    }

    #[test]
    fn official_repository() {
        for u in [
            "https://github.com/NakileSolutions/Steno.git",
            "https://github.com/NakileSolutions/Steno",
            "https://github.com/nakilesolutions/steno/",
            "git@github.com:NakileSolutions/Steno.git",
            "ssh://git@github.com/NakileSolutions/Steno.git",
            "https://moi@github.com/NakileSolutions/Steno.git",
            "ssh://git@ssh.github.com:443/NakileSolutions/Steno.git",
            "ssh://git@github.com:22/NakileSolutions/Steno.git",
        ] {
            assert!(official_remote(u), "{u}");
        }
        for u in [
            "https://github.com/autre/Steno.git",
            "https://github.com/NakileSolutions/Steno-fork.git",
            "https://evil.example/NakileSolutions/Steno.git",
            "https://github.com.evil.example/NakileSolutions/Steno.git",
            "git@github.com:NakileSolutions/Steno/../autre.git",
            "https://github.com@evil.example/NakileSolutions/Steno.git",
            "ssh://git@github.com:2222/NakileSolutions/Steno.git",
            "ssh://git@evil.example/NakileSolutions/Steno.git",
            "/Users/moi/miroir/Steno",
            "",
        ] {
            assert!(!official_remote(u), "{u}");
        }
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
        assert!(install_blocker(Some(d.path())).unwrap().contains("origin"), "no remote: install impossible");
        g(&["remote", "add", "origin", "https://github.com/NakileSolutions/Steno.git"]);
        assert!(install_blocker(Some(d.path())).is_none(), "fichier non suivi : sans effet");
        g(&["add", "Makefile"]);
        assert!(install_blocker(Some(d.path())).unwrap().contains("modifications locales"));
        g(&["-c", "user.email=t@t", "-c", "user.name=t", "-c", "commit.gpgsign=false", "commit", "-q", "-m", "m"]);
        g(&["checkout", "-q", "-b", "essai"]);
        assert!(install_blocker(Some(d.path())).unwrap().contains("essai"));
        assert!(install_blocker(None).is_some());
        g(&["checkout", "-q", "main"]);
        g(&["remote", "set-url", "origin", "https://github.com/quelquun/Steno.git"]);
        assert!(install_blocker(Some(d.path())).unwrap().contains("dépôt officiel"));
        g(&["remote", "set-url", "origin", "https://github.com/NakileSolutions/Steno.git"]);
        assert!(install_blocker(Some(d.path())).is_none());
    }

    /// Test repository with a tag signed by a throwaway SSH key.
    fn signed_repo() -> Option<(tempfile::TempDir, String)> {
        let d = tempfile::tempdir().unwrap();
        let key = d.path().join("cle");
        let ok = Command::new("ssh-keygen").args(["-q", "-t", "ed25519", "-N", "", "-C", "test", "-f"]).arg(&key).status().ok()?.success();
        if !ok {
            return None;
        }
        let pubkey = std::fs::read_to_string(key.with_extension("pub")).unwrap();
        let repo = d.path().join("depot");
        std::fs::create_dir_all(&repo).unwrap();
        let g = |args: &[&str]| {
            let o = Command::new("git").arg("-C").arg(&repo).args(args).output().unwrap();
            assert!(o.status.success(), "{args:?} : {}", String::from_utf8_lossy(&o.stderr));
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        };
        g(&["init", "-q", "-b", "main"]);
        let id = ["-c", "user.email=publication@test", "-c", "user.name=t", "-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"];
        std::fs::write(repo.join("Cargo.toml"), "[workspace.package]\nversion = \"9.9.9\"\n").unwrap();
        g(&["add", "Cargo.toml"]);
        g(&[&id[..], &["commit", "-q", "-m", "x"]].concat());
        let k = format!("user.signingkey={}", key.display());
        let sign = |name: &str| g(&[&id[..], &["-c", "gpg.format=ssh", "-c", &k, "tag", "-s", name, "-m", "signé"]].concat());
        sign("v9.9.9");
        sign("v0.1.0");
        g(&[&id[..], &["tag", "-a", "v9.9.8", "-m", "annoté sans signature"]].concat());
        g(&[&id[..], &["tag", "v9.9.7"]].concat());
        // Old signed tag (v0.1.0) published again under another name.
        let old = g(&["rev-parse", "refs/tags/v0.1.0"]);
        g(&["update-ref", "refs/tags/v9.9.6", &old]);
        // Fake SSH block in the message, final signature of another type.
        let commit = g(&["rev-parse", "HEAD"]);
        let forged = format!(
            "object {commit}\ntype commit\ntag v9.9.5\ntagger t <publication@test> 1795000000 +0100\n\n\
             -----BEGIN SSH SIGNATURE-----\nZmF1eA==\n-----END SSH SIGNATURE-----\n\
             -----BEGIN PGP SIGNATURE-----\nZmF1eA==\n-----END PGP SIGNATURE-----\n"
        );
        let mut mk = Command::new("git").arg("-C").arg(&repo).args(["hash-object", "-t", "tag", "-w", "--stdin", "--literally"])
            .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().unwrap();
        use std::io::Write;
        mk.stdin.take().unwrap().write_all(forged.as_bytes()).unwrap();
        let sha = String::from_utf8_lossy(&mk.wait_with_output().unwrap().stdout).trim().to_string();
        g(&["update-ref", "refs/tags/v9.9.5", &sha]);
        let signers = format!("publication@test namespaces=\"git\" {}", pubkey.trim());
        Some((d, signers))
    }

    #[test]
    fn tag_signed_by_a_release_key() {
        let Some((d, signers)) = signed_repo() else { return };
        let repo = d.path().join("depot");
        assert!(verify_tag(&repo, "v9.9.9", &signers).is_ok());
        assert!(verify_tag(&repo, "v9.9.8", &signers).unwrap_err().to_string().contains("signé"));
        assert!(verify_tag(&repo, "v9.9.7", &signers).unwrap_err().to_string().contains("n'est pas signé"));
        assert!(verify_tag(&repo, "v9.9.9", RELEASE_SIGNERS).is_err(), "key unknown to Sténo: refused");
        assert!(verify_tag(&repo, "v0.0.0", &signers).is_err());
        let e = verify_tag(&repo, "v9.9.6", &signers).unwrap_err().to_string();
        assert!(e.contains("renommé"), "old signed tag published again under another name: {e}");
        assert!(verify_tag(&repo, "v9.9.5", &signers).is_err(), "faux bloc SSH, signature finale PGP");
    }

    #[test]
    fn real_version_tag_verified() {
        // Published tags present in the clone (not in CI, where the repository is copied without tags).
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        if git(&root, &["cat-file", "-t", "refs/tags/v0.2.2"]).ok().as_deref() == Some("tag") {
            assert!(verify_tag(&root, "v0.2.2", RELEASE_SIGNERS).is_ok());
        }
    }

    #[test]
    fn release_key_fingerprint() {
        assert_eq!(signer_fingerprints(RELEASE_SIGNERS), ["SHA256:cVy45Aj++geRYY+SECDc0QWkd3WQQ6H+nBMPjKV9+PY"]);
        assert!(signer_fingerprints("# comment only\n").is_empty());
    }

    #[test]
    fn version_commit_checked() {
        let Some((d, _)) = signed_repo() else { return };
        let repo = d.path().join("depot");
        let g = |args: &[&str]| {
            let o = Command::new("git").arg("-C").arg(&repo).args(args).output().unwrap();
            assert!(o.status.success(), "{args:?}");
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        };
        let oid = g(&["rev-parse", "HEAD"]);
        // No remote main: refused.
        assert!(check_release_commit(&repo, "9.9.9", &oid).unwrap_err().to_string().contains("main"));
        g(&["update-ref", "refs/remotes/origin/main", &oid]);
        // Already on this commit: neither a downgrade nor an endless rebuild.
        assert!(check_release_commit(&repo, "9.9.9", &oid).unwrap_err().to_string().contains("déjà"));
        g(&["checkout", "-q", "--orphan", "autre"]);
        g(&["-c", "user.email=t@t", "-c", "user.name=t", "-c", "commit.gpgsign=false", "commit", "-q", "--allow-empty", "-m", "y"]);
        assert!(check_release_commit(&repo, "9.9.9", &oid).is_ok());
        assert!(check_release_commit(&repo, "9.9.8", &oid).unwrap_err().to_string().contains("version"), "Cargo.toml at another version");
        // A `version = …` line outside [workspace.package] does not count.
        std::fs::write(repo.join("Cargo.toml"), "[workspace.package]\nversion = \"1.0.0\"\n[dependencies.x]\nversion = \"9.9.7\"\n").unwrap();
        g(&["add", "Cargo.toml"]);
        g(&["-c", "user.email=t@t", "-c", "user.name=t", "-c", "commit.gpgsign=false", "commit", "-q", "-m", "z"]);
        let other = g(&["rev-parse", "HEAD"]);
        g(&["update-ref", "refs/remotes/origin/main", &other]);
        g(&["checkout", "-q", "--orphan", "encore"]);
        g(&["-c", "user.email=t@t", "-c", "user.name=t", "-c", "commit.gpgsign=false", "commit", "-q", "--allow-empty", "-m", "w"]);
        assert!(check_release_commit(&repo, "9.9.7", &other).is_err());
        assert!(check_release_commit(&repo, "1.0.0", &other).is_ok());
    }

    #[tokio::test]
    async fn refused_install_does_not_stay_stuck() {
        let core = crate::Core::new(crate::Db::open_in_memory().unwrap(), Default::default()).unwrap();
        let inner = core.inner.clone();
        {
            let mut u = inner.update.lock().unwrap();
            u.latest = Some("9.9.9".into());
            u.available = true;
            u.state = "idle".into();
            u.message = None;
        }
        let m = crate::model::Meeting {
            id: "m".into(),
            title: None,
            title_source: None,
            started_at: crate::now_iso(),
            ended_at: None,
            status: "live".into(),
            confidential: false,
            agent_provider: None,
            agent_session_id: None,
            correlation: None,
            call_app: None,
        };
        inner.db.insert_meeting(&m).unwrap();
        let e = install(inner.clone()).await.unwrap_err().to_string();
        assert!(e.contains("réunion est en cours"), "{e}");
        let st = inner.update_status();
        assert_eq!(st.state, "idle", "state restored after the refusal");
        assert_eq!(st.message, None);
        inner.update.lock().unwrap().state = "installing".into();
        assert!(install(inner.clone()).await.unwrap_err().to_string().contains("déjà en cours"));
        {
            let mut u = inner.update.lock().unwrap();
            u.state = "idle".into();
            u.latest = Some("9.9.9-rc1".into());
        }
        assert!(install(inner.clone()).await.unwrap_err().to_string().contains("version invalide"));
        assert_eq!(inner.update_status().state, "idle");
        core.shutdown();
    }
}
