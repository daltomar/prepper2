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
use ablaufdatum_tracker::{
    csv_io::laden,
    web::{routes, AppState},
};

#[tokio::main]
async fn main() {
    let password = std::env::var("ABLAUFDATUM_PASSWORD").unwrap_or_else(|_| {
        eprintln!("Fehler: Umgebungsvariable ABLAUFDATUM_PASSWORD ist nicht gesetzt.");
        std::process::exit(1);
    });

    let artikel = laden().unwrap_or_else(|e| {
        eprintln!("Warnung: CSV konnte nicht geladen werden: {e}");
        Vec::new()
    });

    let state = AppState {
        artikel: Arc::new(RwLock::new(artikel)),
        password,
    };

    let app = Router::new()
        .route("/", get(routes::index))
        .route("/artikel", post(routes::hinzufuegen))
        .route("/artikel/:id/modal", get(routes::zeige_modal))
        .route("/artikel/:id", put(routes::bearbeiten).delete(routes::loeschen))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .with_state(state);

    let addr = "0.0.0.0:8080";
    println!("Ablaufdatum-Tracker läuft auf http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
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
    let pw = creds.splitn(2, ':').nth(1).unwrap_or(creds);
    pw == expected_password
}
