use axum::{
    extract::{Form, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
};
use askama::Template;
use chrono::Datelike;
use serde::Deserialize;
use crate::{
    artikel::{AblaufStatus, Artikel, parse_datum, parse_menge},
    csv_io::speichern,
    web::AppState,
};

// ── View types ─────────────────────────────────────────────────────────────────

pub struct ArtikelZeile {
    pub id: u64,
    pub name: String,
    pub tage_bis_ablauf: i64,
    pub menge: String,
    pub ablaufdatum: String,
    pub bemerkung: String,
    pub status_class: String,
    /// true, wenn sich das Ablauf-Jahr gegenüber der vorherigen sichtbaren Zeile ändert.
    pub jahreswechsel: bool,
}

#[derive(Default)]
pub struct FormularState {
    pub name: String,
    pub ablaufdatum: String,
    pub menge: String,
    pub bemerkung: String,
    pub fehler_name: String,
    pub fehler_ablaufdatum: String,
    pub fehler_menge: String,
}

// ── Templates ──────────────────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    zeilen: Vec<ArtikelZeile>,
    filter: String,
    form: FormularState,
    fehler: String,
    hinweis: String,
}

#[derive(Template)]
#[template(path = "partials/main_content.html")]
struct MainContentTemplate {
    zeilen: Vec<ArtikelZeile>,
    filter: String,
    form: FormularState,
    fehler: String,
    /// Info-Meldung, z. B. wenn ein gespeicherter Artikel im aktiven Filter nicht sichtbar ist.
    hinweis: String,
}

#[derive(Template)]
#[template(path = "partials/modal.html")]
struct ModalTemplate {
    id: u64,
    name: String,
    menge: String,
    ablaufdatum: String,
    bemerkung: String,
    fehler_name: String,
    fehler_ablaufdatum: String,
    fehler_menge: String,
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn render<T: Template>(t: T) -> Response {
    match t.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

fn format_date(d: chrono::NaiveDate) -> String {
    format!("{:02}.{:02}.{}", d.day(), d.month(), d.year())
}

fn status_class(s: &AblaufStatus) -> &'static str {
    match s {
        AblaufStatus::Abgelaufen   => "status-abgelaufen",
        AblaufStatus::KritischBald => "status-kritisch",
        AblaufStatus::WarnungBald  => "status-warnung",
        AblaufStatus::Ok           => "status-ok",
    }
}

/// Erlaubte Filterwerte. Alles andere (fehlend, leer, unbekannt) wird zu `"alle"`, damit
/// nur bekannte Werte im Template (`value="{{ filter }}"`) und in den Requests landen.
fn normalisiere_filter(filter: Option<&str>) -> &'static str {
    match filter.map(str::trim) {
        Some("abgelaufen") => "abgelaufen",
        Some("kritisch") => "kritisch",
        Some("warnung") => "warnung",
        Some("ok") => "ok",
        _ => "alle",
    }
}

/// Status zu einem (normalisierten) Filter; `None` = alle anzeigen.
fn filter_status(filter: &str) -> Option<AblaufStatus> {
    match filter {
        "abgelaufen" => Some(AblaufStatus::Abgelaufen),
        "kritisch"   => Some(AblaufStatus::KritischBald),
        "warnung"    => Some(AblaufStatus::WarnungBald),
        "ok"         => Some(AblaufStatus::Ok),
        _            => None,
    }
}

/// Beschriftung wie auf dem Filter-Button.
fn filter_label(filter: &str) -> &'static str {
    match filter {
        "abgelaufen" => "Abgelaufen",
        "kritisch"   => "Bald kritisch",
        "warnung"    => "Warnung",
        "ok"         => "OK",
        _            => "Alle",
    }
}

/// Hinweis, falls der gerade gespeicherte Artikel wegen des aktiven Filters nicht in der
/// Tabelle erscheint (sonst sähe es so aus, als wäre nichts gespeichert worden).
fn hinweis_falls_ausgeblendet(a: &Artikel, heute: chrono::NaiveDate, filter: &str) -> String {
    match filter_status(filter) {
        Some(fs) if a.status(heute) != fs => format!(
            "„{}“ wurde gespeichert, ist im Filter „{}“ aber nicht sichtbar.",
            a.name,
            filter_label(filter)
        ),
        _ => String::new(),
    }
}

fn build_zeilen(artikel: &[Artikel], heute: chrono::NaiveDate, filter: &str) -> Vec<ArtikelZeile> {
    let filter_status = filter_status(filter);
    // Sichtbare Zeilen (nach Filter) stabil nach Ablaufdatum sortieren. Die Sortierung
    // passiert hier und nicht nur beim Speichern, weil die Jahres-Trennlinie davon abhängt
    // und `laden()` die CSV-Reihenfolge unverändert übernimmt.
    let mut sichtbar: Vec<&Artikel> = artikel
        .iter()
        .filter(|a| filter_status.as_ref().is_none_or(|fs| &a.status(heute) == fs))
        .collect();
    sichtbar.sort_by_key(|a| a.ablaufdatum);

    let mut vorheriges_jahr: Option<i32> = None;
    sichtbar
        .into_iter()
        .map(|a| {
            let status = a.status(heute);
            let jahr = a.ablaufdatum.year();
            // Erste sichtbare Zeile: nie eine Linie (vorheriges_jahr ist None).
            let jahreswechsel = vorheriges_jahr.is_some_and(|j| j != jahr);
            vorheriges_jahr = Some(jahr);
            ArtikelZeile {
                id: a.id,
                name: a.name.clone(),
                tage_bis_ablauf: (a.ablaufdatum - heute).num_days(),
                menge: a.menge.map(|m| m.to_string()).unwrap_or_default(),
                ablaufdatum: format_date(a.ablaufdatum),
                bemerkung: a.bemerkung.clone().unwrap_or_default(),
                status_class: status_class(&status).to_string(),
                jahreswechsel,
            }
        })
        .collect()
}

fn is_htmx(headers: &HeaderMap) -> bool {
    headers.get("HX-Request").is_some()
}

fn main_with_oob_modal_clear(
    artikel: &[Artikel],
    heute: chrono::NaiveDate,
    filter: &str,
    form: FormularState,
    fehler: String,
    hinweis: String,
) -> Response {
    let zeilen = build_zeilen(artikel, heute, filter);
    let t = MainContentTemplate { zeilen, filter: filter.to_string(), form, fehler, hinweis };
    match t.render() {
        Ok(mut html) => {
            html.push_str(r#"<div id="modal-container" hx-swap-oob="true"></div>"#);
            Html(html).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

struct Validierung {
    ablaufdatum: Option<chrono::NaiveDate>,
    menge: Option<u32>,
    fehler_name: String,
    fehler_ablaufdatum: String,
    fehler_menge: String,
}

impl Validierung {
    fn hat_fehler(&self) -> bool {
        !self.fehler_name.is_empty() || !self.fehler_ablaufdatum.is_empty() || !self.fehler_menge.is_empty()
    }
}

fn validate_form(form: &ArtikelFormData) -> Validierung {
    let mut fehler_name = String::new();
    let mut fehler_ablaufdatum = String::new();

    if form.name.trim().is_empty() {
        fehler_name = "Name darf nicht leer sein.".to_string();
    }

    let ablaufdatum = if form.ablaufdatum.trim().is_empty() {
        fehler_ablaufdatum = "Ablaufdatum ist Pflichtfeld.".to_string();
        None
    } else {
        let d = parse_datum(&form.ablaufdatum);
        if d.is_none() {
            fehler_ablaufdatum = "Ungültiges Datum (TT.MM.JJJJ).".to_string();
        }
        d
    };

    let (menge, fehler_menge) = match parse_menge(&form.menge) {
        Ok(m) => (m, String::new()),
        Err(e) => (None, e),
    };

    Validierung { ablaufdatum, menge, fehler_name, fehler_ablaufdatum, fehler_menge }
}

// ── Query/Form types ───────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub struct FilterParams {
    pub filter: Option<String>,
}

#[derive(Deserialize)]
pub struct ArtikelFormData {
    pub name: String,
    pub ablaufdatum: String,
    pub menge: String,
    pub bemerkung: String,
    /// Aktiver Filter der Seite (verstecktes Feld `#aktiver-filter`), damit die Antwort
    /// gefiltert bleibt. Fehlt er (alte Seite im Browser, curl), gilt `"alle"`.
    #[serde(default)]
    pub filter: String,
}

// ── Statische Dateien ──────────────────────────────────────────────────────────

/// HTMX lokal statt vom CDN: die App funktioniert ohne Internet (LAN/Pi).
/// Unveränderte Kopie, Prüfsumme siehe `static/README.md`.
const HTMX_JS: &str = include_str!("../../static/htmx-2.0.4.min.js");

/// `GET /static/htmx-2.0.4.min.js`. Die Version steht im Pfad, daher darf der Browser
/// die Datei unbegrenzt cachen (`immutable`).
pub async fn htmx_js() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        HTMX_JS,
    )
        .into_response()
}

// ── Handlers ───────────────────────────────────────────────────────────────────

pub async fn index(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<FilterParams>,
) -> Response {
    let artikel = state.artikel.read().await;
    let heute = chrono::Local::now().date_naive();
    let filter = normalisiere_filter(params.filter.as_deref()).to_string();
    let zeilen = build_zeilen(&artikel, heute, &filter);

    if is_htmx(&headers) {
        render(MainContentTemplate {
            zeilen,
            filter,
            form: FormularState::default(),
            fehler: String::new(),
            hinweis: String::new(),
        })
    } else {
        render(IndexTemplate {
            zeilen,
            filter,
            form: FormularState::default(),
            fehler: String::new(),
            hinweis: String::new(),
        })
    }
}

pub async fn hinzufuegen(
    State(state): State<AppState>,
    Form(form): Form<ArtikelFormData>,
) -> Response {
    let v = validate_form(&form);
    let filter = normalisiere_filter(Some(&form.filter));

    if v.hat_fehler() {
        let artikel = state.artikel.read().await;
        let heute = chrono::Local::now().date_naive();
        let zeilen = build_zeilen(&artikel, heute, filter);
        return render(MainContentTemplate {
            zeilen,
            filter: filter.to_string(),
            form: FormularState {
                name: form.name,
                ablaufdatum: form.ablaufdatum,
                menge: form.menge,
                bemerkung: form.bemerkung,
                fehler_name: v.fehler_name,
                fehler_ablaufdatum: v.fehler_ablaufdatum,
                fehler_menge: v.fehler_menge,
            },
            fehler: String::new(),
            hinweis: String::new(),
        });
    }

    let mut artikel = state.artikel.write().await;
    let new_id = artikel.iter().map(|a| a.id).max().unwrap_or(0) + 1;
    artikel.push(Artikel {
        id: new_id,
        name: form.name.trim().to_string(),
        // Kaufdatum wird nicht mehr im Formular erfasst: Einfügedatum verwenden.
        kaufdatum: Some(chrono::Local::now().date_naive()),
        menge: v.menge,
        ablaufdatum: v.ablaufdatum.unwrap(),
        bemerkung: if form.bemerkung.trim().is_empty() { None } else { Some(form.bemerkung.trim().to_string()) },
    });
    artikel.sort_by_key(|a| a.ablaufdatum);

    let fehler = speichern(&artikel).err().unwrap_or_default();
    let heute = chrono::Local::now().date_naive();
    let zeilen = build_zeilen(&artikel, heute, filter);
    let hinweis = artikel
        .iter()
        .find(|a| a.id == new_id)
        .map(|a| hinweis_falls_ausgeblendet(a, heute, filter))
        .unwrap_or_default();
    drop(artikel);

    render(MainContentTemplate {
        zeilen,
        filter: filter.to_string(),
        form: FormularState::default(),
        fehler,
        hinweis,
    })
}

pub async fn zeige_modal(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Response {
    let artikel = state.artikel.read().await;
    let Some(a) = artikel.iter().find(|a| a.id == id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    render(ModalTemplate {
        id: a.id,
        name: a.name.clone(),
        menge: a.menge.map(|m| m.to_string()).unwrap_or_default(),
        ablaufdatum: format_date(a.ablaufdatum),
        bemerkung: a.bemerkung.clone().unwrap_or_default(),
        fehler_name: String::new(),
        fehler_ablaufdatum: String::new(),
        fehler_menge: String::new(),
    })
}

pub async fn bearbeiten(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Form(form): Form<ArtikelFormData>,
) -> Response {
    let v = validate_form(&form);
    let filter = normalisiere_filter(Some(&form.filter));

    if v.hat_fehler() {
        let mut response = render(ModalTemplate {
            id,
            name: form.name,
            menge: form.menge,
            ablaufdatum: form.ablaufdatum,
            bemerkung: form.bemerkung,
            fehler_name: v.fehler_name,
            fehler_ablaufdatum: v.fehler_ablaufdatum,
            fehler_menge: v.fehler_menge,
        });
        response.headers_mut().insert("HX-Retarget", "#modal-container".parse().unwrap());
        response.headers_mut().insert("HX-Reswap", "innerHTML".parse().unwrap());
        return response;
    }

    let mut artikel = state.artikel.write().await;
    if let Some(a) = artikel.iter_mut().find(|a| a.id == id) {
        a.name = form.name.trim().to_string();
        // a.kaufdatum bleibt bewusst unverändert (nicht im Formular editierbar).
        a.menge = v.menge;
        a.ablaufdatum = v.ablaufdatum.unwrap();
        a.bemerkung = if form.bemerkung.trim().is_empty() { None } else { Some(form.bemerkung.trim().to_string()) };
    }
    artikel.sort_by_key(|a| a.ablaufdatum);

    let fehler = speichern(&artikel).err().unwrap_or_default();
    let heute = chrono::Local::now().date_naive();
    // Fällt der Artikel durch die Änderung aus dem Filter, verschwindet die Zeile: Hinweis zeigen.
    let hinweis = artikel
        .iter()
        .find(|a| a.id == id)
        .map(|a| hinweis_falls_ausgeblendet(a, heute, filter))
        .unwrap_or_default();
    main_with_oob_modal_clear(&artikel, heute, filter, FormularState::default(), fehler, hinweis)
}

pub async fn loeschen(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    // HTMX 2 schickt bei DELETE die Parameter (inkl. `filter` via hx-include) in der URL.
    Query(params): Query<FilterParams>,
) -> Response {
    let filter = normalisiere_filter(params.filter.as_deref());
    let mut artikel = state.artikel.write().await;
    artikel.retain(|a| a.id != id);

    let fehler = speichern(&artikel).err().unwrap_or_default();
    let heute = chrono::Local::now().date_naive();
    main_with_oob_modal_clear(&artikel, heute, filter, FormularState::default(), fehler, String::new())
}

#[cfg(test)]
mod tests_filter {
    use super::*;
    use chrono::NaiveDate;

    fn artikel(name: &str, ablauf: NaiveDate) -> Artikel {
        Artikel { id: 1, name: name.into(), kaufdatum: None, menge: None, ablaufdatum: ablauf, bemerkung: None }
    }

    #[test]
    fn filter_wird_normalisiert() {
        assert_eq!(normalisiere_filter(Some("warnung")), "warnung");
        assert_eq!(normalisiere_filter(Some(" ok ")), "ok");
        assert_eq!(normalisiere_filter(Some("")), "alle");
        assert_eq!(normalisiere_filter(Some("\"><script>")), "alle");
        assert_eq!(normalisiere_filter(None), "alle");
    }

    #[test]
    fn hinweis_nur_wenn_ausgeblendet() {
        let heute = NaiveDate::from_ymd_opt(2026, 11, 19).unwrap();
        let abgelaufen = artikel("Milch", NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        assert!(hinweis_falls_ausgeblendet(&abgelaufen, heute, "alle").is_empty());
        assert!(hinweis_falls_ausgeblendet(&abgelaufen, heute, "abgelaufen").is_empty());
        let h = hinweis_falls_ausgeblendet(&abgelaufen, heute, "ok");
        assert!(h.contains("Milch") && h.contains("„OK“"), "{h}");
    }

    #[test]
    fn build_zeilen_filtert() {
        let heute = NaiveDate::from_ymd_opt(2026, 11, 19).unwrap();
        let liste = vec![
            artikel("Alt", NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
            artikel("Neu", NaiveDate::from_ymd_opt(2030, 1, 1).unwrap()),
        ];
        assert_eq!(build_zeilen(&liste, heute, "alle").len(), 2);
        let ok = build_zeilen(&liste, heute, "ok");
        assert_eq!(ok.len(), 1);
        assert_eq!(ok[0].name, "Neu");
    }
}
