//! Tests fonctionnels : le binaire `steno` lancé en sous-processus, comme par un dev.
//! Un seul dossier de données est préparé par `steno simulate` (daily de 45 min, faux agent).

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn steno(home: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_steno"));
    let path = format!("{}:{}", root().join("fixtures/fake-bin").display(), std::env::var("PATH").unwrap_or_default());
    c.env("STENO_HOME", home).env("PATH", path).env_remove("ANTHROPIC_API_KEY").env("RUST_LOG", "warn");
    c
}

/// Dossier de données avec la daily rejouée (préparé une seule fois).
fn home() -> &'static PathBuf {
    static H: OnceLock<PathBuf> = OnceLock::new();
    H.get_or_init(|| {
        let h = std::env::temp_dir().join(format!("steno-cli-tests-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&h);
        std::fs::create_dir_all(&h).unwrap();
        let out = steno(&h)
            .args(["simulate", root().join("fixtures/daily-45min.jsonl").to_str().unwrap(), "--timeout", "120"])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "simulate : {}", String::from_utf8_lossy(&out.stderr));
        assert!(stdout.contains("## Décisions"), "{stdout}");
        assert!(stdout.contains("Marie"), "{stdout}");
        h
    })
}

fn meeting_id(h: &Path) -> String {
    let out = steno(h).args(["meetings"]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    s.lines().next().unwrap().split_whitespace().last().unwrap().to_string()
}

#[test]
fn simulate_puis_liste() {
    let h = home();
    let out = steno(h).args(["meetings", "--query", "Brindille"]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("Daily ORION"), "{s}");
}

#[test]
fn serveur_mcp_lecture_seule() {
    let h = home();
    let id = meeting_id(h);
    let mut child = steno(h).arg("mcp").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut call = |msg: serde_json::Value| -> serde_json::Value {
        writeln!(stdin, "{msg}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        out.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    };
    let init = call(serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}));
    assert_eq!(init["result"]["serverInfo"]["name"], "steno");
    let tools = call(serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}));
    let names: Vec<&str> = tools["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["search_meetings", "get_meeting", "get_transcript", "get_report", "list_decisions", "list_actions", "list_report_templates"]);
    let r = call(serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search_meetings","arguments":{"query":"Brindille"}}}));
    assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("Daily ORION"));
    let r = call(serde_json::json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"get_transcript","arguments":{"id": id}}}));
    let t = r["result"]["content"][0]["text"].as_str().unwrap();
    assert!(t.contains("verrou de créneau"));
    assert!(!t.contains("entretien annuel"), "les apartés ne sortent jamais");
    let r = call(serde_json::json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"list_actions","arguments":{"owner":"Marie"}}}));
    assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("APP-1234"));
    drop(stdin);
    let _ = child.wait();
}

#[test]
fn export_markdown_sans_apartes() {
    let h = home();
    let dir = h.join("export");
    let out = steno(h).args(["export", "--all", "--out", dir.to_str().unwrap()]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
    assert_eq!(files.len(), 1);
    let md = std::fs::read_to_string(files[0].path()).unwrap();
    assert!(md.starts_with("# Daily ORION"));
    assert!(md.contains("## Transcription"));
    assert!(!md.contains("entretien annuel"));
}

#[test]
fn export_transcript_formats_without_asides() {
    let h = home();
    let id = meeting_id(h);
    let run = |format: &str| {
        let out = steno(h).args(["export", &id, "--format", format]).output().unwrap();
        assert!(out.status.success(), "{format}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).to_string()
    };
    let srt = run("srt");
    assert!(srt.starts_with("1\n00:"), "{srt}");
    assert!(srt.contains(" --> "));
    let vtt = run("vtt");
    assert!(vtt.starts_with("WEBVTT\n"), "{vtt}");
    let txt = run("txt");
    assert!(txt.starts_with("Daily ORION"), "{txt}");
    for s in [&srt, &vtt, &txt] {
        assert!(s.contains("verrou de créneau"));
        assert!(!s.contains("entretien annuel"), "asides are never exported");
    }
    assert_eq!(run("markdown"), run("md"));

    let dir = h.join("export-vtt");
    let out = steno(h).args(["export", "--all", "--format", "vtt", "--out", dir.to_str().unwrap()]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|f| f.path()).collect();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].extension().unwrap(), "vtt");

    let out = steno(h).args(["export", &id, "--format", "pdf"]).output().unwrap();
    assert!(!out.status.success());
}

#[test]
fn ask_historique_sans_ia_puis_avec() {
    let h = home();
    let out = steno(h).args(["ask", "--no-ai", "verrou de créneau"]).output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Daily ORION"), "{err}");
    let out = steno(h).args(["ask", "qu'a-t-on décidé sur le verrou de créneau ?"]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("verrou de créneau"));
}

#[test]
fn crochet_de_politique() {
    let h = home();
    let dir = h.join("pol");
    std::fs::create_dir_all(&dir).unwrap();
    let pol = serde_json::json!({
        "base": toml_policy(),
        "readable_dirs": [dir], "tools_enabled": true, "allowed_mcp_servers": ["slack", "atlassian"],
        "max_tool_calls": 0, "counter_file": null
    });
    let pf = dir.join("p.json");
    std::fs::write(&pf, pol.to_string()).unwrap();
    let run = |call: serde_json::Value| -> (i32, String) {
        let mut c = steno(h).args(["policy-hook", pf.to_str().unwrap()]).stdin(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        c.stdin.take().unwrap().write_all(call.to_string().as_bytes()).unwrap();
        let mut err = String::new();
        c.stderr.take().unwrap().read_to_string(&mut err).unwrap();
        (c.wait().unwrap().code().unwrap(), err)
    };
    let cwd = dir.display().to_string();
    assert_eq!(run(serde_json::json!({"tool_name":"Bash","tool_input":{"command":"rg -n creneau"},"cwd":cwd})).0, 0);
    assert_eq!(run(serde_json::json!({"tool_name":"mcp__atlassian__getJiraIssue","tool_input":{},"cwd":cwd})).0, 0);
    let (code, err) = run(serde_json::json!({"tool_name":"mcp__slack__slack_send_message","tool_input":{},"cwd":cwd}));
    assert_eq!(code, 2);
    assert!(err.contains("Refusé par Sténo"));
    assert_eq!(run(serde_json::json!({"tool_name":"Bash","tool_input":{"command":"git push"},"cwd":cwd})).0, 2);
    assert_eq!(run(serde_json::json!({"tool_name":"Write","tool_input":{"file_path":"/tmp/x"},"cwd":cwd})).0, 2);
}

fn toml_policy() -> serde_json::Value {
    let t: toml::Value = toml::from_str(&std::fs::read_to_string(root().join("agent/policy.toml")).unwrap()).unwrap();
    serde_json::to_value(t).unwrap()
}

#[test]
fn reglages_d_equipe_sans_donnees_perso() {
    let h = home();
    let out = steno(h).args(["settings", "export"]).output().unwrap();
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("wake_word = \"Sténo\""));
    assert!(!t.contains(h.to_str().unwrap()));
    let f = h.join("equipe.toml");
    std::fs::write(&f, "[voice]\nwake_word = \"Greffier\"\n[audio]\nvocabulary = [\"ms-booking\"]\n").unwrap();
    // Dossier séparé pour ne pas changer le mot d'appel des autres tests.
    let h2 = h.join("autre");
    std::fs::create_dir_all(&h2).unwrap();
    assert!(steno(&h2).args(["settings", "import", f.to_str().unwrap()]).status().unwrap().success());
    let s = String::from_utf8_lossy(&steno(&h2).args(["settings", "show"]).output().unwrap().stdout).to_string();
    assert!(s.contains("Greffier") && s.contains("ms-booking"));
}

/// Requête HTTP minimale (pas de dépendance) sur 127.0.0.1.
fn http(port: u16, method: &str, path: &str, body: &str) -> (u16, String) {
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    write!(
        s,
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut resp = String::new();
    s.read_to_string(&mut resp).unwrap();
    let code = resp[9..12].parse().unwrap();
    let body = resp.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (code, body)
}

/// Port announced by `steno serve` on stderr ("… http://127.0.0.1:<port>/api …").
/// The rest of stderr keeps being drained so the server never blocks on a full pipe.
fn served_port(stderr: std::process::ChildStderr) -> u16 {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut tx = Some(tx);
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let port = line
                .split("http://127.0.0.1:")
                .nth(1)
                .and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).next()?.parse::<u16>().ok());
            if let Some(port) = port {
                if let Some(tx) = tx.take() {
                    let _ = tx.send(port);
                }
            }
        }
    });
    rx.recv_timeout(Duration::from_secs(20)).expect("the server did not announce its port")
}

#[test]
fn serveur_http_de_l_api() {
    let h = home().join("serve");
    std::fs::create_dir_all(&h).unwrap();
    struct Kill(std::process::Child);
    impl Drop for Kill {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    // Port 0: the system picks a free port, so parallel runs and the Playwright servers
    // (fixed ports in app/ui/e2e/ports.ts) never collide. The server prints the address it
    // bound on stderr once it is listening.
    let mut child = Kill(
        steno(&h)
            .args(["serve", "--port", "0"])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let port = served_port(child.0.stderr.take().unwrap());
    let (c, b) = http(port, "POST", "/api/invoke/live_state", "");
    assert_eq!(c, 200, "{b}");
    assert!(b.contains("\"call\""));
    let (c, b) = http(port, "POST", "/api/invoke/start_meeting", r#"{"title":"Essai HTTP"}"#);
    assert_eq!(c, 200, "{b}");
    let (c, b) = http(port, "POST", "/api/invoke/ask", r#"{"query":"Poste ça sur Slack"}"#);
    assert_eq!(c, 200);
    assert!(b.contains("\"refused\""), "{b}");
    let (c, _) = http(port, "POST", "/api/invoke/stop_meeting", "");
    assert_eq!(c, 200);
    let (c, b) = http(port, "POST", "/api/invoke/inconnue", "");
    assert_eq!(c, 404);
    assert!(b.contains("commande inconnue"));
    let (c, b) = http(port, "POST", "/api/invoke/get_meeting", r#"{"id":"n-existe-pas"}"#);
    assert_eq!(c, 400);
    assert!(b.contains("introuvable"));
    let (c, b) = http(port, "POST", "/api/invoke/ask", "{pas du json");
    assert_eq!(c, 400);
    assert!(b.contains("JSON invalide"));
}

/// Runs `steno replay <file>`, sends `cmds` then `shutdown`, and returns every line it printed.
fn replay_session(name: &str, fixture: &str, cmds: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let dir = std::env::temp_dir().join(format!("steno-replay-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("replay.jsonl");
    std::fs::write(&file, fixture).unwrap();
    let mut child = steno(&dir)
        .args(["replay", file.to_str().unwrap(), "--speed", "0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for c in cmds.iter().chain([&serde_json::json!({"cmd": "shutdown"})]) {
        writeln!(stdin, "{c}").unwrap();
    }
    drop(stdin);
    let mut out = String::new();
    child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
    assert!(child.wait().unwrap().success());
    let _ = std::fs::remove_dir_all(&dir);
    out.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

fn first_of<'a>(lines: &'a [serde_json::Value], kind: &str) -> &'a serde_json::Value {
    lines.iter().find(|l| l["type"] == kind).unwrap_or_else(|| panic!("no {kind} in {lines:?}"))
}

#[test]
fn replay_answers_calendar_events_from_the_fixture() {
    let fixture = concat!(
        r#"{"type":"calendar","events":["#,
        r#"{"id":"ev-1","title":"Revue Nova","start":"2026-11-27T10:00:00+01:00","end":"2026-11-27T11:00:00+01:00","all_day":false,"#,
        r#""attendees":[{"name":"Inès","email":"ines@exemple.fr","status":"accepted","organizer":false,"me":false}]},"#,
        r#"{"id":"ev-2","title":"Point mensuel","start":"2026-12-20T09:00:00+01:00","end":"2026-12-20T09:30:00+01:00","all_day":false,"attendees":[]}"#,
        "]}\n",
        r#"{"type":"call","active":true,"app":"teams","title":"Revue Nova"}"#,
        "\n",
    );
    let lines = replay_session(
        "calendar",
        fixture,
        &[
            serde_json::json!({"cmd": "permissions"}),
            serde_json::json!({"cmd": "calendar_events", "from": "2026-11-27T08:00:00+01:00", "to": "2026-12-04T08:00:00+01:00"}),
        ],
    );
    assert!(lines.iter().filter(|l| l["type"] == "permissions").all(|p| p["calendar"] == "granted"), "{lines:?}");
    assert!(!lines.iter().any(|l| l["type"] == "error"), "{lines:?}");
    let cal = first_of(&lines, "calendar");
    let ids: Vec<&str> = cal["events"].as_array().unwrap().iter().map(|e| e["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["ev-1"], "only the events that overlap the requested window");
    assert_eq!(cal["events"][0]["attendees"][0]["name"], "Inès");
    assert_eq!(lines.iter().filter(|l| l["type"] == "calendar").count(), 1, "the fixture's calendar is not replayed as is");
}

#[test]
fn replay_without_calendar_reports_no_calendar_source() {
    let lines = replay_session(
        "no-calendar",
        "{\"type\":\"call\",\"active\":true,\"app\":\"teams\",\"title\":\"Daily\"}\n",
        &[serde_json::json!({"cmd": "calendar_events", "from": "2026-11-27T08:00:00+01:00", "to": "2026-12-04T08:00:00+01:00"})],
    );
    assert_eq!(first_of(&lines, "permissions")["calendar"], "unknown");
    assert_eq!(first_of(&lines, "calendar")["events"], serde_json::json!([]));
}
