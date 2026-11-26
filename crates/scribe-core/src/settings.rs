//! Réglages typés, stockés en base (une ligne JSON par section). Voir contracts §4.

use crate::db::Db;
use crate::model::{Intent, Tier};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub agent: AgentSettings,
    pub voice: VoiceSettings,
    pub audio: AudioSettings,
    pub correlation: CorrelationSettings,
    pub privacy: PrivacySettings,
    pub profile: ProfileSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            agent: AgentSettings::default(),
            voice: VoiceSettings::default(),
            audio: AudioSettings::default(),
            correlation: CorrelationSettings::default(),
            privacy: PrivacySettings::default(),
            profile: ProfileSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct TierModels {
    pub quick: String,
    pub search: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TierTimeouts {
    pub quick: u64,
    pub search: u64,
    pub summary: u64,
}

impl Default for TierTimeouts {
    fn default() -> Self {
        TierTimeouts { quick: 90, search: 240, summary: 300 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AgentSettings {
    pub provider: String,
    pub models: TierModels,
    pub custom_command: String,
    pub allow_api_key: bool,
    pub max_parallel: u32,
    pub timeouts_s: TierTimeouts,
    pub live_tool_budget: u32,
    pub live_context_seconds: u32,
}

impl Default for AgentSettings {
    fn default() -> Self {
        AgentSettings {
            provider: "claude-code".into(),
            models: TierModels::default(),
            custom_command: String::new(),
            allow_api_key: false,
            max_parallel: 2,
            timeouts_s: TierTimeouts::default(),
            live_tool_budget: 15,
            live_context_seconds: 120,
        }
    }
}

impl AgentSettings {
    pub fn model_for(&self, tier: Tier) -> Option<String> {
        let m = match tier {
            Tier::Quick => &self.models.quick,
            Tier::Search => &self.models.search,
            Tier::Summary => &self.models.summary,
        };
        let m = m.trim();
        (!m.is_empty()).then(|| m.to_string())
    }
    pub fn timeout_for(&self, tier: Tier) -> std::time::Duration {
        let s = match tier {
            Tier::Quick => self.timeouts_s.quick,
            Tier::Search => self.timeouts_s.search,
            Tier::Summary => self.timeouts_s.summary,
        };
        std::time::Duration::from_secs(s.max(10))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct VoiceSettings {
    pub wake_word: String,
    pub strict: bool,
    pub prefixes: Vec<String>,
    pub variants: Vec<String>,
    pub verbs: BTreeMap<String, Vec<String>>,
    pub feedback: String,
}

pub fn default_verbs() -> BTreeMap<String, Vec<String>> {
    let mut m = BTreeMap::new();
    let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    m.insert("search".into(), v(&["cherche", "recherche", "regarde", "trouve"]));
    m.insert("check".into(), v(&["vérifie", "confirme"]));
    m.insert("note".into(), v(&["note", "ajoute une action", "retiens"]));
    m.insert("recap".into(), v(&["résume", "récap", "récapitule"]));
    m.insert("cancel".into(), v(&["annule"]));
    m
}

impl Default for VoiceSettings {
    fn default() -> Self {
        VoiceSettings {
            wake_word: "Scribe".into(),
            strict: false,
            prefixes: vec!["ok".into(), "dis".into(), "hey".into()],
            variants: vec![],
            verbs: default_verbs(),
            feedback: "system".into(),
        }
    }
}

impl VoiceSettings {
    /// Verbes par intention (les intentions inconnues sont ignorées).
    pub fn verbs_by_intent(&self) -> Vec<(Intent, Vec<String>)> {
        let mut out = vec![];
        for (k, v) in &self.verbs {
            if let Some(i) = Intent::parse(k) {
                if i != Intent::History {
                    out.push((i, v.clone()));
                }
            }
        }
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Shortcuts {
    pub toggle: String,
    pub aside: String,
    pub quick_note: String,
}

impl Default for Shortcuts {
    fn default() -> Self {
        Shortcuts {
            toggle: "Ctrl+Alt+S".into(),
            aside: "Ctrl+Alt+Space".into(),
            quick_note: "Ctrl+Alt+N".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AudioSettings {
    pub auto_detect_teams: bool,
    pub silence_ms: u32,
    pub vocabulary: Vec<String>,
    pub end_grace_s: u32,
    pub shortcuts: Shortcuts,
}

impl Default for AudioSettings {
    fn default() -> Self {
        AudioSettings {
            auto_detect_teams: true,
            silence_ms: 1000,
            vocabulary: vec![],
            end_grace_s: 60,
            shortcuts: Shortcuts::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CorrelationSettings {
    pub mode: String,
    pub budget: u32,
    pub exclude_words: Vec<String>,
}

impl Default for CorrelationSettings {
    fn default() -> Self {
        CorrelationSettings {
            mode: "on_demand".into(),
            budget: 30,
            exclude_words: vec!["1:1".into(), "RH".into(), "entretien".into(), "perso".into()],
        }
    }
}

impl CorrelationSettings {
    /// Vrai si le titre contient un mot d'exclusion (mot entier, sans casse ni accents).
    pub fn is_excluded(&self, title: Option<&str>) -> bool {
        let Some(title) = title else { return false };
        let t = format!(" {} ", crate::text::normalize_keep(title, ":"));
        self.exclude_words.iter().any(|w| {
            let w = crate::text::normalize_keep(w, ":");
            !w.is_empty() && t.contains(&format!(" {} ", w))
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct PrivacySettings {
    pub audio_retention_days: u32,
    pub transcript_retention_days: Option<u32>,
    pub auto_export_dir: Option<String>,
    pub excluded_titles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ProfileSettings {
    pub name: String,
    pub role: String,
    pub team: String,
    pub jira_key_pattern: String,
}

impl Default for ProfileSettings {
    fn default() -> Self {
        ProfileSettings {
            name: String::new(),
            role: String::new(),
            team: String::new(),
            jira_key_pattern: r"[A-Z][A-Z0-9]+-\d+".into(),
        }
    }
}

const SECTIONS: [&str; 6] = ["agent", "voice", "audio", "correlation", "privacy", "profile"];

impl Settings {
    /// Charge les réglages depuis la base (valeurs par défaut pour ce qui manque).
    pub fn load(db: &Db) -> Result<Settings> {
        let mut root = serde_json::Map::new();
        for s in SECTIONS {
            if let Some(v) = db.get_setting(s)? {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&v) {
                    root.insert(s.into(), json);
                }
            }
        }
        Ok(serde_json::from_value(serde_json::Value::Object(root)).unwrap_or_default())
    }

    pub fn save(&self, db: &Db) -> Result<()> {
        let v = serde_json::to_value(self)?;
        for s in SECTIONS {
            if let Some(sec) = v.get(s) {
                db.set_setting(s, &sec.to_string())?;
            }
        }
        Ok(())
    }

    /// Valide et normalise avant enregistrement.
    pub fn validated(mut self) -> Result<Settings> {
        self.voice.wake_word = self.voice.wake_word.trim().to_string();
        if self.voice.wake_word.is_empty() {
            anyhow::bail!("le mot d'appel ne peut pas être vide");
        }
        if !["none", "system", "headset"].contains(&self.voice.feedback.as_str()) {
            self.voice.feedback = "system".into();
        }
        if !["off", "on_demand", "auto"].contains(&self.correlation.mode.as_str()) {
            anyhow::bail!("mode de corrélation inconnu : {}", self.correlation.mode);
        }
        regex::Regex::new(&self.profile.jira_key_pattern)
            .context("motif des clés Jira invalide")?;
        self.agent.max_parallel = self.agent.max_parallel.clamp(1, 8);
        self.audio.silence_ms = self.audio.silence_ms.clamp(500, 3000);
        if self.audio.end_grace_s == 0 {
            self.audio.end_grace_s = 60;
        }
        self.voice.variants.retain(|v| !v.trim().is_empty());
        self.voice.variants.dedup();
        Ok(self)
    }

    /// Export TOML pour l'équipe : sans chemin ni donnée personnelle.
    pub fn to_team_toml(&self) -> Result<String> {
        let mut s = self.clone();
        s.profile.name = String::new();
        s.privacy.auto_export_dir = None;
        s.agent.custom_command = String::new();
        let body = toml::to_string_pretty(&s)?;
        Ok(format!(
            "# Réglages de départ Scribe (sans chemins ni données personnelles)\n# Exporté le {}\n\n{}",
            chrono::Local::now().format("%Y-%m-%d"),
            body
        ))
    }

    /// Fusionne un TOML d'équipe dans les réglages actuels. Les champs personnels
    /// (nom, dossier d'export, commande personnalisée) sont conservés.
    pub fn merge_team_toml(&self, input: &str) -> Result<Settings> {
        let incoming: toml::Value = toml::from_str(input).context("TOML invalide")?;
        let mut cur = serde_json::to_value(self)?;
        let inc = serde_json::to_value(incoming)?;
        merge_json(&mut cur, &inc);
        let mut out: Settings = serde_json::from_value(cur).context("réglages importés invalides")?;
        out.profile.name = self.profile.name.clone();
        out.privacy.auto_export_dir = self.privacy.auto_export_dir.clone();
        out.agent.custom_command = self.agent.custom_command.clone();
        out.validated()
    }
}

fn merge_json(dst: &mut serde_json::Value, src: &serde_json::Value) {
    match (dst, src) {
        (serde_json::Value::Object(d), serde_json::Value::Object(s)) => {
            for (k, v) in s {
                match d.get_mut(k) {
                    Some(dv) => merge_json(dv, v),
                    None => {
                        d.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (d, s) => *d = s.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aller_retour_base() {
        let db = Db::open_in_memory().unwrap();
        let mut s = Settings::load(&db).unwrap();
        assert_eq!(s, Settings::default());
        s.voice.wake_word = "Greffier".into();
        s.save(&db).unwrap();
        assert_eq!(Settings::load(&db).unwrap().voice.wake_word, "Greffier");
    }

    #[test]
    fn section_partielle() {
        let db = Db::open_in_memory().unwrap();
        db.set_setting("voice", r#"{"wake_word":"Jarvis"}"#).unwrap();
        let s = Settings::load(&db).unwrap();
        assert_eq!(s.voice.wake_word, "Jarvis");
        assert_eq!(s.voice.verbs, default_verbs());
    }

    #[test]
    fn toml_equipe_sans_donnees_perso() {
        let mut s = Settings::default();
        s.profile.name = "Alex".into();
        s.privacy.auto_export_dir = Some("/Users/alex/notes".into());
        s.audio.vocabulary = vec!["ms-booking".into()];
        let t = s.to_team_toml().unwrap();
        assert!(!t.contains("Alex"));
        assert!(!t.contains("/Users"));
        let mut other = Settings::default();
        other.profile.name = "Sam".into();
        let merged = other.merge_team_toml(&t).unwrap();
        assert_eq!(merged.audio.vocabulary, vec!["ms-booking".to_string()]);
        assert_eq!(merged.profile.name, "Sam");
    }

    #[test]
    fn exclusions_correlation() {
        let c = CorrelationSettings::default();
        assert!(c.is_excluded(Some("1:1 Alex / Sam")));
        assert!(c.is_excluded(Some("Point RH mensuel")));
        assert!(c.is_excluded(Some("Entretien annuel")));
        assert!(!c.is_excluded(Some("Daily ORION")));
        assert!(!c.is_excluded(Some("Refonte CRH")), "mot entier uniquement");
    }
}
