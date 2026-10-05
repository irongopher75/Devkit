use axum::extract::{Path, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use crate::security::{is_allowed_origin, tokens_match, TOKEN_HEADER};
use crate::state::AppState;
use crate::supervisor::ProcessStatus;

#[derive(Serialize)]
pub struct CommandView {
    pub id: String,
    pub name: String,
    pub cmd: String,
    pub source: String,
    pub auto_start: bool,
    pub status: ProcessStatus,
}

#[derive(Serialize)]
pub struct StateResponse {
    pub commands: Vec<CommandView>,
}

/// Applied to every `/api/*` route. The browser authenticates with the
/// per-run token (never a password, never persisted beyond this run); the
/// Origin check is the backstop against DNS rebinding specifically.
pub async fn auth_guard(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let origin = req.headers().get(header::ORIGIN).and_then(|v| v.to_str().ok());
    if !is_allowed_origin(origin, state.port) {
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    let token = req.headers().get(TOKEN_HEADER).and_then(|v| v.to_str().ok());
    if !tokens_match(token, &state.token) {
        return (StatusCode::UNAUTHORIZED, "missing or invalid session token").into_response();
    }
    next.run(req).await
}

pub async fn get_state(State(state): State<AppState>) -> Json<StateResponse> {
    let commands = state
        .config
        .commands
        .iter()
        .map(|c| CommandView {
            id: c.id.clone(),
            name: c.name.clone(),
            cmd: c.cmd.clone(),
            source: c.source.clone(),
            auto_start: c.auto_start,
            status: state.supervisor.status_of(&c.id),
        })
        .collect();
    Json(StateResponse { commands })
}

/// All three handlers below take a command *id* that the daemon looks up
/// in its own config — the browser cannot send an arbitrary shell string,
/// so a malicious page can't use this endpoint to run code on the host
/// even if it somehow got past the token and Origin checks.
pub async fn run_command(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    run_supervisor_action(move || state.supervisor.run(&id))
}

pub async fn stop_command(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    run_supervisor_action(move || state.supervisor.stop(&id))
}

pub async fn restart_command(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    run_supervisor_action(move || state.supervisor.restart(&id))
}

fn run_supervisor_action(action: impl FnOnce() -> anyhow::Result<()>) -> Response {
    match action() {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
    }
}
