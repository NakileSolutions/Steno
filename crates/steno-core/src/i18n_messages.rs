//! Traduction anglaise des messages d'erreur et de diagnostic rédigés en français par le cœur
//! (erreurs `anyhow`, `steno doctor`, erreurs du sidecar audio).
//!
//! Les messages restent écrits en français dans le code ; la traduction se fait aux frontières
//! (commandes de l'app, API HTTP, diagnostic, état du direct, notifications d'échec) avec
//! [`localize`]. Dans les motifs, `{}` remplace toute partie variable ; le modèle anglais
//! reprend les `{}` dans le même ordre.

use regex::Regex;
use std::sync::OnceLock;

/// (motif français, modèle anglais).
static TABLE: &[(&str, &str)] = &[
    // ------------------------------------------------------------ core.rs
    ("agent inconnu", "unknown agent"),
    ("agent inconnu : {}", "unknown agent: {}"),
    ("réunion confidentielle : aucun appel IA", "confidential meeting: no AI calls"),
    ("la session de l'agent a expiré ; relancez la demande", "the agent session has expired; send the request again"),
    ("aucune nouvelle version à installer", "no new version to install"),
    (
        "une réunion est en cours : installez la mise à jour après la réunion",
        "a meeting is in progress: install the update after the meeting",
    ),
    (
        "installation impossible hors du moteur asynchrone de l'app",
        "cannot install outside the app's async runtime",
    ),
    (
        "la capture audio n'est pas démarrée : ouvrez les Réglages Système",
        "audio capture is not started: open System Settings",
    ),
    ("la session en direct n'est pas démarrée", "the live session is not started"),
    ("session en direct arrêtée", "live session stopped"),
    ("réunion introuvable", "meeting not found"),
    ("cette réunion n'est pas une réunion prévue", "this meeting is not a planned meeting"),
    ("une autre réunion a été démarrée ({})", "another meeting was started ({})"),
    ("aucune écoute en cours", "not listening"),
    ("la capture audio n'est pas disponible", "audio capture is not available"),
    ("la demande est vide", "the request is empty"),
    ("phrase introuvable", "sentence not found"),
    ("la réunion est encore en cours", "the meeting is still in progress"),
    ("modèle de compte rendu inconnu : {}", "unknown summary template: {}"),
    ("exemple trop court : collez un compte rendu complet", "example too short: paste a complete summary"),
    ("l'agent n'a pas proposé de format", "the agent did not suggest a format"),
    (
        "CLI « steno » introuvable : installez Sténo avec « make install »",
        "“steno” CLI not found: install Sténo with “make install”",
    ),
    ("aucun agent compatible trouvé (Claude Code, Codex…)", "no compatible agent found (Claude Code, Codex…)"),
    ("nom de fichier manquant", "missing file name"),
    ("contenu du fichier illisible (base64 attendu)", "unreadable file content (base64 expected)"),
    ("document introuvable", "document not found"),
    ("Corrélation impossible : {}", "Cannot correlate: {}"),
    ("corrélation impossible : {}", "cannot correlate: {}"),
    ("aucun compte rendu à corréler", "no summary to correlate"),
    ("une corrélation est déjà en cours", "a correlation is already in progress"),
    ("écriture de {}", "writing {}"),
    ("aucune réunion en cours pour cette note", "no meeting in progress for this note"),
    // ------------------------------------------------------------ export.rs
    ("format d'export inconnu : {}", "unknown export format: {}"),
    // ------------------------------------------------------------ updates.rs
    ("réponse GitHub illisible", "unreadable GitHub response"),
    ("curl introuvable", "curl not found"),
    ("GitHub injoignable : {}", "GitHub unreachable: {}"),
    ("GitHub a répondu {}", "GitHub responded {}"),
    ("git introuvable", "git not found"),
    ("git {} : {}", "git {}: {}"),
    ("vérification interrompue", "check interrupted"),
    ("une installation est déjà en cours", "an installation is already in progress"),
    ("dossier des sources introuvable", "source folder not found"),
    (
        "dossier des sources introuvable (Sténo a-t-il été installé avec `make install` ?)",
        "source folder not found (was Sténo installed with `make install`?)",
    ),
    (
        "outils manquants pour compiler : {} — installez-les, ou mettez à jour à la main (README)",
        "tools missing to build: {} — install them, or update manually (README)",
    ),
    (
        "les sources contiennent des modifications locales : mettez à jour à la main",
        "the sources contain local changes: update manually",
    ),
    (
        "les sources sont sur la branche « {} » : mettez à jour à la main",
        "the sources are on branch “{}”: update manually",
    ),
    ("sources illisibles : {}", "unreadable sources: {}"),
    ("Vérification impossible : {}", "Check failed: {}"),
    ("Installation de Sténo {}… (quelques minutes)", "Installing Sténo {}… (a few minutes)"),
    ("Sténo {} est installé : redémarrez pour l'utiliser.", "Sténo {} is installed: restart to use it."),
    ("Installation en échec : {}", "Installation failed: {}"),
    ("installation interrompue", "installation interrupted"),
    ("tag {} introuvable", "tag {} not found"),
    ("les sources ont divergé de la version publiée", "the sources have diverged from the published version"),
    (
        "make introuvable (outils de développement Xcode installés ?)",
        "make not found (are the Xcode developer tools installed?)",
    ),
    ("`make install` a échoué (voir {})", "`make install` failed (see {})"),
    // ------------------------------------------------------------ sidecar.rs
    ("impossible de lancer le sidecar audio « {} »", "cannot start the audio sidecar “{}”"),
    ("stdout du sidecar", "sidecar stdout"),
    ("stderr du sidecar", "sidecar stderr"),
    ("stdin du sidecar", "sidecar stdin"),
    // ------------------------------------------------------------ agents/
    (
        "{} ne sait pas bloquer les outils MCP en écriture : connecteurs refusés",
        "{} cannot block write MCP tools: connectors refused",
    ),
    ("{} (« {} ») introuvable", "{} (“{}”) not found"),
    ("annulée", "cancelled"),
    ("aucune commande personnalisée n'est configurée", "no custom command is configured"),
    ("la commande personnalisée ne peut pas utiliser de connecteurs", "the custom command cannot use connectors"),
    ("La commande personnalisée", "The custom command"),
    ("« {} » introuvable", "“{}” not found"),
    ("« {} » introuvable dans le PATH", "“{}” not found in PATH"),
    ("« {} {} » ne répond pas", "“{} {}” is not responding"),
    ("« {} {} » a échoué : {}", "“{} {}” failed: {}"),
    ("la liste des connecteurs ne répond pas (90 s)", "the connector list is not responding (90 s)"),
    ("impossible de lancer l'agent", "cannot start the agent"),
    ("{} : délai dépassé", "{}: timed out"),
    ("{} a échoué (code {}) : {}", "{} failed (code {}): {}"),
    // ------------------------------------------------------------ db/
    ("impossible de créer {}", "cannot create {}"),
    ("impossible d'ouvrir la base {}", "cannot open database {}"),
    ("sauvegarde avant migration", "backup before migration"),
    ("migration de la base", "database migration"),
    // ------------------------------------------------------------ permissions.rs
    ("permission inconnue : {}", "unknown permission: {}"),
    ("impossible d'ouvrir les Réglages Système", "cannot open System Settings"),
    ("Réglages Système : macOS uniquement", "System Settings: macOS only"),
    // ------------------------------------------------------------ live.rs
    ("réunion prévue introuvable", "planned meeting not found"),
    ("sidecar absent", "sidecar missing"),
    ("la capture audio n'est pas disponible (sidecar absent)", "audio capture is not available (sidecar missing)"),
    ("le sidecar audio s'est arrêté", "the audio sidecar stopped"),
    ("session en direct indisponible", "live session unavailable"),
    ("retranscription : délai dépassé", "retranscription: timed out"),
    ("retranscription interrompue", "retranscription interrupted"),
    ("pas d'audio conservé pour cette réunion", "no audio kept for this meeting"),
    ("diarisation : délai dépassé", "diarization: timed out"),
    ("diarisation interrompue", "diarization interrupted"),
    // ------------------------------------------------------------ planning.rs
    ("calendrier : délai dépassé", "calendar: timed out"),
    ("calendrier interrompu", "calendar interrupted"),
    ("date prévue invalide (ISO 8601 attendu)", "invalid scheduled date (ISO 8601 expected)"),
    ("l'agent n'a proposé aucune question", "the agent suggested no questions"),
    ("aucune réponse", "no answer"),
    // ------------------------------------------------------------ settings.rs
    ("le mot d'appel ne peut pas être vide", "the wake word cannot be empty"),
    ("mode de corrélation inconnu : {}", "unknown correlation mode: {}"),
    ("motif des clés Jira invalide", "invalid Jira key pattern"),
    ("TOML invalide", "invalid TOML"),
    ("réglages importés invalides", "invalid imported settings"),
    // ------------------------------------------------------------ jobs.rs
    ("type de tâche inconnu : {}", "unknown job type: {}"),
    ("request_id manquant", "missing request_id"),
    ("aucune réunion à récapituler", "no meeting to recap"),
    ("réunion manquante", "missing meeting"),
    // ------------------------------------------------------------ context.rs
    (
        "format non pris en charge : « {} » (PDF, Excel, CSV, Word, Markdown ou texte)",
        "unsupported format: “{}” (PDF, Excel, CSV, Word, Markdown or text)",
    ),
    ("classeur illisible", "unreadable workbook"),
    (
        "textutil introuvable (conversion des documents Word : macOS uniquement)",
        "textutil not found (Word document conversion: macOS only)",
    ),
    ("document illisible ({})", "unreadable document ({})"),
    ("« {} » dépasse {} Mo", "“{}” exceeds {} MB"),
    ("PDF illisible : {}", "unreadable PDF: {}"),
    ("aucun texte trouvé dans « {} » (document scanné ?)", "no text found in “{}” (scanned document?)"),
    // ------------------------------------------------------------ api.rs, commands.rs
    ("argument « {} » invalide : {}", "invalid argument “{}”: {}"),
    ("version introuvable", "version not found"),
    ("statut inconnu", "unknown status"),
    ("dossier introuvable : {}", "folder not found: {}"),
    ("le micro n'a rien renvoyé à temps", "the microphone returned nothing in time"),
    ("commande inconnue : {}", "unknown command: {}"),
    ("une réunion est en cours : redémarrez après la réunion", "a meeting is in progress: restart after the meeting"),
    // ------------------------------------------------------------ doctor.rs : noms
    ("Apps de visio", "Video call apps"),
    ("Système", "System"),
    ("Base de données", "Database"),
    ("Capture audio", "Audio capture"),
    ("Micro", "Microphone"),
    ("Audio du système", "System audio"),
    ("Accessibilité", "Accessibility"),
    ("Agent IA", "AI agent"),
    ("Connecteurs", "Connectors"),
    ("Clé d'API", "API key"),
    ("Politique lecture seule", "Read-only policy"),
    ("Test réel « poste sur Slack »", "Live test “post on Slack”"),
    // ------------------------------------------------------------ doctor.rs : messages
    (
        "macOS {} : les process taps Core Audio demandent macOS 14.2+",
        "macOS {}: Core Audio process taps require macOS 14.2+",
    ),
    ("version inconnue", "unknown version"),
    ("{} — version inconnue", "{} — unknown version"),
    ("aucune app de visio surveillée n'est installée", "none of the monitored video call apps is installed"),
    ("surveillées et installées : {}", "monitored and installed: {}"),
    ("surveillées et installées : {} (absentes : {})", "monitored and installed: {} (missing: {})"),
    (
        "hors macOS : capture audio indisponible (mode simulation possible)",
        "not on macOS: audio capture unavailable (simulation mode available)",
    ),
    ("base en mémoire", "in-memory database"),
    ("sidecar steno-audio lancé", "steno-audio sidecar running"),
    ("sidecar steno-audio introuvable", "steno-audio sidecar not found"),
    ("sidecar en cours de démarrage", "sidecar starting"),
    ("sidecar : {}", "sidecar: {}"),
    ("autorisé", "granted"),
    ("refusé", "denied"),
    ("état inconnu (vérifié au premier appel)", "unknown status (checked on the first call)"),
    (
        "{} ne sait pas bloquer les outils MCP en écriture : connecteurs désactivés",
        "{} cannot block write MCP tools: connectors disabled",
    ),
    ("{} détecté(s), {} activé(s)", "{} detected, {} enabled"),
    (
        "{} présente et autorisée : appels facturés hors abonnement",
        "{} present and allowed: calls billed outside the subscription",
    ),
    ("{} présente mais ignorée (non transmise à l'agent)", "{} present but ignored (not passed to the agent)"),
    (
        "les outils d'écriture et commandes dangereuses sont refusés",
        "write tools and dangerous commands are refused",
    ),
    ("la politique laisse passer {}", "the policy lets {} through"),
    ("la politique laisse passer « {} »", "the policy lets “{}” through"),
    ("refusé par l'agent, aucun outil d'écriture exécuté", "refused by the agent, no write tool run"),
    ("outils d'écriture exécutés : {}", "write tools run: {}"),
    ("test impossible : {}", "test failed: {}"),
    // ------------------------------------------------------------ doctor.rs : marche à suivre
    ("Mettre à jour macOS.", "Update macOS."),
    ("Réglages → Audio & transcription → Apps de visio.", "Settings → Audio & transcription → Video call apps."),
    (
        "Recompiler avec `make install` (il est embarqué dans Steno.app).",
        "Rebuild with `make install` (it is bundled in Steno.app).",
    ),
    (
        "Voir les journaux dans ~/Library/Application Support/Steno/logs.",
        "See the logs in ~/Library/Application Support/Steno/logs.",
    ),
    (
        "Réglages Système → Confidentialité et sécurité → Micro → cocher Sténo.",
        "System Settings → Privacy & Security → Microphone → turn on Sténo.",
    ),
    (
        "Réglages Système → Confidentialité et sécurité → Enregistrement de l'écran et de l'audio système → cocher Sténo. Si la case est absente ou grisée, le MDM de l'entreprise la bloque (spike S4).",
        "System Settings → Privacy & Security → Screen & System Audio Recording → turn on Sténo. If the option is missing or greyed out, your company's MDM blocks it (spike S4).",
    ),
    (
        "Réglages Système → Confidentialité et sécurité → Accessibilité → cocher Sténo (titre de la réunion Teams).",
        "System Settings → Privacy & Security → Accessibility → turn on Sténo (Teams meeting title).",
    ),
    (
        "Installer le CLI de l'agent et s'y connecter, ou en choisir un autre dans Réglages → Agent IA.",
        "Install the agent's CLI and sign in, or choose another one in Settings → AI agent.",
    ),
    ("Réglages → Connecteurs → Rafraîchir.", "Settings → Connectors → Refresh."),
    (
        "Vérifier ~/Library/Application Support/Steno/policy.toml.",
        "Check ~/Library/Application Support/Steno/policy.toml.",
    ),
    ("Désactiver tous les connecteurs et signaler le problème.", "Disable all connectors and report the problem."),
    // ------------------------------------------------------------ sidecar audio (Swift)
    (
        "Modèle de transcription indisponible : {}. Lancez `make install` (téléchargement des modèles) puis réessayez.",
        "Transcription model unavailable: {}. Run `make install` (downloads the models), then try again.",
    ),
    ("Accès au micro refusé : test impossible.", "Microphone access denied: cannot run the test."),
    ("Micro indisponible pour le test : {}", "Microphone unavailable for the test: {}"),
    ("Transcription du test impossible : {}", "Could not transcribe the test: {}"),
    ("Diarisation impossible : {}", "Diarization failed: {}"),
    ("Modèles de diarisation indisponibles : {}", "Diarization models unavailable: {}"),
    ("Retranscription impossible : {}", "Retranscription failed: {}"),
    ("Transcription impossible : {}", "Transcription failed: {}"),
    (
        "Accès au micro refusé : autorisez Sténo dans Réglages Système › Confidentialité et sécurité › Micro.",
        "Microphone access denied: allow Sténo in System Settings › Privacy & Security › Microphone.",
    ),
    (
        "Micro indisponible après un changement de périphérique ({}). Nouvel essai toutes les 5 s.",
        "Microphone unavailable after a device change ({}). Retrying every 5 s.",
    ),
    ("Démarrage du micro impossible : {}", "Could not start the microphone: {}"),
    (
        "Aucune app de visio n'est surveillée : seul le micro est capté, pas le son des participants.",
        "No video call app is monitored: only the microphone is recorded, not the other participants.",
    ),
    (
        "Aucun processus audio de {} : l'app n'est pas lancée ou n'a pas encore ouvert l'audio. Le micro est capté ; le son des participants le sera dès que {} apparaîtra.",
        "No audio process for {}: the app is not running or has not opened audio yet. The microphone is recorded; the other participants will be as soon as {} appears.",
    ),
    (
        "Accès à l'audio système refusé : autorisez Sténo dans Réglages Système › Confidentialité et sécurité › Enregistrement audio du système.",
        "System audio access denied: allow Sténo in System Settings › Privacy & Security › System Audio Recording.",
    ),
    (
        "Capture du son du Mac impossible ({}). Nouvel essai en cours.",
        "Could not capture the Mac's audio ({}). Retrying.",
    ),
    (
        "Capture du son de {} impossible ({}). Nouvel essai en cours.",
        "Could not capture audio from {} ({}). Retrying.",
    ),
    ("Micro « {} » introuvable : micro par défaut utilisé.", "Microphone “{}” not found: using the default microphone."),
    ("Fichier audio introuvable : {}", "Audio file not found: {}"),
    ("aucun micro disponible", "no microphone available"),
    ("redémarrage du micro impossible : {}", "could not restart the microphone: {}"),
    ("accès à l'audio système refusé (OSStatus {} « {} »)", "system audio access denied (OSStatus {} “{}”)"),
    ("{} a échoué (OSStatus {} « {} »)", "{} failed (OSStatus {} “{}”)"),
    ("Commande illisible : JSON invalide.", "Unreadable command: invalid JSON."),
    ("Commande illisible : champ \"cmd\" absent.", "Unreadable command: missing \"cmd\" field."),
    ("Commande inconnue : \"{}\".", "Unknown command: \"{}\"."),
    ("Commande \"{}\" : champ \"{}\" absent ou invalide.", "Command \"{}\": missing or invalid \"{}\" field."),
];

struct Entry {
    re: Regex,
    en: &'static str,
}

/// Motif → expression régulière ancrée (`{}` = `.*?`).
fn pattern_regex(pattern: &str) -> String {
    let parts: Vec<String> = pattern.split("{}").map(regex::escape).collect();
    format!("(?s)^{}$", parts.join("(.*?)"))
}

/// Entrées compilées, les plus précises (le plus de texte fixe) en premier.
fn entries() -> &'static [Entry] {
    static ENTRIES: OnceLock<Vec<Entry>> = OnceLock::new();
    ENTRIES.get_or_init(|| {
        let mut v: Vec<(usize, Entry)> = TABLE
            .iter()
            .map(|(fr, en)| {
                let fixed = fr.replace("{}", "").chars().count();
                (fixed, Entry { re: Regex::new(&pattern_regex(fr)).expect("motif de traduction invalide"), en })
            })
            .collect();
        v.sort_by(|a, b| b.0.cmp(&a.0));
        v.into_iter().map(|(_, e)| e).collect()
    })
}

/// Traduction d'un message entier, ou `None` s'il ne correspond à aucun motif.
/// Les parties variables sont elles-mêmes traduites quand elles correspondent à un motif
/// (message imbriqué), sinon recopiées telles quelles.
fn whole(msg: &str) -> Option<String> {
    for e in entries() {
        if let Some(c) = e.re.captures(msg) {
            let mut out = String::new();
            for (i, part) in e.en.split("{}").enumerate() {
                if i > 0 {
                    out.push_str(&to_english(c.get(i).map(|m| m.as_str()).unwrap_or("")));
                }
                out.push_str(part);
            }
            return Some(out);
        }
    }
    None
}

fn to_english(msg: &str) -> String {
    if let Some(t) = whole(msg) {
        return t;
    }
    // Chaîne « contexte: cause » (anyhow `{:#}`) ou « code : message » : le plus long morceau
    // traduisible d'abord, en partant de la gauche.
    let segs: Vec<&str> = msg.split(": ").collect();
    if segs.len() < 2 {
        return msg.to_string();
    }
    let mut out: Vec<String> = vec![];
    let mut changed = false;
    let mut i = 0;
    while i < segs.len() {
        let found = (i + 1..=segs.len()).rev().find_map(|j| {
            let chunk = segs[i..j].join(": ");
            let t = chunk.trim();
            if t.is_empty() {
                return None;
            }
            whole(t).map(|en| (j, en))
        });
        match found {
            Some((j, en)) => {
                out.push(en);
                changed = true;
                i = j;
            }
            None => {
                out.push(segs[i].trim().to_string());
                i += 1;
            }
        }
    }
    if changed { out.join(": ") } else { msg.to_string() }
}

/// Message du cœur dans la langue des réglages : inchangé en français, traduit en anglais
/// quand il correspond à la table (sinon inchangé).
pub fn localize(lang: &str, msg: &str) -> String {
    if lang != "en" {
        return msg.to_string();
    }
    to_english(msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_entier_avec_parties_variables() {
        assert_eq!(localize("en", "réunion introuvable"), "meeting not found");
        assert_eq!(localize("en", "« codex exec » a échoué : boom"), "“codex exec” failed: boom");
        assert_eq!(localize("en", "tag v1.2.0 introuvable"), "tag v1.2.0 not found");
        assert_eq!(
            localize("en", "Capture du son du Mac impossible (err -1). Nouvel essai en cours."),
            "Could not capture the Mac's audio (err -1). Retrying."
        );
        assert_eq!(
            localize("en", "Capture du son de Teams impossible (err -1). Nouvel essai en cours."),
            "Could not capture audio from Teams (err -1). Retrying."
        );
    }

    #[test]
    fn chaine_de_contextes() {
        assert_eq!(
            localize("en", "impossible d'ouvrir la base /tmp/x.db: unable to open database file"),
            "cannot open database /tmp/x.db: unable to open database file"
        );
        assert_eq!(localize("en", "migration de la base: disk full"), "database migration: disk full");
        assert_eq!(localize("en", "session en direct arrêtée: channel closed"), "live session stopped: channel closed");
        // Message imbriqué traduit lui aussi.
        assert_eq!(
            localize("en", "Corrélation impossible : réunion confidentielle : aucun appel IA"),
            "Cannot correlate: confidential meeting: no AI calls"
        );
        // Erreur du sidecar : « code : message ».
        assert_eq!(
            localize("en", "mic_denied : Accès au micro refusé : test impossible."),
            "mic_denied: Microphone access denied: cannot run the test."
        );
        assert_eq!(
            localize("en", "asr_failed : Diarisation impossible : Fichier audio introuvable : /a/b.wav"),
            "asr_failed: Diarization failed: Audio file not found: /a/b.wav"
        );
    }

    #[test]
    fn francais_inchange() {
        for m in ["réunion introuvable", "migration de la base: disk full", "Autre chose"] {
            assert_eq!(localize("fr", m), m);
        }
    }

    #[test]
    fn texte_inconnu_inchange() {
        for m in ["something went wrong", "erreur : quelque chose", "a: b: c", "", "foo : "] {
            assert_eq!(localize("en", m), m);
        }
    }

    #[test]
    fn table_coherente() {
        let mut seen = std::collections::HashSet::new();
        for (fr, en) in TABLE {
            assert!(seen.insert(*fr), "motif en double : {fr}");
            assert_eq!(fr.matches("{}").count(), en.matches("{}").count(), "nombre de {{}} différent : {fr}");
            assert!(!fr.replace("{}", "").trim().is_empty(), "motif sans texte fixe : {fr}");
            // Un message anglais ne doit pas être retraduit.
            assert_eq!(localize("en", &en.replace("{}", "x")), en.replace("{}", "x"), "{en}");
        }
    }

    /// Littéraux des `bail!`, `anyhow!`, `.context(` et `.with_context(` (hors tests),
    /// normalisés (`{nom}` → `{}`).
    fn error_literals(src: &str) -> Vec<String> {
        let src = src.split("#[cfg(test)]").next().unwrap_or(src);
        let mut out = vec![];
        for tok in ["bail!(", "anyhow!(", ".context(", ".with_context("] {
            let mut from = 0;
            while let Some(pos) = src[from..].find(tok) {
                let start = from + pos + tok.len();
                from = start;
                let mut rest = src[start..].trim_start();
                if let Some(r) = rest.strip_prefix("||") {
                    rest = r.trim_start();
                }
                if let Some(r) = rest.strip_prefix("format!(") {
                    rest = r.trim_start();
                }
                let Some(body) = rest.strip_prefix('"') else { continue };
                let mut lit = String::new();
                let mut chars = body.chars();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => {
                            if let Some(n) = chars.next() {
                                lit.push(n);
                            }
                        }
                        _ => lit.push(c),
                    }
                }
                out.push(normalize(&lit));
            }
        }
        out
    }

    fn normalize(lit: &str) -> String {
        let lit = lit.replace("{{", "\u{1}").replace("}}", "\u{2}");
        let re = Regex::new(r"\{[^{}]*\}").unwrap();
        re.replace_all(&lit, "{}").replace('\u{1}', "{").replace('\u{2}', "}")
    }

    fn rs_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                rs_files(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }

    /// Toute nouvelle erreur en français doit avoir sa traduction dans la table.
    #[test]
    fn toutes_les_erreurs_sont_traduites() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = vec![];
        rs_files(&root.join("src"), &mut files);
        rs_files(&root.join("../../app/src-tauri/src"), &mut files);
        // Littéraux sans texte à traduire.
        let neutral = ["{}", "stdout", "stderr"];
        let patterns: std::collections::HashSet<&str> = TABLE.iter().map(|(fr, _)| *fr).collect();
        let mut missing = vec![];
        let mut count = 0;
        for f in &files {
            if f.ends_with("i18n_messages.rs") {
                continue;
            }
            for lit in error_literals(&std::fs::read_to_string(f).unwrap()) {
                if neutral.contains(&lit.as_str()) {
                    continue;
                }
                count += 1;
                let sample = lit.replace("{}", "X");
                if !patterns.contains(lit.as_str()) && localize("en", &sample) == sample {
                    missing.push(format!("{}: {lit}", f.display()));
                }
            }
        }
        assert!(count > 100, "trop peu de littéraux trouvés ({count}) : analyse cassée ?");
        assert!(missing.is_empty(), "messages sans traduction anglaise :\n{}", missing.join("\n"));
    }
}
