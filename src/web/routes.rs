use axum::{
    extract::{Form, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
};
use askama::Template;
use chrono::Datelike;
use serde::Deserialize;
use crate::{
    artikel::{AblaufStatus, Artikel, parse_datum},
    csv_io::speichern,
    web::AppState,
};

// ── View types ─────────────────────────────────────────────────────────────────

pub struct ArtikelZeile {
    pub id: u64,
    pub name: String,
    pub kaufdatum: String,
    pub menge: String,
    pub ablaufdatum: String,
    pub kaufdatum_eingabe: String,
    pub ablaufdatum_eingabe: String,
    pub bemerkung: String,
    pub status_class: String,
}

#[derive(Default)]
pub struct FormularState {
    pub name: String,
    pub ablaufdatum: String,
    pub kaufdatum: String,
    pub menge: String,
    pub bemerkung: String,
    pub fehler_name: String,
    pub fehler_ablaufdatum: String,
    pub fehler_kaufdatum: String,
}

// ── Templates ──────────────────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    zeilen: Vec<ArtikelZeile>,
    filter: String,
    form: FormularState,
    fehler: String,
}

#[derive(Template)]
#[template(path = "partials/main_content.html")]
struct MainContentTemplate {
    zeilen: Vec<ArtikelZeile>,
    filter: String,
    form: FormularState,
    fehler: String,
}

#[derive(Template)]
#[template(path = "partials/modal.html")]
struct ModalTemplate {
    id: u64,
    name: String,
    kaufdatum: String,
    menge: String,
    ablaufdatum: String,
    bemerkung: String,
    fehler_name: String,
    fehler_ablaufdatum: String,
    fehler_kaufdatum: String,
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

fn build_zeilen(artikel: &[Artikel], heute: chrono::NaiveDate, filter: &str) -> Vec<ArtikelZeile> {
    let filter_status: Option<AblaufStatus> = match filter {
        "abgelaufen" => Some(AblaufStatus::Abgelaufen),
        "kritisch"   => Some(AblaufStatus::KritischBald),
        "warnung"    => Some(AblaufStatus::WarnungBald),
        "ok"         => Some(AblaufStatus::Ok),
        _            => None,
    };
    artikel
        .iter()
        .filter(|a| filter_status.as_ref().is_none_or(|fs| &a.status(heute) == fs))
        .map(|a| {
            let status = a.status(heute);
            ArtikelZeile {
                id: a.id,
                name: a.name.clone(),
                kaufdatum: a.kaufdatum.map(format_date).unwrap_or_default(),
                menge: a.menge.clone().unwrap_or_default(),
                ablaufdatum: format_date(a.ablaufdatum),
                kaufdatum_eingabe: a.kaufdatum.map(format_date).unwrap_or_default(),
                ablaufdatum_eingabe: format_date(a.ablaufdatum),
                bemerkung: a.bemerkung.clone().unwrap_or_default(),
                status_class: status_class(&status).to_string(),
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
) -> Response {
    let zeilen = build_zeilen(artikel, heute, filter);
    let t = MainContentTemplate { zeilen, filter: filter.to_string(), form, fehler };
    match t.render() {
        Ok(mut html) => {
            html.push_str(r#"<div id="modal-container" hx-swap-oob="true"></div>"#);
            Html(html).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

fn validate_form(
    name: &str,
    ablaufdatum_str: &str,
    kaufdatum_str: &str,
) -> (Option<chrono::NaiveDate>, Option<chrono::NaiveDate>, String, String, String) {
    let mut fehler_name = String::new();
    let mut fehler_ablaufdatum = String::new();
    let mut fehler_kaufdatum = String::new();

    if name.trim().is_empty() {
        fehler_name = "Name darf nicht leer sein.".to_string();
    }

    let ablaufdatum = if ablaufdatum_str.trim().is_empty() {
        fehler_ablaufdatum = "Ablaufdatum ist Pflichtfeld.".to_string();
        None
    } else {
        let d = parse_datum(ablaufdatum_str);
        if d.is_none() {
            fehler_ablaufdatum = "Ungültiges Datum (TT.MM.JJJJ).".to_string();
        }
        d
    };

    let kaufdatum = if kaufdatum_str.trim().is_empty() {
        None
    } else {
        let d = parse_datum(kaufdatum_str);
        if d.is_none() {
            fehler_kaufdatum = "Ungültiges Datum (TT.MM.JJJJ).".to_string();
        }
        d
    };

    (ablaufdatum, kaufdatum, fehler_name, fehler_ablaufdatum, fehler_kaufdatum)
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
    pub kaufdatum: String,
    pub menge: String,
    pub bemerkung: String,
}

// ── Handlers ───────────────────────────────────────────────────────────────────

pub async fn index(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<FilterParams>,
) -> Response {
    let artikel = state.artikel.read().await;
    let heute = chrono::Local::now().date_naive();
    let filter = params.filter.as_deref().unwrap_or("alle").to_string();
    let zeilen = build_zeilen(&artikel, heute, &filter);

    if is_htmx(&headers) {
        render(MainContentTemplate {
            zeilen,
            filter,
            form: FormularState::default(),
            fehler: String::new(),
        })
    } else {
        render(IndexTemplate {
            zeilen,
            filter,
            form: FormularState::default(),
            fehler: String::new(),
        })
    }
}

pub async fn hinzufuegen(
    State(state): State<AppState>,
    Form(form): Form<ArtikelFormData>,
) -> Response {
    let (ablaufdatum, kaufdatum, fn_, fa, fk) =
        validate_form(&form.name, &form.ablaufdatum, &form.kaufdatum);

    if !fn_.is_empty() || !fa.is_empty() || !fk.is_empty() {
        let artikel = state.artikel.read().await;
        let heute = chrono::Local::now().date_naive();
        let zeilen = build_zeilen(&artikel, heute, "alle");
        return render(MainContentTemplate {
            zeilen,
            filter: "alle".to_string(),
            form: FormularState {
                name: form.name,
                ablaufdatum: form.ablaufdatum,
                kaufdatum: form.kaufdatum,
                menge: form.menge,
                bemerkung: form.bemerkung,
                fehler_name: fn_,
                fehler_ablaufdatum: fa,
                fehler_kaufdatum: fk,
            },
            fehler: String::new(),
        });
    }

    let mut artikel = state.artikel.write().await;
    let new_id = artikel.iter().map(|a| a.id).max().unwrap_or(0) + 1;
    artikel.push(Artikel {
        id: new_id,
        name: form.name.trim().to_string(),
        kaufdatum,
        menge: if form.menge.trim().is_empty() { None } else { Some(form.menge.trim().to_string()) },
        ablaufdatum: ablaufdatum.unwrap(),
        bemerkung: if form.bemerkung.trim().is_empty() { None } else { Some(form.bemerkung.trim().to_string()) },
    });
    artikel.sort_by_key(|a| a.ablaufdatum);

    let fehler = speichern(&artikel).err().unwrap_or_default();
    let heute = chrono::Local::now().date_naive();
    let zeilen = build_zeilen(&artikel, heute, "alle");
    drop(artikel);

    render(MainContentTemplate {
        zeilen,
        filter: "alle".to_string(),
        form: FormularState::default(),
        fehler,
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
        kaufdatum: a.kaufdatum.map(format_date).unwrap_or_default(),
        menge: a.menge.clone().unwrap_or_default(),
        ablaufdatum: format_date(a.ablaufdatum),
        bemerkung: a.bemerkung.clone().unwrap_or_default(),
        fehler_name: String::new(),
        fehler_ablaufdatum: String::new(),
        fehler_kaufdatum: String::new(),
    })
}

pub async fn bearbeiten(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Form(form): Form<ArtikelFormData>,
) -> Response {
    let (ablaufdatum, kaufdatum, fn_, fa, fk) =
        validate_form(&form.name, &form.ablaufdatum, &form.kaufdatum);

    if !fn_.is_empty() || !fa.is_empty() || !fk.is_empty() {
        let mut response = render(ModalTemplate {
            id,
            name: form.name,
            kaufdatum: form.kaufdatum,
            menge: form.menge,
            ablaufdatum: form.ablaufdatum,
            bemerkung: form.bemerkung,
            fehler_name: fn_,
            fehler_ablaufdatum: fa,
            fehler_kaufdatum: fk,
        });
        response.headers_mut().insert("HX-Retarget", "#modal-container".parse().unwrap());
        response.headers_mut().insert("HX-Reswap", "innerHTML".parse().unwrap());
        return response;
    }

    let mut artikel = state.artikel.write().await;
    if let Some(a) = artikel.iter_mut().find(|a| a.id == id) {
        a.name = form.name.trim().to_string();
        a.kaufdatum = kaufdatum;
        a.menge = if form.menge.trim().is_empty() { None } else { Some(form.menge.trim().to_string()) };
        a.ablaufdatum = ablaufdatum.unwrap();
        a.bemerkung = if form.bemerkung.trim().is_empty() { None } else { Some(form.bemerkung.trim().to_string()) };
    }
    artikel.sort_by_key(|a| a.ablaufdatum);

    let fehler = speichern(&artikel).err().unwrap_or_default();
    let heute = chrono::Local::now().date_naive();
    main_with_oob_modal_clear(&artikel, heute, "alle", FormularState::default(), fehler)
}

pub async fn loeschen(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Response {
    let mut artikel = state.artikel.write().await;
    artikel.retain(|a| a.id != id);

    let fehler = speichern(&artikel).err().unwrap_or_default();
    let heute = chrono::Local::now().date_naive();
    main_with_oob_modal_clear(&artikel, heute, "alle", FormularState::default(), fehler)
}
