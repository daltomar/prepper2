use crate::app::AppState;
use ablaufdatum_tracker::artikel::{AblaufStatus, Artikel};
use chrono::{Local, NaiveDate};
use egui::{Color32, RichText};
use egui_extras::{Column, TableBuilder};

const FARBE_OK_BG: Color32 = Color32::WHITE;
const FARBE_OK_TEXT: Color32 = Color32::BLACK;
const FARBE_WARNUNG_BG: Color32 = Color32::from_rgb(0xFF, 0xF1, 0x76);
const FARBE_WARNUNG_TEXT: Color32 = Color32::BLACK;
const FARBE_KRITISCH_BG: Color32 = Color32::from_rgb(0xFF, 0xF1, 0x76);
const FARBE_KRITISCH_TEXT: Color32 = Color32::from_rgb(0xD3, 0x2F, 0x2F);
const FARBE_ABGELAUFEN_BG: Color32 = Color32::from_rgb(0xDC, 0x14, 0x3C);
const FARBE_ABGELAUFEN_TEXT: Color32 = Color32::WHITE;

fn farben(artikel: &Artikel, heute: NaiveDate) -> (Color32, Color32) {
    match artikel.status(heute) {
        AblaufStatus::Ok => (FARBE_OK_BG, FARBE_OK_TEXT),
        AblaufStatus::WarnungBald => (FARBE_WARNUNG_BG, FARBE_WARNUNG_TEXT),
        AblaufStatus::KritischBald => (FARBE_KRITISCH_BG, FARBE_KRITISCH_TEXT),
        AblaufStatus::Abgelaufen => (FARBE_ABGELAUFEN_BG, FARBE_ABGELAUFEN_TEXT),
    }
}

struct ZeilenDaten {
    bg: Color32,
    fg: Color32,
    name: String,
    kaufdatum: String,
    menge: String,
    ablaufdatum: String,
    bemerkung: String,
    artikel: Artikel,
}

pub fn zeige_tabelle(ui: &mut egui::Ui, state: &mut AppState) {
    let heute = Local::now().date_naive();

    let zeilen: Vec<ZeilenDaten> = state
        .artikel
        .iter()
        .filter(|a| match &state.filter {
            None => true,
            Some(f) => &a.status(heute) == f,
        })
        .map(|a| {
            let (bg, fg) = farben(a, heute);
            ZeilenDaten {
                bg,
                fg,
                name: a.name.clone(),
                kaufdatum: a
                    .kaufdatum
                    .map(|d| d.format("%d.%m.%Y").to_string())
                    .unwrap_or_default(),
                menge: a.menge.clone().unwrap_or_default(),
                ablaufdatum: a.ablaufdatum.format("%d.%m.%Y").to_string(),
                bemerkung: a.bemerkung.clone().unwrap_or_default(),
                artikel: a.clone(),
            }
        })
        .collect();

    let verfuegbare_breite = ui.available_width();
    let mut doppelklick: Option<Artikel> = None;

    TableBuilder::new(ui)
        .striped(false)
        .resizable(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::initial(verfuegbare_breite * 0.25).clip(true))
        .column(Column::initial(verfuegbare_breite * 0.15).clip(true))
        .column(Column::initial(verfuegbare_breite * 0.12).clip(true))
        .column(Column::initial(verfuegbare_breite * 0.15).clip(true))
        .column(Column::remainder().clip(true))
        .min_scrolled_height(0.0)
        .header(24.0, |mut header| {
            header.col(|ui| { ui.strong("Name"); });
            header.col(|ui| { ui.strong("Kaufdatum"); });
            header.col(|ui| { ui.strong("Menge"); });
            header.col(|ui| { ui.strong("Ablaufdatum"); });
            header.col(|ui| { ui.strong("Bemerkung"); });
        })
        .body(|mut body| {
            for zeile in &zeilen {
                let bg = zeile.bg;
                let fg = zeile.fg;
                let artikel_id = zeile.artikel.id;

                body.row(22.0, |mut row| {
                    let mut zeile_doppelklick = false;

                    row.col(|ui| {
                        let rect = ui.available_rect_before_wrap();
                        ui.painter().rect_filled(rect, 0.0, bg);
                        ui.label(RichText::new(&zeile.name).color(fg));
                        if ui.interact(rect, egui::Id::new(("c0", artikel_id)), egui::Sense::click()).double_clicked() {
                            zeile_doppelklick = true;
                        }
                    });

                    row.col(|ui| {
                        let rect = ui.available_rect_before_wrap();
                        ui.painter().rect_filled(rect, 0.0, bg);
                        ui.label(RichText::new(&zeile.kaufdatum).color(fg));
                        if ui.interact(rect, egui::Id::new(("c1", artikel_id)), egui::Sense::click()).double_clicked() {
                            zeile_doppelklick = true;
                        }
                    });

                    row.col(|ui| {
                        let rect = ui.available_rect_before_wrap();
                        ui.painter().rect_filled(rect, 0.0, bg);
                        ui.label(RichText::new(&zeile.menge).color(fg));
                        if ui.interact(rect, egui::Id::new(("c2", artikel_id)), egui::Sense::click()).double_clicked() {
                            zeile_doppelklick = true;
                        }
                    });

                    row.col(|ui| {
                        let rect = ui.available_rect_before_wrap();
                        ui.painter().rect_filled(rect, 0.0, bg);
                        ui.label(RichText::new(&zeile.ablaufdatum).color(fg));
                        if ui.interact(rect, egui::Id::new(("c3", artikel_id)), egui::Sense::click()).double_clicked() {
                            zeile_doppelklick = true;
                        }
                    });

                    row.col(|ui| {
                        let rect = ui.available_rect_before_wrap();
                        ui.painter().rect_filled(rect, 0.0, bg);
                        ui.label(RichText::new(&zeile.bemerkung).color(fg));
                        if ui.interact(rect, egui::Id::new(("c4", artikel_id)), egui::Sense::click()).double_clicked() {
                            zeile_doppelklick = true;
                        }
                    });

                    if zeile_doppelklick {
                        doppelklick = Some(zeile.artikel.clone());
                    }
                });
            }
        });

    if let Some(a) = doppelklick {
        state.bearbeitungs_artikel = Some(a);
        state.modal_felder_init();
    }
}

pub fn zeige_filter(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        ui.label("Filter:");

        let filter_optionen: &[(Option<AblaufStatus>, &str)] = &[
            (None, "Alle"),
            (Some(AblaufStatus::Abgelaufen), "Abgelaufen"),
            (Some(AblaufStatus::KritischBald), "Bald kritisch"),
            (Some(AblaufStatus::WarnungBald), "Warnung"),
            (Some(AblaufStatus::Ok), "OK"),
        ];

        for (opt, label) in filter_optionen {
            let aktiv = &state.filter == opt;
            if ui.selectable_label(aktiv, *label).clicked() {
                state.filter = opt.clone();
            }
        }
    });
}
