use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Router,
};
use crate::artikel::Artikel;

pub mod routes;

#[derive(Clone)]
pub struct AppState {
    pub artikel: Arc<RwLock<Vec<Artikel>>>,
    pub password: String,
    /// Ziel von `speichern()`. Im State statt global, damit Tests eigene Dateien nutzen.
    pub csv_pfad: PathBuf,
}

/// Alle Routen inkl. Basic-Auth-Middleware. In `main.rs` und in den Tests (`tests/web.rs`)
/// verwendet.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(routes::index))
        .route("/static/htmx-2.0.4.min.js", get(routes::htmx_js))
        .route("/artikel", post(routes::hinzufuegen))
        .route("/artikel/:id/modal", get(routes::zeige_modal))
        .route("/artikel/:id", put(routes::bearbeiten).delete(routes::loeschen))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .with_state(state)
}

async fn auth_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if check_basic_auth(request.headers(), &state.password) {
        next.run(request).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            [(
                header::WWW_AUTHENTICATE,
                r#"Basic realm="Ablaufdatum-Tracker", charset="UTF-8""#,
            )],
            "Nicht autorisiert",
        )
            .into_response()
    }
}

fn check_basic_auth(headers: &HeaderMap, expected_password: &str) -> bool {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let Some(auth) = headers.get(header::AUTHORIZATION) else {
        return false;
    };
    let Ok(auth_str) = auth.to_str() else {
        return false;
    };
    let Some(encoded) = auth_str.strip_prefix("Basic ") else {
        return false;
    };
    let Ok(decoded) = STANDARD.decode(encoded.trim()) else {
        return false;
    };
    let Ok(creds) = std::str::from_utf8(&decoded) else {
        return false;
    };
    // Accept "anyuser:password" — the username is ignored
    let pw = creds.split_once(':').map_or(creds, |x| x.1);
    pw == expected_password
}
