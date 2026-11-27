//! `steno serve` : l'API de l'app (docs/contracts.md §5) en HTTP + SSE, et l'interface web
//! servie depuis le même serveur. Sert aux tests e2e réels (Playwright) et au développement
//! sans Tauri : ouvrir `http://127.0.0.1:<port>/?backend=http`.
//!
//! - `POST /api/invoke/<commande>` : corps = arguments JSON (camelCase) → 200 + JSON, ou
//!   4xx + `{"error": "…"}` ;
//! - `GET /api/events` : flux SSE, `event:` = nom de l'événement, `data:` = charge JSON.
//!
//! Écoute uniquement sur 127.0.0.1.

use anyhow::Result;
use axum::extract::{Path, State};
use axum::http::StatusCode;
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
    match steno_core::api::dispatch(&core, &cmd, args).await {
        Ok(v) => Json(v).into_response(),
        Err(msg) => {
            let status = if msg.starts_with("commande inconnue") { StatusCode::NOT_FOUND } else { StatusCode::BAD_REQUEST };
            (status, Json(json!({ "error": msg }))).into_response()
        }
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

pub fn router(core: Core, ui: Option<PathBuf>) -> Router {
    let api = Router::new()
        .route("/api/invoke/{cmd}", post(invoke))
        .route("/api/events", get(events))
        .with_state(core);
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
    axum::serve(listener, router(core, ui))
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            shutdown.shutdown();
        })
        .await?;
    Ok(())
}
