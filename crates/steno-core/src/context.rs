//! Contexte d'une réunion : texte libre, modèle de compte rendu et documents joints.
//!
//! Les documents sont convertis en texte à l'ajout (PDF, Excel/ODS/CSV, Word/RTF/ODT/HTML, texte
//! brut) : l'agent du compte rendu les lit dans la demande, sans outil. Word (docx), OpenDocument
//! (odt), HTML et RTF sont lus en Rust (sur macOS, `textutil` d'abord) ; l'ancien format Word
//! (`.doc`) passe par `textutil` sur macOS, `antiword` ou `catdoc` ailleurs.
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
    /// Format repris de la dernière réunion du même titre quand aucun n'est choisi ici
    /// (prime sur le format par défaut des réglages).
    #[serde(default)]
    pub inherited_template: Option<String>,
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
#[cfg(target_os = "macos")]
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
    let out = out.context("textutil introuvable")?;
    if !out.status.success() {
        bail!("document illisible ({})", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Texte d'un document Word, RTF, ODT ou HTML. Sur macOS, `textutil` d'abord, puis la lecture
/// en Rust si `textutil` échoue.
fn document_text(name: &str, bytes: &[u8]) -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        textutil_text(name, bytes).or_else(|e| portable_document_text(name, bytes).map_err(|_| e))
    }
    #[cfg(not(target_os = "macos"))]
    {
        portable_document_text(name, bytes)
    }
}

/// Lecture sans outil du système (sauf `.doc` : `antiword` ou `catdoc`).
fn portable_document_text(name: &str, bytes: &[u8]) -> Result<String> {
    match extension(name).as_str() {
        "docx" => docx_text(bytes),
        "odt" => odt_text(bytes),
        "html" | "htm" => Ok(html_text(&String::from_utf8_lossy(bytes))),
        "rtf" => Ok(rtf_text(&String::from_utf8_lossy(bytes))),
        _ => legacy_doc_text(bytes),
    }
}

/// Fichier XML d'une archive zip (docx, odt).
fn zip_entry(bytes: &[u8], entry: &str) -> Result<String> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("document illisible (archive zip attendue)")?;
    let mut f = zip.by_name(entry).with_context(|| format!("document illisible ({entry} absent)"))?;
    let mut xml = String::new();
    f.read_to_string(&mut xml).with_context(|| format!("document illisible ({entry} n'est pas en UTF-8)"))?;
    Ok(xml)
}

/// Parcourt un XML et rassemble son texte. `on_tag(nom local, ouverture, attributs)` renvoie le
/// texte à insérer pour une balise (fin de paragraphe, tabulation…) ; `keep(pile)` dit si le
/// texte à cet endroit est gardé.
fn xml_text(
    xml: &str,
    collapse_spaces: bool,
    on_tag: impl Fn(&str, Tag, &quick_xml::events::BytesStart) -> Option<String>,
    keep: impl Fn(&[String]) -> bool,
) -> Result<String> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut stack: Vec<String> = vec![];
    let mut out = String::new();
    loop {
        match reader.read_event().map_err(|e| anyhow::anyhow!("document illisible ({e})"))? {
            Event::Start(e) => {
                let n = e.local_name().as_ref().to_string();
                if let Some(t) = on_tag(&n, Tag::Open, &e) {
                    out.push_str(&t);
                }
                stack.push(n);
            }
            Event::Empty(e) => {
                let n = e.local_name().as_ref().to_string();
                if let Some(t) = on_tag(&n, Tag::Empty, &e) {
                    out.push_str(&t);
                }
            }
            Event::End(e) => {
                let n = e.local_name().as_ref().to_string();
                if let Some(pos) = stack.iter().rposition(|x| *x == n) {
                    stack.truncate(pos);
                }
                if let Some(t) = on_tag(&n, Tag::Close, &quick_xml::events::BytesStart::new(n.clone())) {
                    out.push_str(&t);
                }
            }
            Event::Text(t) if keep(&stack) => {
                let text = t.xml10_content();
                if collapse_spaces {
                    // OpenDocument : une suite d'espaces vaut une espace (`text:s` pour les autres).
                    let mut last_space = out.ends_with(' ');
                    for c in text.chars() {
                        let space = matches!(c, ' ' | '\t' | '\n' | '\r');
                        if !(space && last_space) {
                            out.push(if space { ' ' } else { c });
                        }
                        last_space = space;
                    }
                } else {
                    out.push_str(&text);
                }
            }
            Event::CData(t) if keep(&stack) => out.push_str(&t.xml10_content()),
            Event::GeneralRef(r) if keep(&stack) => {
                let name = r.xml10_content();
                match r.resolve_char_ref() {
                    Ok(Some(c)) => out.push(c),
                    _ => out.push_str(match name.as_ref() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => "",
                    }),
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

#[derive(Clone, Copy, PartialEq)]
enum Tag {
    Open,
    Close,
    Empty,
}

/// Word (docx) : `word/document.xml`, texte des balises `w:t`.
fn docx_text(bytes: &[u8]) -> Result<String> {
    let xml = zip_entry(bytes, "word/document.xml")?;
    xml_text(
        &xml,
        false,
        |name, tag, _| match (name, tag) {
            ("p", Tag::Close) | ("br", Tag::Empty) | ("cr", Tag::Empty) => Some("\n".into()),
            ("tab", Tag::Empty) => Some("\t".into()),
            ("tc", Tag::Close) => Some("\t".into()),
            _ => None,
        },
        |stack| stack.last().is_some_and(|n| n == "t"),
    )
}

/// OpenDocument (odt) : `content.xml`, texte du corps du document.
fn odt_text(bytes: &[u8]) -> Result<String> {
    let xml = zip_entry(bytes, "content.xml")?;
    xml_text(
        &xml,
        true,
        |name, tag, e| match (name, tag) {
            ("p" | "h", Tag::Close) | ("p" | "h", Tag::Empty) | ("line-break", _) => Some("\n".into()),
            ("tab", Tag::Empty) => Some("\t".into()),
            ("s", Tag::Empty) => {
                let n = e
                    .try_get_attribute("text:c")
                    .ok()
                    .flatten()
                    .and_then(|a| a.value.parse::<usize>().ok())
                    .unwrap_or(1);
                Some(" ".repeat(n.min(100)))
            }
            _ => None,
        },
        |stack| {
            stack.iter().any(|n| n == "body")
                && stack.iter().any(|n| n == "p" || n == "h")
                && !stack.iter().any(|n| n == "note-citation" || n == "tracked-changes")
        },
    )
}

/// Entité HTML nommée (les plus courantes en français et en anglais).
fn html_entity(name: &str) -> Option<&'static str> {
    Some(match name {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => "\u{a0}",
        "eacute" => "é",
        "Eacute" => "É",
        "egrave" => "è",
        "Egrave" => "È",
        "ecirc" => "ê",
        "Ecirc" => "Ê",
        "euml" => "ë",
        "agrave" => "à",
        "Agrave" => "À",
        "acirc" => "â",
        "auml" => "ä",
        "ccedil" => "ç",
        "Ccedil" => "Ç",
        "icirc" => "î",
        "iuml" => "ï",
        "ocirc" => "ô",
        "ouml" => "ö",
        "ugrave" => "ù",
        "ucirc" => "û",
        "uuml" => "ü",
        "oelig" => "œ",
        "OElig" => "Œ",
        "aelig" => "æ",
        "szlig" => "ß",
        "ntilde" => "ñ",
        "euro" => "€",
        "pound" => "£",
        "copy" => "©",
        "reg" => "®",
        "trade" => "™",
        "deg" => "°",
        "laquo" => "«",
        "raquo" => "»",
        "lsquo" => "‘",
        "rsquo" => "’",
        "ldquo" => "“",
        "rdquo" => "”",
        "hellip" => "…",
        "mdash" => "—",
        "ndash" => "–",
        "bull" => "•",
        "middot" => "·",
        "times" => "×",
        "shy" => "",
        _ => return None,
    })
}

/// Remplace les entités `&nom;`, `&#123;`, `&#x1F;` d'un texte HTML.
fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let end = rest[1..].find(|c: char| !(c.is_ascii_alphanumeric() || c == '#')).map(|j| j + 1);
        let decoded = end.filter(|&j| rest[j..].starts_with(';')).and_then(|j| {
            let name = &rest[1..j];
            let c = if let Some(num) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                u32::from_str_radix(num, 16).ok().and_then(char::from_u32).map(String::from)
            } else if let Some(num) = name.strip_prefix('#') {
                num.parse::<u32>().ok().and_then(char::from_u32).map(String::from)
            } else {
                html_entity(name).map(String::from)
            };
            c.map(|c| (c, j + 1))
        });
        match decoded {
            Some((c, len)) => {
                out.push_str(&c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// HTML : balises retirées (sauf leur rôle de saut de ligne), scripts et styles ignorés,
/// espaces regroupés comme dans un navigateur, entités décodées.
fn html_text(html: &str) -> String {
    const BLOCKS: &[&str] = &[
        "p", "div", "br", "li", "tr", "h1", "h2", "h3", "h4", "h5", "h6", "table", "section", "article", "header",
        "footer", "ul", "ol", "blockquote", "pre", "hr", "dt", "dd", "title", "main", "nav", "aside", "figure",
    ];
    let mut out = String::with_capacity(html.len() / 2);
    let mut rest = html;
    let mut skip_until: Option<&str> = None;
    let mut pre = 0usize;
    let push_text = |out: &mut String, text: &str, pre: bool| {
        let text = decode_entities(text);
        if pre {
            out.push_str(&text);
            return;
        }
        for (i, word) in text.split(|c: char| c.is_whitespace() && c != '\u{a0}').enumerate() {
            if i > 0 && !out.ends_with([' ', '\n']) && !out.is_empty() {
                out.push(' ');
            }
            out.push_str(word);
        }
    };
    while !rest.is_empty() {
        if let Some(close) = skip_until {
            match rest.to_ascii_lowercase().find(close) {
                Some(i) => rest = &rest[i + close.len()..],
                None => break,
            }
            skip_until = None;
            continue;
        }
        let Some(lt) = rest.find('<') else {
            push_text(&mut out, rest, pre > 0);
            break;
        };
        push_text(&mut out, &rest[..lt], pre > 0);
        rest = &rest[lt..];
        if let Some(r) = rest.strip_prefix("<!--") {
            rest = r.find("-->").map(|i| &r[i + 3..]).unwrap_or("");
            continue;
        }
        let Some(gt) = rest.find('>') else { break };
        let tag = &rest[1..gt];
        rest = &rest[gt + 1..];
        let closing = tag.starts_with('/');
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        match name.as_str() {
            "script" | "style" | "head" | "template" | "noscript" if !closing && !tag.ends_with('/') => {
                skip_until = Some(match name.as_str() {
                    "script" => "</script>",
                    "style" => "</style>",
                    "head" => "</head>",
                    "template" => "</template>",
                    _ => "</noscript>",
                });
            }
            "td" | "th" if closing => out.push('\t'),
            n if BLOCKS.contains(&n) => {
                if n == "pre" {
                    pre = if closing { pre.saturating_sub(1) } else { pre + 1 };
                }
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                if n == "li" && !closing {
                    out.push_str("- ");
                }
            }
            _ => {}
        }
    }
    out
}

/// Caractère d'un octet en Windows-1252 (code page des RTF français et anglais).
fn cp1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘', '’',
        '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
    ];
    match b {
        0x80..=0x9f => HIGH[(b - 0x80) as usize],
        _ => b as char,
    }
}

/// RTF : extraction simple du texte (groupes de mise en forme, tables de polices, images et
/// métadonnées ignorés ; `\par`, `\tab`, `\'hh`, `\uN` compris).
fn rtf_text(rtf: &str) -> String {
    const SKIP: &[&str] = &[
        "fonttbl", "colortbl", "stylesheet", "info", "pict", "listtable", "listoverridetable", "rsidtbl", "generator",
        "xmlnstbl", "themedata", "colorschememapping", "datastore", "latentstyles", "revtbl", "filetbl", "object",
        "fldinst", "header", "headerl", "headerr", "headerf", "footer", "footerl", "footerr", "footerf", "footnote",
        "operator", "author", "company", "title", "subject", "keywords", "comment", "doccomm", "bkmkstart", "bkmkend",
    ];
    let b = rtf.as_bytes();
    let mut out = String::new();
    // Pile des groupes : (ignoré, nombre de caractères de repli après \uN).
    let mut stack: Vec<(bool, usize)> = vec![];
    let (mut skip, mut uc) = (false, 1usize);
    let mut pending_fallback = 0usize;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        match c {
            b'{' => {
                stack.push((skip, uc));
                i += 1;
            }
            b'}' => {
                if let Some((s, u)) = stack.pop() {
                    skip = s;
                    uc = u;
                }
                pending_fallback = 0;
                i += 1;
            }
            b'\\' => {
                i += 1;
                let Some(&n) = b.get(i) else { break };
                if n.is_ascii_alphabetic() {
                    let start = i;
                    while i < b.len() && b[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    let word = &rtf[start..i];
                    let num_start = i;
                    if i < b.len() && (b[i] == b'-' || b[i].is_ascii_digit()) {
                        i += 1;
                        while i < b.len() && b[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                    let param: Option<i32> = rtf[num_start..i].parse().ok();
                    if i < b.len() && b[i] == b' ' {
                        i += 1;
                    }
                    if SKIP.contains(&word) {
                        skip = true;
                        continue;
                    }
                    if skip {
                        continue;
                    }
                    let text = match word {
                        "par" | "line" | "row" | "sect" | "page" => "\n",
                        "tab" | "cell" => "\t",
                        "emdash" => "—",
                        "endash" => "–",
                        "lquote" => "‘",
                        "rquote" => "’",
                        "ldblquote" => "“",
                        "rdblquote" => "”",
                        "bullet" => "•",
                        "uc" => {
                            uc = param.unwrap_or(1).max(0) as usize;
                            ""
                        }
                        "u" => {
                            if let Some(p) = param {
                                let code = if p < 0 { p + 65536 } else { p } as u32;
                                if let Some(ch) = char::from_u32(code) {
                                    out.push(ch);
                                }
                                pending_fallback = uc;
                            }
                            ""
                        }
                        _ => "",
                    };
                    out.push_str(text);
                } else {
                    i += 1;
                    match n {
                        b'*' => skip = true,
                        b'\'' => {
                            let hex = rtf.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok());
                            i += 2;
                            if let Some(h) = hex {
                                if pending_fallback > 0 {
                                    pending_fallback -= 1;
                                } else if !skip {
                                    out.push(cp1252(h));
                                }
                            }
                        }
                        b'~' if !skip => out.push('\u{a0}'),
                        b'_' if !skip => out.push('‑'),
                        b'{' | b'}' | b'\\' if !skip => out.push(n as char),
                        b'\n' | b'\r' if !skip => out.push('\n'),
                        _ => {}
                    }
                }
            }
            b'\r' | b'\n' => i += 1,
            _ => {
                // Texte brut jusqu'au prochain caractère spécial.
                let start = i;
                while i < b.len() && !matches!(b[i], b'{' | b'}' | b'\\' | b'\r' | b'\n') {
                    i += 1;
                }
                if !skip {
                    let mut chunk = rtf[start..i].chars();
                    while pending_fallback > 0 && chunk.next().is_some() {
                        pending_fallback -= 1;
                    }
                    out.push_str(chunk.as_str());
                }
            }
        }
    }
    out
}

/// Ancien format Word (`.doc`) : `antiword` ou `catdoc`, s'ils sont installés.
fn legacy_doc_text(bytes: &[u8]) -> Result<String> {
    let tools: [(&str, &[&str]); 2] = [("antiword", &["-w", "0"]), ("catdoc", &["-w"])];
    let Some((tool, args)) = tools.iter().find_map(|(t, a)| which::which(t).ok().map(|p| (p, *a))) else {
        bail!("document Word .doc illisible : installez antiword ou catdoc, ou enregistrez-le en .docx");
    };
    let dir = std::env::temp_dir().join(format!("steno-ctx-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir)?;
    let src = dir.join("source.doc");
    std::fs::write(&src, bytes)?;
    let out = std::process::Command::new(&tool).args(args).arg(&src).output();
    let _ = std::fs::remove_dir_all(&dir);
    let out = out.with_context(|| format!("impossible de lancer {}", tool.display()))?;
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
        "document" => document_text(name, bytes)?,
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
        let (k, t, _) = extract("note.html", b"<html><body><h1>TVA</h1><p>Belgique 21 %</p></body></html>").unwrap();
        assert_eq!(k, "document");
        assert!(t.contains("Belgique 21 %"), "{t}");
    }

    /// Archive zip en mémoire (docx, odt).
    fn zip_of(files: &[(&str, &str)]) -> Vec<u8> {
        use std::io::Write;
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, body) in files {
            w.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(body.as_bytes()).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn html_sans_balises_ni_scripts() {
        let t = html_text(
            "<html><head><title>x</title><style>p{color:red}</style></head><body>\n<h1>Comité&nbsp;de   pilotage</h1>\
             <script>alert('non')</script><p>TVA &amp; taux : 21&#160;% &eacute;t&#xE9;</p><!-- caché --><ul><li>Un</li><li>Deux</li></ul>\
             <table><tr><td>A</td><td>B</td></tr></table></body></html>",
        );
        let t = tidy(&t);
        assert!(t.contains("Comité\u{a0}de pilotage"), "{t}");
        assert!(t.contains("TVA & taux : 21\u{a0}% été"), "{t}");
        assert!(t.contains("- Un\n- Deux"), "{t}");
        assert!(t.contains("A\tB"), "{t}");
        assert!(!t.contains("alert") && !t.contains("color") && !t.contains("caché") && t.starts_with("Comité"), "{t}");
        assert_eq!(decode_entities("a &inconnu; b & c &#65;"), "a &inconnu; b & c A");
    }

    #[test]
    fn word_docx() {
        let doc = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:t>Compte rendu</w:t></w:r></w:p>
<w:p><w:r><w:t xml:space="preserve">TVA &amp; </w:t></w:r><w:r><w:t>Belgique</w:t><w:tab/><w:t>21 %</w:t></w:r></w:p>
<w:p><w:del><w:r><w:delText>supprimé</w:delText></w:r></w:del><w:r><w:instrText>PAGE</w:instrText><w:t>Fin</w:t></w:r></w:p>
</w:body></w:document>"#;
        let bytes = zip_of(&[("[Content_Types].xml", "<Types/>"), ("word/document.xml", doc)]);
        let (k, t, _) = extract("note.DOCX", &bytes).unwrap();
        assert_eq!(k, "document");
        assert_eq!(t, "Compte rendu\nTVA & Belgique\t21 %\nFin");
        assert!(extract("faux.docx", b"pas un zip").is_err());
        assert!(extract("vide.docx", &zip_of(&[("autre.xml", "<a/>")])).is_err());
    }

    #[test]
    fn opendocument_odt() {
        let content = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
<office:automatic-styles><style:style xmlns:style="s">ignoré</style:style></office:automatic-styles>
<office:body><office:text>
<text:h text:outline-level="1">Décisions</text:h>
<text:p>Export<text:s text:c="2"/>validé<text:tab/>vendredi<text:line-break/>Suite</text:p>
<text:p><text:span>Taux &lt; 20 %</text:span></text:p>
</office:text></office:body></office:document-content>"#;
        let bytes = zip_of(&[("mimetype", "application/vnd.oasis.opendocument.text"), ("content.xml", content)]);
        let (k, t, _) = extract("note.odt", &bytes).unwrap();
        assert_eq!(k, "document");
        assert_eq!(t, "Décisions\nExport  validé\tvendredi\nSuite\nTaux < 20 %");
    }

    #[test]
    fn rtf_simple() {
        let rtf = r"{\rtf1\ansi\ansicpg1252\deff0{\fonttbl{\f0\fswiss Helvetica;}}{\colortbl;\red255\green0\blue0;}
{\*\generator Riched20;}{\info{\title Titre cach\'e9}}\pard\f0\fs24 Comit\'e9 de pilotage\par
TVA \{21 %\}\tab Belgique\line \uc1\u8364?\~50\par
{\b Gras}\rquote ok\par}";
        let (k, t, _) = extract("note.rtf", rtf.as_bytes()).unwrap();
        assert_eq!(k, "document");
        assert_eq!(t, "Comité de pilotage\nTVA {21 %}\tBelgique\n€\u{a0}50\nGras’ok");
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn ancien_word_sans_outil() {
        // Sans antiword ni catdoc dans le PATH : erreur claire.
        if which::which("antiword").is_err() && which::which("catdoc").is_err() {
            let e = extract("vieux.doc", b"\xd0\xcf\x11\xe0").unwrap_err().to_string();
            assert!(e.contains("antiword"), "{e}");
        }
    }
}
