//! Informations sur les processus (/proc) : nom de l'exécutable, identifiant Flatpak ou Snap,
//! parent. Sert à rattacher un flux audio à une app de visio, même quand le flux vient d'un
//! processus auxiliaire (moteur de rendu Electron, processus « utility » de Chrome…).

use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProcInfo {
    pub pid: u32,
    pub ppid: Option<u32>,
    /// Noms à comparer aux préfixes : `comm`, nom de l'exécutable, identifiant Flatpak, nom du snap.
    pub names: Vec<String>,
}

fn read(path: impl AsRef<Path>) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Parent d'après `/proc/<pid>/stat` (le nom du processus, entre parenthèses, peut contenir des espaces).
pub fn parse_ppid(stat: &str) -> Option<u32> {
    let after = &stat[stat.rfind(')')? + 1..];
    after.split_whitespace().nth(1)?.parse().ok()
}

/// `name=com.slack.Slack` dans la section `[Application]` de `.flatpak-info`.
pub fn parse_flatpak_info(s: &str) -> Option<String> {
    let mut in_app = false;
    for line in s.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_app = l == "[Application]";
        } else if in_app {
            if let Some(v) = l.strip_prefix("name=") {
                return Some(v.trim().to_string()).filter(|v| !v.is_empty());
            }
        }
    }
    None
}

/// `snap.slack.slack-xxxx.scope` (ou `snap.slack.slack.<uuid>.scope`) dans `/proc/<pid>/cgroup` → `slack`.
pub fn parse_snap(cgroup: &str) -> Option<String> {
    for part in cgroup.split(['/', '\n']) {
        if let Some(rest) = part.strip_prefix("snap.") {
            let name = rest.split('.').next()?;
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

pub fn info(pid: u32) -> Option<ProcInfo> {
    let base = format!("/proc/{pid}");
    let stat = read(format!("{base}/stat"))?;
    let mut names = vec![];
    if let Some(c) = read(format!("{base}/comm")) {
        names.push(c.trim().to_string());
    }
    if let Ok(exe) = std::fs::read_link(format!("{base}/exe")) {
        if let Some(n) = exe.file_name() {
            names.push(n.to_string_lossy().trim_end_matches(" (deleted)").to_string());
        }
    }
    // Ligne de commande : premier argument (les lanceurs comme `/opt/slack/slack` y figurent).
    if let Some(cmd) = read(format!("{base}/cmdline")) {
        if let Some(first) = cmd.split('\0').next() {
            if let Some(n) = Path::new(first).file_name() {
                names.push(n.to_string_lossy().to_string());
            }
        }
    }
    if let Some(fp) = read(format!("{base}/root/.flatpak-info")).and_then(|s| parse_flatpak_info(&s)) {
        names.push(fp);
    }
    if let Some(snap) = read(format!("{base}/cgroup")).and_then(|s| parse_snap(&s)) {
        names.push(snap);
    }
    names.retain(|n| !n.is_empty());
    names.dedup();
    Some(ProcInfo { pid, ppid: parse_ppid(&stat), names })
}

/// Tous les processus visibles de l'utilisateur.
pub fn all() -> Vec<ProcInfo> {
    let Ok(rd) = std::fs::read_dir("/proc") else { return vec![] };
    rd.filter_map(|e| e.ok()?.file_name().to_str()?.parse::<u32>().ok()).filter_map(info).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyse() {
        assert_eq!(parse_ppid("1234 (Web Content (x)) S 99 1234 1234 0 -1"), Some(99));
        assert_eq!(parse_flatpak_info("[Application]\nname=com.slack.Slack\nruntime=x\n[Instance]\nname=autre"), Some("com.slack.Slack".into()));
        assert_eq!(parse_snap("0::/user.slice/user-1000.slice/user@1000.service/app.slice/snap.slack.slack-2c3e.scope\n"), Some("slack".into()));
        assert_eq!(parse_snap("0::/user.slice"), None);
        let me = info(std::process::id()).unwrap();
        assert!(!me.names.is_empty());
        assert!(me.ppid.is_some());
    }
}
