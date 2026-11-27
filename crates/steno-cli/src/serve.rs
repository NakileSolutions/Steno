//! `steno serve` : l'API de l'app (docs/contracts.md §5) en HTTP + SSE, et l'interface web
//! servie depuis le même serveur. Sert aux tests e2e réels (Playwright) et au développement
//! sans Tauri : ouvrir `http://127.0.0.1:<port>/?backend=http`.
//!
//! - `POST /api/invoke/<commande>` : corps = arguments JSON (camelCase) → 200 + JSON, ou
//!   4xx + `{"error": "…"}` ;
//! - `GET /api/events` : flux SSE, `event:` = nom de l'événement, `data:` = charge JSON.
//!
//! Écoute uniquement sur 127.0.0.1. Les routes `/api/*` n'acceptent que des requêtes venues
//! de la page servie ici (voir `check_request`) : une page web tierce ouverte dans le
//! navigateur ne peut ni appeler l'API ni lire le flux d'événements.

use anyhow::Result;
use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream::Stream;
use steno_core::Core;
use serde_json::{json, Value};
use std::convert::Infallible;
use std::path::PathBuf;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

const LOCAL_HOSTS: [&str; 3] = ["127.0.0.1", "localhost", "[::1]"];

fn single_header<'a>(headers: &'a HeaderMap, name: &header::HeaderName) -> Result<Option<&'a str>, ()> {
    let mut it = headers.get_all(name).iter();
    let Some(first) = it.next() else { return Ok(None) };
    if it.next().is_some() {
        return Err(());
    }
    first.to_str().map(|s| Some(s.trim())).map_err(|_| ())
}

/// En-tête `Host` local : `127.0.0.1`, `localhost` ou `[::1]`, sans port ou avec le port du
/// serveur. Tout autre nom (rebinding DNS : `evil.example` résolu vers 127.0.0.1) est refusé.
fn host_allowed(host: &str, port: u16) -> bool {
    let host = host.to_ascii_lowercase();
    let (name, rest) = match host.find(']') {
        Some(i) if host.starts_with('[') => host.split_at(i + 1),
        _ => match host.find(':') {
            Some(i) => host.split_at(i),
            None => (host.as_str(), ""),
        },
    };
    let port_ok = rest.is_empty() || rest.strip_prefix(':').and_then(|p| p.parse::<u16>().ok()) == Some(port);
    LOCAL_HOSTS.contains(&name) && port_ok
}

/// `Origin` de la page servie : le serveur n'écoute qu'en IPv4 (pas de `[::1]`).
fn origin_allowed(origin: &str, port: u16) -> bool {
    let origin = origin.to_ascii_lowercase();
    ["127.0.0.1", "localhost"].iter().any(|h| origin == format!("http://{h}:{port}"))
}

fn json_content_type(ct: &str) -> bool {
    let ct = ct.to_ascii_lowercase();
    match ct.strip_prefix("application/json") {
        Some(rest) => rest.is_empty() || rest.starts_with(';') || rest.starts_with(' '),
        None => false,
    }
}

/// Contrôle d'une requête `/api/*` : hôte local, origine du serveur lui-même, pas de requête
/// inter-sites, et corps JSON déclaré pour les appels de commande (une requête `text/plain`
/// envoyée sans pré-vérification CORS est refusée).
pub(crate) fn check_request(port: u16, method: &Method, headers: &HeaderMap) -> Result<(), (StatusCode, &'static str)> {
    let forbidden = |m| Err((StatusCode::FORBIDDEN, m));
    match single_header(headers, &header::HOST) {
        Ok(Some(h)) if host_allowed(h, port) => {}
        _ => return forbidden("hôte non autorisé : l'API n'est servie qu'en local"),
    }
    match single_header(headers, &header::ORIGIN) {
        Ok(None) => {}
        Ok(Some(o)) if origin_allowed(o, port) => {}
        _ => return forbidden("origine non autorisée"),
    }
    match single_header(headers, &header::HeaderName::from_static("sec-fetch-site")) {
        Ok(None) => {}
        Ok(Some(s)) if s.eq_ignore_ascii_case("same-origin") || s.eq_ignore_ascii_case("none") => {}
        _ => return forbidden("requête inter-sites refusée"),
    }
    if method == Method::POST {
        match single_header(headers, &header::CONTENT_TYPE) {
            Ok(Some(ct)) if json_content_type(ct) => {}
            _ => return Err((StatusCode::UNSUPPORTED_MEDIA_TYPE, "corps JSON attendu (Content-Type: application/json)")),
        }
    }
    Ok(())
}

async fn guard(State(port): State<u16>, req: Request, next: Next) -> Response {
    if let Err((status, msg)) = check_request(port, req.method(), req.headers()) {
        return (status, Json(json!({ "error": msg }))).into_response();
    }
    next.run(req).await
}

async fn invoke(State(core): State<Core>, Path(cmd): Path<String>, body: axum::body::Bytes) -> Response {
    // Corps vide ou absent = aucun argument.
    let args = if body.iter().all(|b| b.is_ascii_whitespace()) {
        Value::Null
    } else {
        match serde_json::from_slice::<Value>(&body) {
            Ok(v) => v,
            Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("JSON invalide : {e}") }))).into_response(),
        }
    };
    // Commande inconnue : 404, quelle que soit la langue du message d'erreur.
    let status = if steno_core::api::COMMANDS.contains(&cmd.as_str()) { StatusCode::BAD_REQUEST } else { StatusCode::NOT_FOUND };
    match steno_core::api::dispatch(&core, &cmd, args).await {
        Ok(v) => Json(v).into_response(),
        Err(msg) => (status, Json(json!({ "error": msg }))).into_response(),
    }
}

async fn events(State(core): State<Core>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(core.subscribe()).filter_map(|ev| {
        let ev = ev.ok()?;
        let (name, data) = ev.to_wire()?;
        Some(Ok(Event::default().event(name).data(data.to_string())))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Routes du serveur ; `port` est celui sur lequel il écoute (contrôle de `Host` et `Origin`).
pub fn router(core: Core, ui: Option<PathBuf>, port: u16) -> Router {
    let api = Router::new()
        .route("/api/invoke/{cmd}", post(invoke))
        .route("/api/events", get(events))
        .with_state(core)
        .route_layer(middleware::from_fn_with_state(port, guard));
    match ui {
        Some(dir) => {
            let index = dir.join("index.html");
            api.fallback_service(tower_http::services::ServeDir::new(dir).fallback(tower_http::services::ServeFile::new(index)))
        }
        None => api,
    }
}

pub async fn run(core: Core, port: u16, ui: Option<PathBuf>) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let addr = listener.local_addr()?;
    eprintln!("Sténo sert l'API sur http://{addr}/api ; interface : http://{addr}/?backend=http");
    let shutdown = core.clone();
    axum::serve(listener, router(core, ui, addr.port()))
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            shutdown.shutdown();
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.append(header::HeaderName::from_bytes(k.as_bytes()).unwrap(), HeaderValue::from_str(v).unwrap());
        }
        h
    }

    fn status(method: Method, pairs: &[(&str, &str)]) -> Option<StatusCode> {
        check_request(4180, &method, &headers(pairs)).err().map(|(s, _)| s)
    }

    #[test]
    fn hotes_locaux_seulement() {
        for h in ["127.0.0.1", "127.0.0.1:4180", "localhost", "LOCALHOST:4180", "[::1]", "[::1]:4180"] {
            assert!(host_allowed(h, 4180), "{h} devrait être accepté");
        }
        for h in ["evil.example", "evil.example:4180", "127.0.0.1:4181", "localhost:", "localhost.evil.example", "[::1]x", "127.0.0.2", ""] {
            assert!(!host_allowed(h, 4180), "{h} devrait être refusé");
        }
        assert_eq!(status(Method::GET, &[]), Some(StatusCode::FORBIDDEN), "Host absent");
        assert_eq!(status(Method::GET, &[("host", "rebind.example:4180")]), Some(StatusCode::FORBIDDEN));
        assert_eq!(status(Method::GET, &[("host", "localhost"), ("host", "evil.example")]), Some(StatusCode::FORBIDDEN));
        assert_eq!(status(Method::GET, &[("host", "127.0.0.1:4180")]), None, "flux SSE de la page servie");
    }

    #[test]
    fn appels_de_commande() {
        let ok = [("host", "127.0.0.1:4180"), ("content-type", "application/json")];
        assert_eq!(status(Method::POST, &ok), None);
        assert_eq!(status(Method::POST, &[("host", "localhost"), ("content-type", "application/json; charset=utf-8")]), None);
        assert_eq!(
            status(Method::POST, &[("host", "localhost"), ("content-type", "text/plain")]),
            Some(StatusCode::UNSUPPORTED_MEDIA_TYPE)
        );
        assert_eq!(status(Method::POST, &[("host", "localhost")]), Some(StatusCode::UNSUPPORTED_MEDIA_TYPE));
        assert_eq!(
            status(Method::POST, &[("host", "localhost"), ("content-type", "application/jsonp")]),
            Some(StatusCode::UNSUPPORTED_MEDIA_TYPE)
        );
    }

    #[test]
    fn origine_et_requetes_inter_sites() {
        let base = [("host", "127.0.0.1:4180"), ("content-type", "application/json")];
        let with = |extra: (&'static str, &'static str)| {
            let mut v = base.to_vec();
            v.push(extra);
            status(Method::POST, &v)
        };
        assert_eq!(with(("origin", "http://127.0.0.1:4180")), None);
        assert_eq!(with(("origin", "http://localhost:4180")), None);
        assert_eq!(with(("origin", "https://evil.example")), Some(StatusCode::FORBIDDEN));
        assert_eq!(with(("origin", "http://localhost:3000")), Some(StatusCode::FORBIDDEN));
        assert_eq!(with(("origin", "null")), Some(StatusCode::FORBIDDEN));
        assert_eq!(with(("origin", "http://[::1]:4180")), Some(StatusCode::FORBIDDEN), "serveur en IPv4 seulement");
        assert_eq!(with(("sec-fetch-site", "same-origin")), None);
        assert_eq!(with(("sec-fetch-site", "none")), None);
        assert_eq!(with(("sec-fetch-site", "cross-site")), Some(StatusCode::FORBIDDEN));
        assert_eq!(with(("sec-fetch-site", "same-site")), Some(StatusCode::FORBIDDEN));
    }
}
