//! Handler-Tests: der komplette Router (inkl. Basic Auth) ohne echten Server,
//! jeder Test mit eigener CSV-Datei in einem Temp-Verzeichnis.

use std::path::PathBuf;
use std::sync::Arc;

use ablaufdatum_tracker::{
    csv_io::laden_aus,
    web::{router, AppState},
};
use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    Router,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use tokio::sync::RwLock;
use tower::ServiceExt; // oneshot

const PASSWORT: &str = "geheim";
const KOPF: &str = "name,kaufdatum,menge,ablaufdatum,bemerkung\n";

struct Testumgebung {
    app: Router,
    csv: PathBuf,
    _dir: tempfile::TempDir,
}

/// Startet den Router mit `csv_inhalt` als artikel.csv (`None` = Datei existiert nicht).
/// Ablaufdaten in den Tests liegen weit in der Zukunft/Vergangenheit, damit der Status
/// nicht vom Tag des Testlaufs abhängt.
fn umgebung(csv_inhalt: Option<&str>) -> Testumgebung {
    let dir = tempfile::tempdir().unwrap();
    let csv = dir.path().join("artikel.csv");
    if let Some(inhalt) = csv_inhalt {
        std::fs::write(&csv, inhalt).unwrap();
    }
    let artikel = laden_aus(&csv).unwrap();
    let state = AppState {
        artikel: Arc::new(RwLock::new(artikel)),
        password: PASSWORT.into(),
        csv_pfad: csv.clone(),
    };
    Testumgebung { app: router(state), csv, _dir: dir }
}

fn standard() -> Testumgebung {
    // id 0 = Reis (ok), id 1 = Milch (abgelaufen)
    umgebung(Some(&format!("{KOPF}Reis,,2,2099-06-10,Keller\nMilch,,,2000-01-01,\n")))
}

fn auth(pw: &str) -> String {
    format!("Basic {}", STANDARD.encode(format!("irgendwer:{pw}")))
}

/// Anfrage wie von HTMX (mit Auth und `HX-Request`); `form` als urlencoded Body.
fn htmx(method: Method, uri: &str, form: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, auth(PASSWORT))
        .header("HX-Request", "true");
    if form.is_some() {
        b = b.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    }
    b.body(Body::from(form.unwrap_or("").to_string())).unwrap()
}

struct Antwort {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    text: String,
}

async fn senden(app: &Router, req: Request<Body>) -> Antwort {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    Antwort { status, headers, text: String::from_utf8(bytes.to_vec()).unwrap() }
}

fn csv(t: &Testumgebung) -> String {
    std::fs::read_to_string(&t.csv).unwrap_or_default()
}

// ── Auth ───────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn ohne_oder_mit_falschem_passwort_401() {
    let t = standard();
    let ohne = senden(&t.app, Request::get("/").body(Body::empty()).unwrap()).await;
    assert_eq!(ohne.status, StatusCode::UNAUTHORIZED);
    assert!(ohne.headers[header::WWW_AUTHENTICATE].to_str().unwrap().starts_with("Basic "));

    let falsch = Request::get("/")
        .header(header::AUTHORIZATION, auth("falsch"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(senden(&t.app, falsch).await.status, StatusCode::UNAUTHORIZED);

    let kaputt = Request::get("/")
        .header(header::AUTHORIZATION, "Basic %%%")
        .body(Body::empty())
        .unwrap();
    assert_eq!(senden(&t.app, kaputt).await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn schreibender_request_ohne_auth_aendert_nichts() {
    let t = standard();
    let vorher = csv(&t);
    let req = Request::delete("/artikel/0").body(Body::empty()).unwrap();
    assert_eq!(senden(&t.app, req).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(csv(&t), vorher);
}

// ── Seite / Partial ────────────────────────────────────────────────────────────

#[tokio::test]
async fn volle_seite_und_htmx_partial() {
    let t = standard();
    let seite = Request::get("/")
        .header(header::AUTHORIZATION, auth(PASSWORT))
        .body(Body::empty())
        .unwrap();
    let a = senden(&t.app, seite).await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(a.text.contains("<html") && a.text.contains("/static/htmx-2.0.4.min.js"));
    assert!(a.text.contains("<td>Reis</td>") && a.text.contains("<td>Milch</td>"));

    let p = senden(&t.app, htmx(Method::GET, "/", None)).await;
    assert_eq!(p.status, StatusCode::OK);
    assert!(!p.text.contains("<html"));
    assert!(p.text.starts_with("<div id=\"main-content\">"));
}

#[tokio::test]
async fn sortiert_und_farbklassen() {
    let t = standard();
    let p = senden(&t.app, htmx(Method::GET, "/", None)).await;
    // Milch (2000) vor Reis (2099), obwohl in der CSV umgekehrt
    let milch = p.text.find("<td>Milch</td>").unwrap();
    let reis = p.text.find("<td>Reis</td>").unwrap();
    assert!(milch < reis);
    assert!(p.text.contains("status-abgelaufen") && p.text.contains("status-ok"));
}

#[tokio::test]
async fn filter_und_unbekannter_filter() {
    let t = standard();
    let p = senden(&t.app, htmx(Method::GET, "/?filter=abgelaufen", None)).await;
    assert!(p.text.contains("<td>Milch</td>") && !p.text.contains("<td>Reis</td>"));
    assert!(p.text.contains(r#"id="aktiver-filter" name="filter" value="abgelaufen""#));

    let p = senden(&t.app, htmx(Method::GET, "/?filter=%3Cscript%3E", None)).await;
    assert!(p.text.contains(r#"value="alle""#) && !p.text.contains("<script>"));
}

#[tokio::test]
async fn htmx_datei_mit_cache_header() {
    let t = standard();
    let a = senden(&t.app, htmx(Method::GET, "/static/htmx-2.0.4.min.js", None)).await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(a.headers[header::CACHE_CONTROL].to_str().unwrap().contains("immutable"));
    assert!(a.text.contains(r#"version:"2.0.4""#));
}

// ── Hinzufügen ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn hinzufuegen_speichert_in_csv() {
    let t = umgebung(None); // Datei existiert noch nicht
    let a = senden(
        &t.app,
        htmx(Method::POST, "/artikel", Some("name=+Mehl+&ablaufdatum=2099-03-15&menge=4&bemerkung=Vorrat&filter=alle")),
    )
    .await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(a.text.contains("<td>Mehl</td>") && a.text.contains("<td>15.03.2099</td>"));
    let inhalt = csv(&t);
    assert!(inhalt.starts_with(KOPF));
    // Name getrimmt, Kaufdatum = heute (automatisch), Datum ISO
    let heute = chrono::Local::now().date_naive().format("%Y-%m-%d").to_string();
    assert!(inhalt.contains(&format!("Mehl,{heute},4,2099-03-15,Vorrat")), "{inhalt}");
}

#[tokio::test]
async fn hinzufuegen_akzeptiert_tt_mm_jjjj() {
    let t = umgebung(None);
    senden(&t.app, htmx(Method::POST, "/artikel", Some("name=Salz&ablaufdatum=01.02.2099"))).await;
    assert!(csv(&t).contains(",2099-02-01,"));
}

#[tokio::test]
async fn hinzufuegen_mit_fehlern_speichert_nicht() {
    let t = standard();
    let vorher = csv(&t);
    let a = senden(
        &t.app,
        htmx(Method::POST, "/artikel", Some("name=&ablaufdatum=31.02.2027&menge=1,5&bemerkung=x")),
    )
    .await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(a.text.contains("Name darf nicht leer sein."));
    assert!(a.text.contains("Ungültiges Datum"));
    assert!(a.text.contains("Menge muss eine ganze Zahl sein."));
    assert!(a.text.contains(r#"value="x""#), "Eingaben bleiben erhalten");
    assert_eq!(csv(&t), vorher);
}

#[tokio::test]
async fn fehlende_felder_kein_422() {
    // Issue #3: früher 422, wenn ein optionales Feld fehlte
    let t = umgebung(None);
    let a = senden(&t.app, htmx(Method::POST, "/artikel", Some("name=Mehl&ablaufdatum=2099-01-01"))).await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(csv(&t).contains("Mehl,"));

    let leer = senden(&t.app, htmx(Method::POST, "/artikel", Some(""))).await;
    assert_eq!(leer.status, StatusCode::OK);
    assert!(leer.text.contains("Name darf nicht leer sein."));
    assert!(leer.text.contains("Ablaufdatum ist Pflichtfeld."));
}

#[tokio::test]
async fn hinzufuegen_behaelt_filter_und_zeigt_hinweis() {
    let t = standard();
    let a = senden(
        &t.app,
        htmx(Method::POST, "/artikel", Some("name=Honig&ablaufdatum=2099-01-01&filter=abgelaufen")),
    )
    .await;
    assert!(a.text.contains(r#"value="abgelaufen""#));
    assert!(a.text.contains("„Honig“ wurde gespeichert, ist im Filter „Abgelaufen“ aber nicht sichtbar."));
    assert!(!a.text.contains("<td>Honig</td>"));
}

// ── Modal / Bearbeiten ─────────────────────────────────────────────────────────

#[tokio::test]
async fn modal_mit_iso_datum_und_404() {
    let t = standard();
    let a = senden(&t.app, htmx(Method::GET, "/artikel/0/modal", None)).await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(a.text.contains(r#"value="Reis""#));
    assert!(a.text.contains(r#"type="date" name="ablaufdatum" value="2099-06-10""#));

    let fehlt = senden(&t.app, htmx(Method::GET, "/artikel/999/modal", None)).await;
    assert_eq!(fehlt.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn bearbeiten_speichert_und_schliesst_modal() {
    let t = standard();
    let a = senden(
        &t.app,
        htmx(Method::PUT, "/artikel/0", Some("name=Reis&ablaufdatum=2098-01-01&menge=&bemerkung=")),
    )
    .await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(a.text.contains(r#"<div id="modal-container" hx-swap-oob="true"></div>"#));
    let inhalt = csv(&t);
    // Menge/Bemerkung geleert, Kaufdatum (leer) unverändert
    assert!(inhalt.contains("Reis,,,2098-01-01,\n"), "{inhalt}");
    assert!(inhalt.contains("Milch,,,2000-01-01,"));
}

#[tokio::test]
async fn bearbeiten_mit_fehler_bleibt_im_modal() {
    let t = standard();
    let vorher = csv(&t);
    let a = senden(&t.app, htmx(Method::PUT, "/artikel/0", Some("name=&ablaufdatum=10.06.2099&menge=0"))).await;
    assert_eq!(a.headers["HX-Retarget"], "#modal-container");
    assert_eq!(a.headers["HX-Reswap"], "innerHTML");
    assert!(a.text.contains("Name darf nicht leer sein."));
    assert!(a.text.contains("Menge muss größer als 0 sein."));
    assert!(a.text.contains(r#"value="2099-06-10""#), "Datum für type=date als ISO");
    assert_eq!(csv(&t), vorher);
}

// ── Löschen ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn loeschen_entfernt_und_behaelt_filter() {
    let t = standard();
    let a = senden(&t.app, htmx(Method::DELETE, "/artikel/1?name=Milch&filter=ok", None)).await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(a.text.contains(r#"value="ok""#));
    assert!(a.text.contains(r#"<div id="modal-container" hx-swap-oob="true"></div>"#));
    let inhalt = csv(&t);
    assert!(!inhalt.contains("Milch") && inhalt.contains("Reis"));
}
