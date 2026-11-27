//! Contexte d'une réunion : texte libre, modèle de compte rendu et documents joints.
//!
//! Les documents sont convertis en texte à l'ajout (PDF, Excel/ODS/CSV, Word/RTF/ODT/HTML via
//! `textutil`, texte brut) : l'agent du compte rendu les lit dans la demande, sans outil.
//! Le texte de chaque document est plafonné pour garder une demande raisonnable.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

/// Taille maximale d'un fichier joint.
pub const MAX_FILE_BYTES: usize = 20 * 1024 * 1024;
/// Texte gardé par document (caractères).
pub const MAX_FILE_CHARS: usize = 60_000;
/// Texte de tous les documents d'une réunion envoyé à l'agent (caractères).
pub const MAX_TOTAL_CHARS: usize = 150_000;

/// Formats acceptés (extension → nature).
pub const ACCEPTED: &[(&str, &str)] = &[
    ("pdf", "pdf"),
    ("xlsx", "tableur"),
    ("xlsm", "tableur"),
    ("xls", "tableur"),
    ("ods", "tableur"),
    ("csv", "csv"),
    ("tsv", "csv"),
    ("docx", "document"),
    ("doc", "document"),
    ("rtf", "document"),
    ("odt", "document"),
    ("html", "document"),
    ("htm", "document"),
    ("md", "texte"),
    ("markdown", "texte"),
    ("txt", "texte"),
    ("json", "texte"),
    ("yaml", "texte"),
    ("yml", "texte"),
    ("xml", "texte"),
    ("log", "texte"),
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextFile {
    pub id: i64,
    pub meeting_id: String,
    pub name: String,
    /// `pdf | tableur | csv | document | texte`.
    pub kind: String,
    /// Taille du fichier d'origine (octets).
    pub size: i64,
    /// Longueur du texte extrait (caractères).
    pub chars: i64,
    pub truncated: bool,
    pub added_at: String,
}

/// Contexte complet d'une réunion (sans le texte des documents).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MeetingContext {
    pub meeting_id: String,
    pub text: String,
    /// Modèle de compte rendu choisi (`None` = modèle par défaut des réglages).
    pub template: Option<String>,
    pub files: Vec<ContextFile>,
    /// Heure (UTC) à laquelle la rédaction du compte rendu démarrera, si elle attend encore :
    /// délai laissé après la réunion pour ajouter du contexte.
    #[serde(default)]
    pub report_at: Option<String>,
}

fn extension(name: &str) -> String {
    std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default()
}

/// Nature d'un fichier d'après son extension, ou erreur si le format n'est pas pris en charge.
pub fn kind_of(name: &str) -> Result<&'static str> {
    let ext = extension(name);
    ACCEPTED
        .iter()
        .find(|(e, _)| *e == ext)
        .map(|(_, k)| *k)
        .with_context(|| format!("format non pris en charge : « {name} » (PDF, Excel, CSV, Word, Markdown ou texte)"))
}

/// Coupe un texte à `max` caractères (sur une fin de ligne si possible).
pub fn cap(text: &str, max: usize) -> (String, bool) {
    if text.chars().count() <= max {
        return (text.to_string(), false);
    }
    let cut: String = text.chars().take(max).collect();
    let cut = match cut.rfind('\n') {
        Some(i) if i > cut.len() / 2 => cut[..i].to_string(),
        _ => cut,
    };
    (cut, true)
}

/// Nettoyage commun : fins de ligne, espaces en fin de ligne, lignes vides en série.
fn tidy(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank = 0;
    for line in text.replace("\r\n", "\n").replace('\r', "\n").lines() {
        let l = line.trim_end();
        if l.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(l);
        out.push('\n');
    }
    out.trim().to_string()
}

fn spreadsheet_text(bytes: &[u8]) -> Result<String> {
    use calamine::{open_workbook_auto_from_rs, Reader};
    let mut wb = open_workbook_auto_from_rs(std::io::Cursor::new(bytes.to_vec())).context("classeur illisible")?;
    let mut out = String::new();
    for name in wb.sheet_names() {
        let Ok(range) = wb.worksheet_range(&name) else { continue };
        if range.is_empty() {
            continue;
        }
        out.push_str(&format!("### Feuille « {name} »\n"));
        for row in range.rows() {
            let cells: Vec<String> = row.iter().map(|c| c.to_string().replace(['\n', '\t'], " ")).collect();
            if cells.iter().all(|c| c.trim().is_empty()) {
                continue;
            }
            out.push_str(cells.join(" | ").trim_end_matches([' ', '|']));
            out.push('\n');
        }
        out.push('\n');
    }
    Ok(out)
}

/// Conversion par `textutil` (macOS) : Word, RTF, ODT, HTML.
fn textutil_text(name: &str, bytes: &[u8]) -> Result<String> {
    let dir = std::env::temp_dir().join(format!("steno-ctx-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir)?;
    let src = dir.join(format!("source.{}", extension(name)));
    std::fs::write(&src, bytes)?;
    let out = std::process::Command::new("/usr/bin/textutil")
        .args(["-convert", "txt", "-stdout"])
        .arg(&src)
        .output();
    let _ = std::fs::remove_dir_all(&dir);
    let out = out.context("textutil introuvable (conversion des documents Word : macOS uniquement)")?;
    if !out.status.success() {
        bail!("document illisible ({})", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Texte d'un document : `(nature, texte, tronqué)`.
pub fn extract(name: &str, bytes: &[u8]) -> Result<(&'static str, String, bool)> {
    if bytes.len() > MAX_FILE_BYTES {
        bail!("« {name} » dépasse {} Mo", MAX_FILE_BYTES / 1024 / 1024);
    }
    let kind = kind_of(name)?;
    let raw = match kind {
        "pdf" => pdf_extract::extract_text_from_mem(bytes).map_err(|e| anyhow::anyhow!("PDF illisible : {e}"))?,
        "tableur" => spreadsheet_text(bytes)?,
        "document" => textutil_text(name, bytes)?,
        _ => String::from_utf8_lossy(bytes).into_owned(),
    };
    let text = tidy(&raw);
    if text.is_empty() {
        bail!("aucun texte trouvé dans « {name} » (document scanné ?)");
    }
    let (text, truncated) = cap(&text, MAX_FILE_CHARS);
    Ok((kind, text, truncated))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_et_texte_brut() {
        assert_eq!(kind_of("Compte Rendu.PDF").unwrap(), "pdf");
        assert_eq!(kind_of("budget.xlsx").unwrap(), "tableur");
        assert!(kind_of("photo.png").is_err());
        let (k, t, cut) = extract("notes.md", "# TVA\r\n\r\n\r\n\r\nBelgique : 19 %   \n".as_bytes()).unwrap();
        assert_eq!(k, "texte");
        assert_eq!(t, "# TVA\n\nBelgique : 19 %");
        assert!(!cut);
        assert!(extract("vide.txt", b"   \n ").is_err());
    }

    #[test]
    fn plafond_sur_une_fin_de_ligne() {
        let text = "ligne\n".repeat(20);
        let (t, cut) = cap(&text, 50);
        assert!(cut);
        assert!(t.chars().count() <= 50);
        assert!(t.ends_with("ligne"));
    }

    #[test]
    fn csv_et_word() {
        let (k, t, _) = extract("taux.csv", b"pays;taux\nBelgique;21\n").unwrap();
        assert_eq!(k, "csv");
        assert!(t.contains("Belgique;21"));
        #[cfg(target_os = "macos")]
        {
            let (k, t, _) = extract("note.html", b"<html><body><h1>TVA</h1><p>Belgique 21 %</p></body></html>").unwrap();
            assert_eq!(k, "document");
            assert!(t.contains("Belgique 21 %"), "{t}");
        }
    }
}
