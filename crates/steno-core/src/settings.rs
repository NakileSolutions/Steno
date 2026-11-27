//! Réglages typés, stockés en base (une ligne JSON par section). Voir contracts §4.

use crate::db::Db;
use crate::model::{Intent, Tier};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
#[derive(Default)]
pub struct Settings {
    pub agent: AgentSettings,
    pub voice: VoiceSettings,
    pub audio: AudioSettings,
    pub correlation: CorrelationSettings,
    pub privacy: PrivacySettings,
    pub profile: ProfileSettings,
    pub report: ReportSettings,
    pub updates: UpdateSettings,
}

/// Mises à jour (contact de l'API publique de GitHub, aucune donnée envoyée).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UpdateSettings {
    /// Vérifier les nouvelles versions (au démarrage puis toutes les 6 h).
    pub check: bool,
    /// Installer automatiquement, hors réunion (le redémarrage reste manuel).
    pub auto_install: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        UpdateSettings { check: true, auto_install: false }
    }
}

/// Modèle de compte rendu ajouté par l'utilisateur.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct CustomTemplate {
    pub id: String,
    pub name: String,
    pub instructions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ReportSettings {
    /// Modèle utilisé quand la réunion n'en précise pas (`standard`, `daily`… ou un modèle perso).
    pub default_template: String,
    /// Proposer d'ajouter du contexte quand on arrête l'écoute depuis la fenêtre (désactivé :
    /// l'arrêt reste en un clic, le contexte s'ajoute pendant la réunion ou sur sa page).
    pub ask_context_on_stop: bool,
    /// Délai (s) entre la fin de la réunion et la rédaction, pour ajouter contexte et documents.
    /// 0 = rédaction immédiate.
    pub context_delay_s: u32,
    pub templates: Vec<CustomTemplate>,
}

impl Default for ReportSettings {
    fn default() -> Self {
        ReportSettings { default_template: "standard".into(), ask_context_on_stop: false, context_delay_s: 60, templates: vec![] }
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
    /// Agent par défaut.
    pub provider: String,
    /// Choix par usage (agent, modèle, niveau de réflexion).
    pub tiers: TierChoices,
    /// Ancien format : modèle par niveau pour l'agent par défaut (lu si `tiers.*.model` est vide).
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
            tiers: TierChoices::default(),
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

/// Agent, modèle et niveau de réflexion d'un usage (vide = valeur par défaut).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct TierChoice {
    pub agent: String,
    pub model: String,
    pub effort: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct TierChoices {
    /// Recherches en direct, questions sur l'historique, corrélation.
    pub search: TierChoice,
    /// Compte rendu de fin de réunion.
    pub summary: TierChoice,
    /// Notes, récapitulatifs, tests.
    pub quick: TierChoice,
}

fn non_empty(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

impl AgentSettings {
    pub fn choice(&self, tier: Tier) -> &TierChoice {
        match tier {
            Tier::Quick => &self.tiers.quick,
            Tier::Search => &self.tiers.search,
            Tier::Summary => &self.tiers.summary,
        }
    }

    /// Agent utilisé pour cet usage.
    pub fn agent_for(&self, tier: Tier) -> String {
        non_empty(&self.choice(tier).agent).unwrap_or_else(|| self.provider.clone())
    }

    /// Modèle de cet usage (`None` = défaut du CLI). L'ancien réglage `models` ne vaut que
    /// pour l'agent par défaut.
    pub fn model_for(&self, tier: Tier) -> Option<String> {
        if let Some(m) = non_empty(&self.choice(tier).model) {
            return Some(m);
        }
        if self.agent_for(tier) != self.provider {
            return None;
        }
        non_empty(match tier {
            Tier::Quick => &self.models.quick,
            Tier::Search => &self.models.search,
            Tier::Summary => &self.models.summary,
        })
    }

    /// Niveau de réflexion de cet usage (`None` = défaut du CLI).
    pub fn effort_for(&self, tier: Tier) -> Option<String> {
        non_empty(&self.choice(tier).effort)
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
    /// UID de la sortie où jouer le signal de détection (`None` = sortie par défaut).
    pub feedback_device: Option<String>,
}

pub fn default_verbs() -> BTreeMap<String, Vec<String>> {
    let mut m = BTreeMap::new();
    let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    m.insert("search".into(), v(&["cherche", "recherche", "regarde", "trouve"]));
    m.insert("check".into(), v(&["vérifie", "confirme"]));
    m.insert("note".into(), v(&["note", "ajoute une action", "retiens"]));
    m.insert("recap".into(), v(&["résume", "récap", "récapitule"]));
    m.insert("cancel".into(), v(&["annule"]));
    m.insert(
        "stop".into(),
        v(&["arrête l'écoute", "arrête d'écouter", "arrête la transcription", "termine la réunion", "fin de la réunion"]),
    );
    m
}

impl Default for VoiceSettings {
    fn default() -> Self {
        VoiceSettings {
            wake_word: "Sténo".into(),
            strict: false,
            prefixes: vec!["ok".into(), "dis".into(), "hey".into()],
            variants: vec![],
            verbs: default_verbs(),
            feedback: "system".into(),
            feedback_device: None,
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

/// App de visio surveillée (appel détecté quand elle utilise le micro).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CallApp {
    pub id: String,
    pub name: String,
    /// Préfixes d'identifiant de bundle (auxiliaires compris).
    pub bundles: Vec<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Navigateur : on capte tout son audio (autres onglets compris).
    #[serde(default)]
    pub browser: bool,
}

fn yes() -> bool {
    true
}

/// Catalogue par défaut des apps de visio.
pub fn default_call_apps() -> Vec<CallApp> {
    let app = |id: &str, name: &str, bundles: &[&str], enabled: bool, browser: bool| CallApp {
        id: id.into(),
        name: name.into(),
        bundles: bundles.iter().map(|b| b.to_string()).collect(),
        enabled,
        browser,
    };
    vec![
        app("teams", "Microsoft Teams", &["com.microsoft.teams2", "com.microsoft.teams"], true, false),
        app("slack", "Slack", &["com.tinyspeck.slackmacgap"], true, false),
        app("zoom", "Zoom", &["us.zoom.xos", "us.zoom.CptHost"], true, false),
        app("webex", "Webex", &["Cisco-Systems.Spark", "com.cisco.webexmeetingsapp", "com.webex.meetingmanager"], true, false),
        app("discord", "Discord", &["com.hnc.Discord"], true, false),
        app("facetime", "FaceTime", &["com.apple.FaceTime"], true, false),
        app(
            "browser",
            "Navigateur (Google Meet…)",
            &[
                "com.google.Chrome",
                "com.apple.Safari",
                "company.thebrowser.Browser",
                "com.microsoft.edgemac",
                "org.mozilla.firefox",
                "com.brave.Browser",
            ],
            false,
            true,
        ),
    ]
}

/// Nom court pour les libellés (« Teams », « Slack »…).
pub fn short_app_name(app: &CallApp) -> String {
    match app.id.as_str() {
        "teams" => "Teams".into(),
        _ => app.name.split(" (").next().unwrap_or(&app.name).to_string(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AudioSettings {
    /// Proposer de démarrer l'écoute quand un appel commence dans une app surveillée
    /// (notification seulement : l'écoute ne démarre jamais toute seule).
    #[serde(alias = "auto_detect_teams")]
    pub auto_detect_calls: bool,
    /// UID du micro choisi (`None` = micro par défaut du système).
    pub input_device: Option<String>,
    /// Nom du micro au moment du choix (affiché s'il est débranché).
    pub input_device_name: Option<String>,
    /// Apps de visio surveillées.
    pub call_apps: Vec<CallApp>,
    pub silence_ms: u32,
    pub vocabulary: Vec<String>,
    /// Obsolète (l'écoute ne s'arrête plus à la fin de l'appel Teams) ; conservé pour la
    /// compatibilité des réglages enregistrés.
    pub end_grace_s: u32,
    pub shortcuts: Shortcuts,
    /// Retranscription de fin de réunion avec un modèle plus précis, langue imposée
    /// (code ISO, ex. `fr`) ; `off` pour garder la transcription du direct.
    pub final_transcription: String,
}

impl AudioSettings {
    pub fn call_app(&self, id: &str) -> Option<&CallApp> {
        self.call_apps.iter().find(|a| a.id == id)
    }
}

impl Default for AudioSettings {
    fn default() -> Self {
        AudioSettings {
            auto_detect_calls: true,
            input_device: None,
            input_device_name: None,
            call_apps: default_call_apps(),
            silence_ms: 1000,
            vocabulary: vec![],
            end_grace_s: 60,
            shortcuts: Shortcuts::default(),
            final_transcription: "fr".into(),
        }
    }
}

/// Langues de la retranscription de fin de réunion (modèle Canary 1B v2).
pub const FINAL_LANGUAGES: &[&str] = &[
    "bg", "cs", "da", "de", "el", "en", "es", "et", "fi", "fr", "hr", "hu", "it", "lt", "lv", "mt", "nl", "pl",
    "pt", "ro", "ru", "sk", "sl", "sv", "uk",
];

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PrivacySettings {
    pub audio_retention_days: u32,
    pub transcript_retention_days: Option<u32>,
    pub auto_export_dir: Option<String>,
    pub excluded_titles: Vec<String>,
    /// Au début de l'écoute, rappeler de prévenir les participants (RGPD).
    pub consent_reminder: bool,
    /// Message d'information à coller dans le chat de l'appel.
    pub consent_message: String,
}

/// Message d'information par défaut : honnête sur ce qui sort du Mac (le compte rendu passe
/// par un assistant IA).
pub const DEFAULT_CONSENT_MESSAGE: &str = "Pour information : j'utilise Sténo pour transcrire cette réunion et en préparer un compte rendu. La transcription reste sur mon ordinateur ; un assistant IA s'en sert pour rédiger le compte rendu. Dites-le-moi si vous préférez que je ne transcrive pas.";

impl Default for PrivacySettings {
    fn default() -> Self {
        PrivacySettings {
            audio_retention_days: 0,
            transcript_retention_days: None,
            auto_export_dir: None,
            excluded_titles: vec![],
            consent_reminder: true,
            consent_message: DEFAULT_CONSENT_MESSAGE.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ProfileSettings {
    pub name: String,
    pub role: String,
    pub team: String,
    pub jira_key_pattern: String,
    /// Apparence de l'app : `auto` (suit macOS), `light` ou `dark`.
    pub theme: String,
    /// Langue de l'interface et des comptes rendus : `auto` (langue de macOS), `fr` ou `en`.
    pub language: String,
}

impl Default for ProfileSettings {
    fn default() -> Self {
        ProfileSettings {
            name: String::new(),
            role: String::new(),
            team: String::new(),
            jira_key_pattern: r"[A-Z][A-Z0-9]+-\d+".into(),
            theme: "auto".into(),
            language: "auto".into(),
        }
    }
}

const SECTIONS: [&str; 8] = ["agent", "voice", "audio", "correlation", "privacy", "profile", "report", "updates"];

/// Langue de macOS (préférences de l'utilisateur), `fr` ou `en`.
pub fn system_lang() -> &'static str {
    static L: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
    L.get_or_init(|| {
        let from_env = std::env::var("LANG").unwrap_or_default();
        let mac = std::process::Command::new("/usr/bin/defaults")
            .args(["read", "-g", "AppleLanguages"])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        let first = mac.split(['"', ',', '(', ')', '\n', ' ']).find(|w| w.len() >= 2).unwrap_or(&from_env).to_lowercase();
        let pick = if first.is_empty() { from_env.to_lowercase() } else { first };
        if pick.starts_with("fr") || pick.is_empty() { "fr" } else { "en" }
    })
}

impl Settings {
    /// Langue effective (`fr` ou `en`) de l'interface et des comptes rendus.
    pub fn lang(&self) -> &'static str {
        match self.profile.language.as_str() {
            "fr" => "fr",
            "en" => "en",
            _ => system_lang(),
        }
    }

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
        if self.privacy.consent_message.trim().is_empty() {
            self.privacy.consent_message = DEFAULT_CONSENT_MESSAGE.into();
        }
        // Modèles perso : nom obligatoire, identifiant unique dérivé du nom, distinct des modèles intégrés.
        let mut ids: std::collections::HashSet<String> =
            crate::report::BUILTIN_TEMPLATES.iter().map(|t| t.id.to_string()).collect();
        let mut templates = vec![];
        for mut t in std::mem::take(&mut self.report.templates) {
            t.name = t.name.trim().to_string();
            t.instructions = t.instructions.trim().to_string();
            if t.name.is_empty() {
                continue;
            }
            let mut id = t.id.trim().to_string();
            if id.is_empty() || crate::report::BUILTIN_TEMPLATES.iter().any(|b| b.id == id) {
                id = format!("perso-{}", crate::text::normalize(&t.name).replace(' ', "-"));
            }
            let base = id.clone();
            let mut n = 2;
            while !ids.insert(id.clone()) {
                id = format!("{base}-{n}");
                n += 1;
            }
            t.id = id;
            templates.push(t);
        }
        self.report.templates = templates;
        if crate::report::template(&self, &self.report.default_template).is_none() {
            self.report.default_template = "standard".into();
        }
        self.report.context_delay_s = self.report.context_delay_s.min(600);
        if !["auto", "fr", "en"].contains(&self.profile.language.as_str()) {
            self.profile.language = "auto".into();
        }
        if !["auto", "light", "dark"].contains(&self.profile.theme.as_str()) {
            self.profile.theme = "auto".into();
        }
        let lang = self.audio.final_transcription.trim().to_lowercase();
        self.audio.final_transcription =
            if FINAL_LANGUAGES.contains(&lang.as_str()) { lang } else { "off".into() };
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
        // Apps de visio : identifiants en minuscules, uniques, avec au moins un bundle.
        let mut seen = std::collections::HashSet::new();
        for app in self.audio.call_apps.iter_mut() {
            app.id = app.id.trim().to_lowercase().replace(' ', "-");
            app.name = app.name.trim().to_string();
            app.bundles = app.bundles.iter().map(|b| b.trim().to_string()).filter(|b| !b.is_empty()).collect();
        }
        self.audio.call_apps.retain(|a| !a.id.is_empty() && !a.bundles.is_empty() && seen.insert(a.id.clone()));
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
            "# Réglages de départ Sténo (sans chemins ni données personnelles)\n# Exporté le {}\n\n{}",
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
    fn choix_par_usage() {
        let mut a = AgentSettings::default();
        a.models.summary = "ancien".into();
        assert_eq!(a.agent_for(Tier::Summary), "claude-code");
        assert_eq!(a.model_for(Tier::Summary).as_deref(), Some("ancien"), "ancien format encore lu");
        a.tiers.summary = TierChoice { agent: "".into(), model: "opus".into(), effort: "high".into() };
        assert_eq!(a.model_for(Tier::Summary).as_deref(), Some("opus"));
        assert_eq!(a.effort_for(Tier::Summary).as_deref(), Some("high"));
        a.tiers.search = TierChoice { agent: "grok".into(), model: "".into(), effort: " ".into() };
        assert_eq!(a.agent_for(Tier::Search), "grok");
        assert_eq!(a.effort_for(Tier::Search), None);
        a.models.search = "pour-claude".into();
        assert_eq!(a.model_for(Tier::Search), None, "l'ancien modèle ne s'applique pas à un autre agent");
    }

    #[test]
    fn apps_de_visio() {
        // Anciens réglages : `auto_detect_teams`, pas de `call_apps`.
        let db = Db::open_in_memory().unwrap();
        db.set_setting("audio", r#"{"auto_detect_teams":false}"#).unwrap();
        let s = Settings::load(&db).unwrap();
        assert!(!s.audio.auto_detect_calls);
        assert_eq!(s.audio.call_apps.len(), 7);
        assert!(!s.audio.call_app("browser").unwrap().enabled);
        assert_eq!(short_app_name(s.audio.call_app("teams").unwrap()), "Teams");
        assert_eq!(short_app_name(s.audio.call_app("browser").unwrap()), "Navigateur");
        let mut s = s;
        s.audio.call_apps.push(CallApp { id: " Mon App ".into(), name: "Mon app".into(), bundles: vec!["com.x".into()], enabled: true, browser: false });
        s.audio.call_apps.push(CallApp { id: "slack".into(), name: "doublon".into(), bundles: vec!["x".into()], enabled: true, browser: false });
        let v = s.validated().unwrap();
        assert!(v.audio.call_app("mon-app").is_some());
        assert_eq!(v.audio.call_apps.iter().filter(|a| a.id == "slack").count(), 1);
    }

    #[test]
    fn theme_et_retranscription_valides() {
        let mut s = Settings::default();
        assert_eq!(s.profile.theme, "auto");
        assert_eq!(s.audio.final_transcription, "fr");
        s.profile.theme = "rose".into();
        s.audio.final_transcription = "FR ".into();
        let v = s.validated().unwrap();
        assert_eq!(v.profile.theme, "auto");
        assert_eq!(v.audio.final_transcription, "fr");
        let mut s = Settings::default();
        s.audio.final_transcription = "klingon".into();
        assert_eq!(s.validated().unwrap().audio.final_transcription, "off");
    }

    #[test]
    fn formats_perso_identifiants_uniques() {
        let mut s = Settings::default();
        let t = |id: &str, name: &str| CustomTemplate { id: id.into(), name: name.into(), instructions: "x".into() };
        s.report.templates = vec![t("", "Note COMEX"), t("", "Note COMEX"), t("mail", "Mon mail"), t("", "  ")];
        s.report.default_template = "inconnu".into();
        let v = s.validated().unwrap();
        let ids: Vec<&str> = v.report.templates.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["perso-note-comex", "perso-note-comex-2", "perso-mon-mail"]);
        assert_eq!(v.report.default_template, "standard");
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
