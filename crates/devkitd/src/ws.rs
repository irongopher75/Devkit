use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use tokio::sync::broadcast;

use crate::security::{is_allowed_origin, tokens_match};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct WsAuth {
    token: Option<String>,
}

/// Browsers cannot set custom headers on a WebSocket handshake, so unlike
/// the REST API (which authenticates via header), the token travels as a
/// query parameter here. It is still a one-run, unguessable secret, and
/// the Origin header — which browsers always send on a WS handshake — is
/// checked the same way as everywhere else.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(auth): Query<WsAuth>,
    headers: HeaderMap,
) -> Response {
    let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok());
    if !is_allowed_origin(origin, state.port) {
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    if !tokens_match(auth.token.as_deref(), &state.token) {
        return (StatusCode::UNAUTHORIZED, "missing or invalid session token").into_response();
    }
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Push-only: the daemon streams events, the UI never sends commands over
/// this socket (see api.rs for why command execution is a REST call keyed
/// by id, never a message the browser composes itself).
async fn handle_socket(mut socket: WebSocket, state: AppState) {
    for event in state.bus.replay() {
        let Ok(text) = serde_json::to_string(&event) else { continue };
        if socket.send(Message::Text(text)).await.is_err() {
            return;
        }
    }

    let mut rx = state.bus.subscribe();
    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(event) => {
                        let Ok(text) = serde_json::to_string(&event) else { continue };
                        if socket.send(Message::Text(text)).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}
