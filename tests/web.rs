//! Handler-Tests: der komplette Router (inkl. Basic Auth) ohne echten Server,
//! jeder Test mit eigener CSV-Datei in einem Temp-Verzeichnis.

use std::path::{Path, PathBuf};

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
use tower::ServiceExt; // oneshot

const PASSWORT: &str = "geheim";
const KOPF: &str = "name,kaufdatum,menge,ablaufdatum,bemerkung\n";

struct Testumgebung {
    app: Router,
    csv: PathBuf,
    /// `AppState::generation` dieses „Serverstarts“.
    gen: String,
    _dir: tempfile::TempDir,
}

/// Ein „Serverstart“: lädt die CSV wie `main.rs` und baut den Router.
fn starten(csv: &Path) -> (Router, String) {
    let state = AppState::neu(laden_aus(csv).unwrap(), PASSWORT.into(), csv.to_path_buf());
    let gen = state.generation.clone();
    (router(state), gen)
}

impl Testumgebung {
    /// Wie [`htmx`], aber mit der aktuellen Generation (wie eine frisch geladene Seite).
    /// GET/DELETE: `gen` in der URL, sonst im Formular (so wie HTMX 2 `hx-include` sendet).
    fn htmx(&self, method: Method, uri: &str, form: Option<&str>) -> Request<Body> {
        mit_gen(method, uri, form, &self.gen)
    }

    /// Simuliert einen Neustart des Servers mit derselben CSV-Datei.
    fn neustart(&mut self) {
        let (app, gen) = starten(&self.csv);
        self.app = app;
        self.gen = gen;
    }
}

fn mit_gen(method: Method, uri: &str, form: Option<&str>, gen: &str) -> Request<Body> {
    if method == Method::GET || method == Method::DELETE {
        let sep = if uri.contains('?') { '&' } else { '?' };
        htmx(method, &format!("{uri}{sep}gen={gen}"), form)
    } else {
        let f = match form {
            Some(f) if !f.is_empty() => format!("{f}&gen={gen}"),
            _ => format!("gen={gen}"),
        };
        htmx(method, uri, Some(&f))
    }
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
    let (app, gen) = starten(&csv);
    Testumgebung { app, csv, gen, _dir: dir }
}

fn standard() -> Testumgebung {
    // id 0 = Reis (ok), id 1 = Milch (abgelaufen)
    umgebung(Some(&format!("{KOPF}Reis,,2,2099-06-10,Keller\nMilch,,,2000-01-01,\n")))
}

fn auth(pw: &str) -> String {
    format!("Basic {}", STANDARD.encode(format!("irgendwer:{pw}")))
}

/// Anfrage wie von HTMX (mit Auth und `HX-Request`), **ohne** Generation;
/// `form` als urlencoded Body.
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

    let p = senden(&t.app, t.htmx(Method::GET, "/", None)).await;
    assert_eq!(p.status, StatusCode::OK);
    assert!(!p.text.contains("<html"));
    assert!(p.text.starts_with("<div id=\"main-content\">"));
}

#[tokio::test]
async fn sortiert_und_farbklassen() {
    let t = standard();
    let p = senden(&t.app, t.htmx(Method::GET, "/", None)).await;
    // Milch (2000) vor Reis (2099), obwohl in der CSV umgekehrt
    let milch = p.text.find("<td>Milch</td>").unwrap();
    let reis = p.text.find("<td>Reis</td>").unwrap();
    assert!(milch < reis);
    assert!(p.text.contains("status-abgelaufen") && p.text.contains("status-ok"));
}

#[tokio::test]
async fn filter_und_unbekannter_filter() {
    let t = standard();
    let p = senden(&t.app, t.htmx(Method::GET, "/?filter=abgelaufen", None)).await;
    assert!(p.text.contains("<td>Milch</td>") && !p.text.contains("<td>Reis</td>"));
    assert!(p.text.contains(r#"id="aktiver-filter" name="filter" value="abgelaufen""#));

    let p = senden(&t.app, t.htmx(Method::GET, "/?filter=%3Cscript%3E", None)).await;
    assert!(p.text.contains(r#"value="alle""#) && !p.text.contains("<script>"));
}

#[tokio::test]
async fn htmx_datei_mit_cache_header() {
    let t = standard();
    let a = senden(&t.app, t.htmx(Method::GET, "/static/htmx-2.0.4.min.js", None)).await;
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
        t.htmx(Method::POST, "/artikel", Some("name=+Mehl+&ablaufdatum=2099-03-15&menge=4&bemerkung=Vorrat&filter=alle")),
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
    senden(&t.app, t.htmx(Method::POST, "/artikel", Some("name=Salz&ablaufdatum=01.02.2099"))).await;
    assert!(csv(&t).contains(",2099-02-01,"));
}

#[tokio::test]
async fn hinzufuegen_mit_fehlern_speichert_nicht() {
    let t = standard();
    let vorher = csv(&t);
    let a = senden(
        &t.app,
        t.htmx(Method::POST, "/artikel", Some("name=&ablaufdatum=31.02.2027&menge=1,5&bemerkung=x")),
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
    let a = senden(&t.app, t.htmx(Method::POST, "/artikel", Some("name=Mehl&ablaufdatum=2099-01-01"))).await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(csv(&t).contains("Mehl,"));

    // Nur die Generation, sonst nichts
    let leer = senden(&t.app, t.htmx(Method::POST, "/artikel", Some(""))).await;
    assert_eq!(leer.status, StatusCode::OK);
    assert!(leer.text.contains("Name darf nicht leer sein."));
    assert!(leer.text.contains("Ablaufdatum ist Pflichtfeld."));
}

#[tokio::test]
async fn hinzufuegen_behaelt_filter_und_zeigt_hinweis() {
    let t = standard();
    let a = senden(
        &t.app,
        t.htmx(Method::POST, "/artikel", Some("name=Honig&ablaufdatum=2099-01-01&filter=abgelaufen")),
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
    let a = senden(&t.app, t.htmx(Method::GET, "/artikel/0/modal", None)).await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(a.text.contains(r#"value="Reis""#));
    assert!(a.text.contains(r#"type="date" name="ablaufdatum" value="2099-06-10""#));

    // Unbekannte ID: kein stilles 404 mehr, sondern Tabelle + Fehlerbanner
    let fehlt = senden(&t.app, t.htmx(Method::GET, "/artikel/999/modal", None)).await;
    assert_eq!(fehlt.status, StatusCode::OK);
    assert_eq!(fehlt.headers["HX-Retarget"], "#main-content");
    assert!(fehlt.text.contains("Artikel nicht gefunden – wurde er inzwischen gelöscht?"));
}

#[tokio::test]
async fn bearbeiten_speichert_und_schliesst_modal() {
    let t = standard();
    let a = senden(
        &t.app,
        t.htmx(Method::PUT, "/artikel/0", Some("name=Reis&ablaufdatum=2098-01-01&menge=&bemerkung=")),
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
    let a = senden(&t.app, t.htmx(Method::PUT, "/artikel/0", Some("name=&ablaufdatum=10.06.2099&menge=0"))).await;
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
    let a = senden(&t.app, t.htmx(Method::DELETE, "/artikel/1?name=Milch&filter=ok", None)).await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(a.text.contains(r#"value="ok""#));
    assert!(a.text.contains(r#"<div id="modal-container" hx-swap-oob="true"></div>"#));
    let inhalt = csv(&t);
    assert!(!inhalt.contains("Milch") && inhalt.contains("Reis"));
}

// ── Punkt 2: Artikel existiert nicht (mehr) ────────────────────────────────────

#[tokio::test]
async fn bearbeiten_eines_geloeschten_artikels_zeigt_fehler() {
    let t = standard();
    let vorher = csv(&t);
    let a = senden(
        &t.app,
        t.htmx(Method::PUT, "/artikel/999", Some("name=Geist&ablaufdatum=2099-01-01&filter=ok")),
    )
    .await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(a.text.contains(r#"class="error-banner">Artikel nicht gefunden – wurde er inzwischen gelöscht?"#));
    assert!(a.text.contains(r#"<div id="modal-container" hx-swap-oob="true"></div>"#), "Modal schließt");
    assert!(a.text.contains(r#"value="ok""#), "Filter bleibt");
    assert_eq!(csv(&t), vorher);
}

#[tokio::test]
async fn loeschen_eines_geloeschten_artikels_zeigt_fehler() {
    let t = standard();
    // Zweimal dieselbe ID löschen (z. B. zwei Geräte): beim zweiten Mal Hinweis, CSV unverändert
    senden(&t.app, t.htmx(Method::DELETE, "/artikel/1", None)).await;
    let nach_erstem = csv(&t);
    let a = senden(&t.app, t.htmx(Method::DELETE, "/artikel/1", None)).await;
    assert!(a.text.contains("Artikel nicht gefunden – wurde er inzwischen gelöscht?"));
    assert!(a.text.contains(r#"<div id="modal-container" hx-swap-oob="true"></div>"#));
    assert_eq!(csv(&t), nach_erstem);
    assert!(nach_erstem.contains("Reis"));
}

// ── Punkt 1: veraltete IDs nach Neustart ───────────────────────────────────────

fn stale_szenario() -> Testumgebung {
    umgebung(Some(&format!("{KOPF}Apfelmus,,,2027-03-01,\nBohnen,,,2028-03-01,\n")))
}

#[tokio::test]
async fn veraltete_seite_nach_neustart_ueberschreibt_keinen_anderen_artikel() {
    let mut t = stale_szenario();
    // Seite A: Couscous hinzufügen → in diesem Lauf id 2 (max+1)
    let a = senden(
        &t.app,
        t.htmx(Method::POST, "/artikel", Some("name=Couscous&ablaufdatum=2026-12-24")),
    )
    .await;
    assert!(a.text.contains(r#"hx-get="/artikel/2/modal""#) && a.text.contains("<td>Couscous</td>"));
    let alte_gen = t.gen.clone();

    // Neustart: CSV ist nach Datum sortiert gespeichert → Couscous 0, Apfelmus 1, Bohnen 2
    t.neustart();
    assert_ne!(t.gen, alte_gen);
    let vorher = csv(&t);

    // Die alte Seite speichert „ihren“ Couscous (id 2) → muss abgelehnt werden
    let put = senden(
        &t.app,
        mit_gen(Method::PUT, "/artikel/2", Some("name=Couscous&ablaufdatum=2026-12-31&filter=ok"), &alte_gen),
    )
    .await;
    assert_eq!(put.status, StatusCode::CONFLICT);
    assert_eq!(put.headers["HX-Redirect"], "/?filter=ok&veraltet=1");
    assert_eq!(csv(&t), vorher, "Bohnen darf nicht überschrieben werden");
    assert!(csv(&t).contains("Bohnen,,,2028-03-01,"));

    // Auch Löschen, Modal öffnen und Hinzufügen von der alten Seite: nichts passiert
    let del = senden(&t.app, mit_gen(Method::DELETE, "/artikel/2", None, &alte_gen)).await;
    assert_eq!(del.status, StatusCode::CONFLICT);
    let modal = senden(&t.app, mit_gen(Method::GET, "/artikel/2/modal", None, &alte_gen)).await;
    assert_eq!(modal.status, StatusCode::CONFLICT);
    assert!(!modal.text.contains("Bohnen"));
    let post = senden(
        &t.app,
        mit_gen(Method::POST, "/artikel", Some("name=Doppelt&ablaufdatum=2099-01-01"), &alte_gen),
    )
    .await;
    assert_eq!(post.status, StatusCode::CONFLICT);
    assert_eq!(csv(&t), vorher);

    // Nach dem Neuladen (neue Generation) trifft id 2 wirklich Bohnen
    let modal = senden(&t.app, t.htmx(Method::GET, "/artikel/2/modal", None)).await;
    assert!(modal.text.contains(r#"value="Bohnen""#));
}

#[tokio::test]
async fn anfrage_ohne_generation_gilt_als_veraltet() {
    let t = standard();
    let vorher = csv(&t);
    // z. B. eine Seite, die noch vor diesem Update geladen wurde
    let a = senden(&t.app, htmx(Method::DELETE, "/artikel/0?filter=warnung", None)).await;
    assert_eq!(a.status, StatusCode::CONFLICT);
    assert_eq!(a.headers["HX-Redirect"], "/?filter=warnung&veraltet=1");
    assert_eq!(csv(&t), vorher);
}

#[tokio::test]
async fn neu_geladene_seite_zeigt_veraltet_banner_und_neue_generation() {
    let t = standard();
    let seite = Request::get("/?filter=ok&veraltet=1")
        .header(header::AUTHORIZATION, auth(PASSWORT))
        .body(Body::empty())
        .unwrap();
    let a = senden(&t.app, seite).await;
    assert!(a.text.contains(
        r#"class="error-banner">Die Seite war veraltet und wurde neu geladen. Bitte erneut versuchen."#
    ));
    assert!(a.text.contains(r#"value="ok""#));
    assert!(a.text.contains(&format!(r#"id="seiten-generation" name="gen" value="{}""#, t.gen)));
    // ohne veraltet=1 kein Banner
    let p = senden(&t.app, t.htmx(Method::GET, "/", None)).await;
    assert!(!p.text.contains("Die Seite war veraltet"));
}

// ── Punkt 3: Doppeltes Absenden (Markup; das Verwerfen selbst macht HTMX im Browser) ──

#[tokio::test]
async fn formulare_verwerfen_doppeltes_absenden() {
    let t = standard();
    let seite = senden(&t.app, t.htmx(Method::GET, "/", None)).await;
    let form = &seite.text[seite.text.find(r#"<form hx-post="/artikel""#).unwrap()..];
    let form = &form[..form.find('>').unwrap()];
    assert!(form.contains(r#"hx-sync="this:drop""#) && form.contains("hx-disabled-elt="), "{form}");

    let modal = senden(&t.app, t.htmx(Method::GET, "/artikel/0/modal", None)).await;
    assert!(modal.text.contains(r#"hx-sync="this:drop""#));
    assert!(modal.text.contains(r#"hx-sync="closest form:drop""#));
    assert_eq!(modal.text.matches(r##"hx-disabled-elt="#modal-speichern, #modal-loeschen""##).count(), 2);
    assert!(modal.text.contains(r#"id="modal-speichern""#) && modal.text.contains(r#"id="modal-loeschen""#));
}
