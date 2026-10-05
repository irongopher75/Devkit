use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use rust_embed::RustEmbed;

use crate::state::AppState;

/// The built React UI (`ui/dist`), baked into the binary. Run `npm run
/// build` in `ui/` before `cargo build` — there is nothing to embed
/// otherwise.
#[derive(RustEmbed)]
#[folder = "../../ui/dist"]
struct UiAssets;

const TOKEN_PLACEHOLDER: &str = "__DEVKIT_TOKEN_PLACEHOLDER__";

/// Serves `index.html` with the session token stitched in. The page is
/// static and carries no secrets of its own; the token is what lets the
/// JS it loads authenticate its own API and WebSocket calls afterward.
pub async fn index(State(state): State<AppState>) -> Response {
    match UiAssets::get("index.html") {
        Some(file) => {
            let html = String::from_utf8_lossy(&file.data);
            let injected = html.replace(TOKEN_PLACEHOLDER, &state.token);
            Html(injected).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            "UI assets not found — run `npm run build` in ui/ first",
        )
            .into_response(),
    }
}

pub async fn asset(Path(path): Path<String>) -> Response {
    match UiAssets::get(&path) {
        Some(file) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(file.data.into_owned()))
                .unwrap()
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}
