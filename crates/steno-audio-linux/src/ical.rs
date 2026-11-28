//! Lecture d'iCalendar (RFC 5545) : événements, participants, fuseaux, récurrences.
//!
//! Volontairement limité à ce dont Sténo a besoin : `VEVENT` avec `DTSTART`/`DTEND`/`DURATION`,
//! `RRULE` (FREQ DAILY/WEEKLY/MONTHLY/YEARLY, INTERVAL, COUNT, UNTIL, BYDAY, BYMONTHDAY),
//! `EXDATE`, `RECURRENCE-ID` (occurrence modifiée ou annulée), participants et organisateur.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Weekday};

#[derive(Debug, Clone, PartialEq)]
pub struct Prop {
    pub name: String,
    pub params: Vec<(String, String)>,
    pub value: String,
}

impl Prop {
    pub fn param(&self, k: &str) -> Option<&str> {
        self.params.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, v)| v.as_str())
    }
}

/// Lignes dépliées (une ligne commençant par un espace ou une tabulation continue la précédente).
pub fn unfold(text: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if (line.starts_with(' ') || line.starts_with('\t')) && !out.is_empty() {
            out.last_mut().unwrap().push_str(&line[1..]);
        } else if !line.is_empty() {
            out.push(line.to_string());
        }
    }
    out
}

pub fn parse_prop(line: &str) -> Option<Prop> {
    // Le ':' qui sépare la valeur est le premier hors guillemets.
    let mut in_quotes = false;
    let mut split = None;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            ':' if !in_quotes => {
                split = Some(i);
                break;
            }
            _ => {}
        }
    }
    let split = split?;
    let (head, value) = (&line[..split], &line[split + 1..]);
    let mut parts = vec![];
    let mut cur = String::new();
    in_quotes = false;
    for c in head.chars() {
        match c {
            '"' => in_quotes = !in_quotes,
            ';' if !in_quotes => parts.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    parts.push(cur);
    let name = parts.remove(0).to_ascii_uppercase();
    let params = parts.into_iter().filter_map(|p| p.split_once('=').map(|(k, v)| (k.to_ascii_uppercase(), v.to_string()))).collect();
    Some(Prop { name, params, value: value.to_string() })
}

/// Texte iCalendar : `\n`, `\,`, `\;`, `\\`.
pub fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') | Some('N') => out.push('\n'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Composants `VEVENT` (propriétés à plat ; les `VALARM` imbriquées sont ignorées).
pub fn vevents(text: &str) -> Vec<Vec<Prop>> {
    let mut out = vec![];
    let mut cur: Option<Vec<Prop>> = None;
    let mut depth_skip = 0;
    for line in unfold(text) {
        let up = line.to_ascii_uppercase();
        if up == "BEGIN:VEVENT" {
            cur = Some(vec![]);
            continue;
        }
        if up == "END:VEVENT" {
            if let Some(c) = cur.take() {
                out.push(c);
            }
            continue;
        }
        if cur.is_some() && up.starts_with("BEGIN:") {
            depth_skip += 1;
            continue;
        }
        if cur.is_some() && up.starts_with("END:") {
            depth_skip -= 1;
            continue;
        }
        if depth_skip == 0 {
            if let (Some(c), Some(p)) = (cur.as_mut(), parse_prop(&line)) {
                c.push(p);
            }
        }
    }
    out
}

// MARK: - Dates

/// Fuseaux Windows (Outlook / Exchange) courants → IANA.
fn windows_zone(name: &str) -> Option<&'static str> {
    Some(match name {
        "Romance Standard Time" => "Europe/Paris",
        "W. Europe Standard Time" => "Europe/Berlin",
        "Central Europe Standard Time" => "Europe/Budapest",
        "Central European Standard Time" => "Europe/Warsaw",
        "GMT Standard Time" => "Europe/London",
        "Greenwich Standard Time" => "Atlantic/Reykjavik",
        "E. Europe Standard Time" => "Europe/Chisinau",
        "FLE Standard Time" => "Europe/Kiev",
        "GTB Standard Time" => "Europe/Bucharest",
        "Russian Standard Time" => "Europe/Moscow",
        "Eastern Standard Time" => "America/New_York",
        "Central Standard Time" => "America/Chicago",
        "Mountain Standard Time" => "America/Denver",
        "Pacific Standard Time" => "America/Los_Angeles",
        "Atlantic Standard Time" => "America/Halifax",
        "Canada Central Standard Time" => "America/Regina",
        "Morocco Standard Time" => "Africa/Casablanca",
        "W. Central Africa Standard Time" => "Africa/Lagos",
        "South Africa Standard Time" => "Africa/Johannesburg",
        "India Standard Time" => "Asia/Kolkata",
        "China Standard Time" => "Asia/Shanghai",
        "Tokyo Standard Time" => "Asia/Tokyo",
        "Singapore Standard Time" => "Asia/Singapore",
        "AUS Eastern Standard Time" => "Australia/Sydney",
        "UTC" | "Coordinated Universal Time" => "UTC",
        _ => return None,
    })
}

fn resolve_tz(tzid: &str) -> Option<chrono_tz::Tz> {
    let t = tzid.trim().trim_matches('"');
    // Certains clients préfixent l'identifiant (« /freeassociation.sourceforge.net/Europe/Paris »).
    let candidates = [t, t.rsplit_once("/Tzfile/").map(|x| x.1).unwrap_or(t), windows_zone(t).unwrap_or("")];
    for c in candidates {
        if let Ok(z) = c.parse::<chrono_tz::Tz>() {
            return Some(z);
        }
        // Dernières composantes « Zone/Ville ».
        let parts: Vec<&str> = c.split('/').filter(|p| !p.is_empty()).collect();
        if parts.len() >= 2 {
            if let Ok(z) = format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1]).parse::<chrono_tz::Tz>() {
                return Some(z);
            }
        }
    }
    None
}

/// Instant d'une date iCalendar, fuseau compris. Le second élément vaut `true` pour une date seule (journée entière).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Zone {
    Utc,
    Floating,
    Tz(chrono_tz::Tz),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IcalTime {
    pub naive: NaiveDateTime,
    pub zone: Zone,
    pub date_only: bool,
}

impl IcalTime {
    pub fn to_local(self) -> Option<DateTime<Local>> {
        localize(self.naive, self.zone)
    }
}

fn localize(naive: NaiveDateTime, zone: Zone) -> Option<DateTime<Local>> {
    Some(match zone {
        Zone::Utc => chrono::Utc.from_utc_datetime(&naive).with_timezone(&Local),
        Zone::Floating => Local.from_local_datetime(&naive).earliest()?,
        Zone::Tz(z) => z.from_local_datetime(&naive).earliest()?.with_timezone(&Local),
    })
}

pub fn parse_time(value: &str, tzid: Option<&str>) -> Option<IcalTime> {
    let v = value.trim();
    if v.len() == 8 {
        let d = NaiveDate::parse_from_str(v, "%Y%m%d").ok()?;
        return Some(IcalTime { naive: d.and_time(NaiveTime::MIN), zone: Zone::Floating, date_only: true });
    }
    let (body, utc) = match v.strip_suffix('Z') {
        Some(b) => (b, true),
        None => (v, false),
    };
    let naive = NaiveDateTime::parse_from_str(body, "%Y%m%dT%H%M%S").ok()?;
    let zone = if utc {
        Zone::Utc
    } else {
        tzid.and_then(resolve_tz).map(Zone::Tz).unwrap_or(Zone::Floating)
    };
    Some(IcalTime { naive, zone, date_only: false })
}

fn prop_time(p: &Prop) -> Option<IcalTime> {
    parse_time(&p.value, p.param("TZID"))
}

/// `PT1H30M`, `P1D`, `-PT15M`…
pub fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim();
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let s = s.strip_prefix('P')?;
    let mut total = 0i64;
    let mut num = String::new();
    let mut in_time = false;
    for c in s.chars() {
        match c {
            'T' => in_time = true,
            '0'..='9' => num.push(c),
            _ => {
                let n: i64 = num.parse().ok()?;
                num.clear();
                total += n * match (c, in_time) {
                    ('W', _) => 7 * 86_400,
                    ('D', _) => 86_400,
                    ('H', true) => 3_600,
                    ('M', true) => 60,
                    ('S', true) => 1,
                    _ => return None,
                };
            }
        }
    }
    Some(Duration::seconds(if neg { -total } else { total }))
}

// MARK: - Récurrence

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RRule {
    pub freq: String,
    pub interval: i64,
    pub count: Option<usize>,
    pub until: Option<IcalTime>,
    /// (rang facultatif, jour) : `MO`, `1MO`, `-1FR`.
    pub by_day: Vec<(Option<i32>, Weekday)>,
    pub by_month_day: Vec<i32>,
}

fn weekday(s: &str) -> Option<Weekday> {
    Some(match s {
        "MO" => Weekday::Mon,
        "TU" => Weekday::Tue,
        "WE" => Weekday::Wed,
        "TH" => Weekday::Thu,
        "FR" => Weekday::Fri,
        "SA" => Weekday::Sat,
        "SU" => Weekday::Sun,
        _ => return None,
    })
}

pub fn parse_rrule(s: &str) -> Option<RRule> {
    let mut r = RRule { interval: 1, ..Default::default() };
    for part in s.split(';') {
        let (k, v) = part.split_once('=')?;
        match k.to_ascii_uppercase().as_str() {
            "FREQ" => r.freq = v.to_ascii_uppercase(),
            "INTERVAL" => r.interval = v.parse().ok().filter(|n: &i64| *n > 0).unwrap_or(1),
            "COUNT" => r.count = v.parse().ok(),
            "UNTIL" => r.until = parse_time(v, None),
            "BYDAY" => {
                for d in v.split(',') {
                    let d = d.trim().to_ascii_uppercase();
                    let (rank, day) = d.split_at(d.len().saturating_sub(2));
                    r.by_day.push((rank.parse().ok(), weekday(day)?));
                }
            }
            "BYMONTHDAY" => r.by_month_day = v.split(',').filter_map(|x| x.trim().parse().ok()).collect(),
            _ => {}
        }
    }
    (!r.freq.is_empty()).then_some(r)
}

fn add_months(d: NaiveDate, months: i64) -> Option<NaiveDate> {
    let total = d.year() as i64 * 12 + d.month0() as i64 + months;
    NaiveDate::from_ymd_opt((total / 12) as i32, (total % 12) as u32 + 1, 1)
}

fn days_in_month(first: NaiveDate) -> u32 {
    let next = add_months(first, 1).unwrap();
    (next - first).num_days() as u32
}

/// Jours d'un mois correspondant à BYDAY (avec rang) et BYMONTHDAY.
fn month_days(first: NaiveDate, rule: &RRule, default_day: u32) -> Vec<NaiveDate> {
    let n = days_in_month(first);
    let mut days: Vec<NaiveDate> = vec![];
    if !rule.by_month_day.is_empty() {
        for &md in &rule.by_month_day {
            let d = if md > 0 { md as u32 } else { (n as i32 + md + 1).max(1) as u32 };
            if d <= n {
                days.push(first.with_day(d).unwrap());
            }
        }
    } else if !rule.by_day.is_empty() {
        for &(rank, wd) in &rule.by_day {
            let all: Vec<NaiveDate> = (1..=n).map(|d| first.with_day(d).unwrap()).filter(|d| d.weekday() == wd).collect();
            match rank {
                None => days.extend(all),
                Some(r) if r > 0 => days.extend(all.get(r as usize - 1)),
                Some(r) if r < 0 => days.extend(all.len().checked_sub((-r) as usize).and_then(|i| all.get(i))),
                _ => {}
            }
        }
    } else if default_day <= n {
        days.push(first.with_day(default_day).unwrap());
    }
    days.sort();
    days.dedup();
    days
}

/// Débuts des occurrences (heure locale du fuseau de l'événement), dans l'ordre, jusqu'à `until_naive`.
pub fn occurrences(start: NaiveDateTime, rule: &RRule, horizon: NaiveDateTime, max: usize) -> Vec<NaiveDateTime> {
    let time = start.time();
    let date = start.date();
    let until = rule.until.map(|u| u.naive);
    let mut out = vec![];
    let mut emitted = 0usize;
    let mut push = |d: NaiveDate, out: &mut Vec<NaiveDateTime>| -> bool {
        let t = d.and_time(time);
        if t < start {
            return true;
        }
        if until.map(|u| t > u).unwrap_or(false) || t > horizon || rule.count.map(|c| emitted >= c).unwrap_or(false) || out.len() >= max {
            return false;
        }
        emitted += 1;
        out.push(t);
        true
    };
    match rule.freq.as_str() {
        "DAILY" => {
            let mut d = date;
            loop {
                let ok_day = rule.by_day.is_empty() || rule.by_day.iter().any(|(_, w)| *w == d.weekday());
                if ok_day && !push(d, &mut out) {
                    break;
                }
                d += Duration::days(rule.interval);
            }
        }
        "WEEKLY" => {
            let days: Vec<Weekday> = if rule.by_day.is_empty() { vec![date.weekday()] } else { rule.by_day.iter().map(|(_, w)| *w).collect() };
            // Semaines commençant le lundi (WKST=MO, valeur par défaut).
            let mut week = date - Duration::days(date.weekday().num_days_from_monday() as i64);
            'outer: loop {
                let mut in_week: Vec<NaiveDate> = days.iter().map(|w| week + Duration::days(w.num_days_from_monday() as i64)).collect();
                in_week.sort();
                for d in in_week {
                    if !push(d, &mut out) {
                        break 'outer;
                    }
                }
                week += Duration::weeks(rule.interval);
                if week.and_time(time) > horizon + Duration::weeks(1) {
                    break;
                }
            }
        }
        "MONTHLY" | "YEARLY" => {
            let step = if rule.freq == "MONTHLY" { rule.interval } else { 12 * rule.interval };
            let mut first = date.with_day(1).unwrap();
            'months: for _ in 0..2_400 {
                for d in month_days(first, rule, date.day()) {
                    if !push(d, &mut out) {
                        break 'months;
                    }
                }
                match add_months(first, step) {
                    Some(f) if f.and_time(time) <= horizon + Duration::days(31) => first = f,
                    _ => break,
                }
            }
        }
        _ => {
            push(date, &mut out);
        }
    }
    out
}

// MARK: - Événements

#[derive(Debug, Clone, PartialEq)]
pub struct Attendee {
    pub name: Option<String>,
    pub email: Option<String>,
    pub status: String,
    pub organizer: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Occurrence {
    pub uid: String,
    pub summary: String,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    pub all_day: bool,
    pub location: Option<String>,
    pub url: Option<String>,
    pub description: Option<String>,
    pub attendees: Vec<Attendee>,
}

fn mailto(v: &str) -> Option<String> {
    let lower = v.to_ascii_lowercase();
    let e = lower.strip_prefix("mailto:").map(|_| &v[7..]).unwrap_or(v).trim();
    e.contains('@').then(|| e.to_string())
}

fn status(s: Option<&str>) -> String {
    match s.map(|x| x.to_ascii_uppercase()).as_deref() {
        Some("ACCEPTED") => "accepted",
        Some("DECLINED") => "declined",
        Some("TENTATIVE") => "tentative",
        Some("NEEDS-ACTION") => "pending",
        _ => "unknown",
    }
    .into()
}

fn get<'a>(ev: &'a [Prop], name: &str) -> Option<&'a Prop> {
    ev.iter().find(|p| p.name == name)
}

fn text(ev: &[Prop], name: &str) -> Option<String> {
    get(ev, name).map(|p| unescape(&p.value).trim().to_string()).filter(|s| !s.is_empty())
}

fn attendees(ev: &[Prop]) -> Vec<Attendee> {
    let org = get(ev, "ORGANIZER");
    let org_email = org.and_then(|o| mailto(&o.value));
    let mut out: Vec<Attendee> = ev
        .iter()
        .filter(|p| p.name == "ATTENDEE")
        .map(|p| {
            let email = mailto(&p.value);
            Attendee {
                name: p.param("CN").map(|c| c.trim().to_string()).filter(|c| !c.is_empty()),
                organizer: email.is_some() && email == org_email,
                email,
                status: status(p.param("PARTSTAT")),
            }
        })
        .collect();
    // L'organisateur n'est pas toujours listé parmi les participants.
    if let (Some(o), Some(e)) = (org, &org_email) {
        if !out.iter().any(|a| a.email.as_ref() == Some(e)) {
            out.insert(0, Attendee { name: o.param("CN").map(String::from), email: Some(e.clone()), status: "accepted".into(), organizer: true });
        }
    }
    out
}

fn occurrence_of(ev: &[Prop], start: DateTime<Local>, end: DateTime<Local>, all_day: bool) -> Occurrence {
    Occurrence {
        uid: text(ev, "UID").unwrap_or_default(),
        summary: text(ev, "SUMMARY").unwrap_or_default(),
        start,
        end,
        all_day,
        location: text(ev, "LOCATION"),
        url: text(ev, "URL"),
        description: text(ev, "DESCRIPTION").map(|d| d.chars().take(4000).collect()),
        attendees: attendees(ev),
    }
}

/// Occurrences des événements de `components` qui chevauchent [from, to].
pub fn expand(components: &[Vec<Prop>], from: DateTime<Local>, to: DateTime<Local>) -> Vec<Occurrence> {
    // Occurrences modifiées : (UID, début d'origine) → composant de remplacement.
    let mut overrides: std::collections::HashMap<(String, NaiveDateTime), &Vec<Prop>> = Default::default();
    for ev in components {
        if let (Some(uid), Some(rid)) = (text(ev, "UID"), get(ev, "RECURRENCE-ID").and_then(prop_time)) {
            overrides.insert((uid, rid.to_local().map(|l| l.naive_local()).unwrap_or(rid.naive)), ev);
        }
    }
    let mut out = vec![];
    for ev in components {
        if get(ev, "RECURRENCE-ID").is_some() {
            continue; // traité avec son événement principal
        }
        let Some(dtstart) = get(ev, "DTSTART").and_then(prop_time) else { continue };
        let duration = match (get(ev, "DTEND").and_then(prop_time), get(ev, "DURATION").and_then(|d| parse_duration(&d.value))) {
            (Some(end), _) => end.naive - dtstart.naive,
            (None, Some(d)) => d,
            (None, None) if dtstart.date_only => Duration::days(1),
            _ => Duration::zero(),
        };
        let uid = text(ev, "UID").unwrap_or_default();
        let cancelled = |e: &[Prop]| text(e, "STATUS").map(|s| s.eq_ignore_ascii_case("CANCELLED")).unwrap_or(false);
        let exdates: Vec<NaiveDateTime> = ev
            .iter()
            .filter(|p| p.name == "EXDATE")
            .flat_map(|p| p.value.split(',').filter_map(|v| parse_time(v, p.param("TZID"))).collect::<Vec<_>>())
            .filter_map(|t| t.to_local().map(|l| l.naive_local()))
            .collect();
        let starts: Vec<NaiveDateTime> = match get(ev, "RRULE").and_then(|r| parse_rrule(&r.value)) {
            Some(rule) => {
                // Horizon dans l'heure du fuseau de l'événement (large d'un jour pour les décalages).
                let horizon = to.naive_local() + Duration::days(1);
                occurrences(dtstart.naive, &rule, horizon, 5_000)
            }
            None => vec![dtstart.naive],
        };
        for s in starts {
            let Some(start) = localize(s, dtstart.zone) else { continue };
            let key = (uid.clone(), start.naive_local());
            if exdates.contains(&start.naive_local()) {
                continue;
            }
            let occ = match overrides.get(&key) {
                Some(ov) => {
                    if cancelled(ov) {
                        continue;
                    }
                    let os = get(ov, "DTSTART").and_then(prop_time).and_then(|t| t.to_local()).unwrap_or(start);
                    let oe = get(ov, "DTEND").and_then(prop_time).and_then(|t| t.to_local()).unwrap_or(os + duration);
                    occurrence_of(ov, os, oe, dtstart.date_only)
                }
                None => {
                    if cancelled(ev) {
                        continue;
                    }
                    occurrence_of(ev, start, start + duration, dtstart.date_only)
                }
            };
            if occ.end > from && occ.start < to {
                out.push(occ);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICS: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:daily-1\r\nSUMMARY:Daily ORION\r\nDTSTART;TZID=Europe/Paris:20261123T093000\r\nDTEND;TZID=Europe/Paris:20261123T094500\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR\r\nEXDATE;TZID=Europe/Paris:20261125T093000\r\nORGANIZER;CN=Inès:mailto:ines@exemple.fr\r\nATTENDEE;CN=\"Martin, Paul\";PARTSTAT=ACCEPTED:mailto:paul@exemple.fr\r\nATTENDEE;PARTSTAT=TENTATIVE:mailto:moi@exemple.fr\r\nDESCRIPTION:Lien : https://meet\r\n .exemple.fr/abc\\, salle 2\r\nBEGIN:VALARM\r\nACTION:DISPLAY\r\nDESCRIPTION:rappel\r\nEND:VALARM\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:daily-1\r\nRECURRENCE-ID;TZID=Europe/Paris:20261126T093000\r\nDTSTART;TZID=Europe/Paris:20261126T110000\r\nDTEND;TZID=Europe/Paris:20261126T111500\r\nSUMMARY:Daily ORION (décalée)\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:conge\r\nSUMMARY:Congé\r\nDTSTART;VALUE=DATE:20261124\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:revue\r\nSUMMARY:Revue Nova\r\nDTSTART:20261124T130000Z\r\nDURATION:PT1H\r\nSTATUS:CANCELLED\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    fn local(s: &str) -> DateTime<Local> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Local)
    }

    #[test]
    fn analyse_des_proprietes() {
        let p = parse_prop("ATTENDEE;CN=\"Martin, Paul\";PARTSTAT=ACCEPTED:mailto:paul@exemple.fr").unwrap();
        assert_eq!(p.param("cn"), Some("Martin, Paul"));
        assert_eq!(p.value, "mailto:paul@exemple.fr");
        assert_eq!(unescape("a\\, b\\nc"), "a, b\nc");
        assert_eq!(parse_duration("PT1H30M"), Some(Duration::minutes(90)));
        assert_eq!(parse_duration("-P1D"), Some(Duration::days(-1)));
        assert!(resolve_tz("Romance Standard Time").is_some());
        assert!(resolve_tz("/freeassociation.sourceforge.net/Tzfile/Europe/Paris").is_some());
    }

    #[test]
    fn recurrences_exceptions_et_annulations() {
        let comps = vevents(ICS);
        assert_eq!(comps.len(), 4);
        let occ = expand(&comps, local("2026-11-23T00:00:00+01:00"), local("2026-11-28T00:00:00+01:00"));
        let titles: Vec<(String, String)> = occ.iter().map(|o| (o.summary.clone(), o.start.with_timezone(&chrono_tz::Europe::Paris).format("%d %H:%M").to_string())).collect();
        // Lundi, mardi, (mercredi exclu), jeudi décalé, vendredi ; congé (journée entière) ; revue annulée.
        assert!(titles.contains(&("Daily ORION".into(), "23 09:30".into())));
        assert!(titles.contains(&("Daily ORION".into(), "24 09:30".into())));
        assert!(!titles.iter().any(|t| t.1 == "25 09:30"));
        assert!(titles.contains(&("Daily ORION (décalée)".into(), "26 11:00".into())));
        assert!(titles.contains(&("Daily ORION".into(), "27 09:30".into())));
        assert!(occ.iter().any(|o| o.summary == "Congé" && o.all_day));
        assert!(!occ.iter().any(|o| o.summary == "Revue Nova"));
        let d = occ.iter().find(|o| o.summary == "Daily ORION").unwrap();
        assert_eq!(d.description.as_deref(), Some("Lien : https://meet.exemple.fr/abc, salle 2"));
        assert_eq!((d.end - d.start).num_minutes(), 15);
        assert_eq!(d.attendees.len(), 3);
        assert!(d.attendees[0].organizer && d.attendees[0].name.as_deref() == Some("Inès"));
        assert_eq!(d.attendees[1].status, "accepted");
        assert_eq!(d.attendees[2].status, "tentative");
    }

    #[test]
    fn regles_mensuelles() {
        let start = NaiveDate::from_ymd_opt(2026, 1, 30).unwrap().and_hms_opt(14, 0, 0).unwrap();
        let horizon = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap().and_hms_opt(0, 0, 0).unwrap();
        // Dernier vendredi du mois.
        let r = parse_rrule("FREQ=MONTHLY;BYDAY=-1FR").unwrap();
        let o = occurrences(start, &r, horizon, 100);
        assert_eq!(o.iter().map(|d| d.format("%m-%d").to_string()).collect::<Vec<_>>(), vec!["01-30", "02-27", "03-27", "04-24", "05-29"]);
        // Le 31 : les mois courts sont sautés.
        let r = parse_rrule("FREQ=MONTHLY;BYMONTHDAY=31;COUNT=3").unwrap();
        let s31 = NaiveDate::from_ymd_opt(2026, 1, 31).unwrap().and_hms_opt(9, 0, 0).unwrap();
        assert_eq!(occurrences(s31, &r, horizon, 100).len(), 3);
        // Quotidien tous les 2 jours jusqu'à une date.
        let r = parse_rrule("FREQ=DAILY;INTERVAL=2;UNTIL=20260205T000000Z").unwrap();
        assert_eq!(occurrences(start, &r, horizon, 100).len(), 3);
    }
}
