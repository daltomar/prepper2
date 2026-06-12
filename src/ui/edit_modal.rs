use crate::app::AppState;
use ablaufdatum_tracker::artikel::parse_datum;
use ablaufdatum_tracker::csv_io::speichern;
use egui::Color32;

pub fn zeige_modal(ctx: &egui::Context, state: &mut AppState) {
    if state.bearbeitungs_artikel.is_none() {
        return;
    }

    let mut offen = true;
    let mut speichern_geklickt = false;
    let mut loeschen_geklickt = false;
    let mut abbrechen = false;

    egui::Window::new("Artikel bearbeiten")
        .collapsible(false)
        .resizable(false)
        .min_width(400.0)
        .open(&mut offen)
        .show(ctx, |ui| {
            egui::Grid::new("modal_grid")
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Name*:");
                    ui.text_edit_singleline(&mut state.modal_name);
                    ui.end_row();

                    if !state.modal_fehler_name.is_empty() {
                        ui.label("");
                        ui.colored_label(Color32::RED, &state.modal_fehler_name);
                        ui.end_row();
                    }

                    ui.label("Ablaufdatum*:");
                    ui.text_edit_singleline(&mut state.modal_ablaufdatum);
                    ui.end_row();

                    if !state.modal_fehler_ablaufdatum.is_empty() {
                        ui.label("");
                        ui.colored_label(Color32::RED, &state.modal_fehler_ablaufdatum);
                        ui.end_row();
                    }

                    ui.label("Kaufdatum:");
                    ui.text_edit_singleline(&mut state.modal_kaufdatum);
                    ui.end_row();

                    if !state.modal_fehler_kaufdatum.is_empty() {
                        ui.label("");
                        ui.colored_label(Color32::RED, &state.modal_fehler_kaufdatum);
                        ui.end_row();
                    }

                    ui.label("Menge:");
                    ui.text_edit_singleline(&mut state.modal_menge);
                    ui.end_row();

                    ui.label("Bemerkung:");
                    ui.text_edit_singleline(&mut state.modal_bemerkung);
                    ui.end_row();
                });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Speichern").clicked() {
                    speichern_geklickt = true;
                }
                if ui.button("Löschen").clicked() {
                    loeschen_geklickt = true;
                }
                if ui.button("Abbrechen").clicked() {
                    abbrechen = true;
                }
            });
        });

    if !offen || abbrechen {
        state.bearbeitungs_artikel = None;
        state.modal_felder_leeren();
        return;
    }

    if loeschen_geklickt {
        if let Some(a) = &state.bearbeitungs_artikel {
            state.loeschen_bestaetigung_id = Some(a.id);
        }
    }

    if speichern_geklickt {
        state.modal_fehler_name.clear();
        state.modal_fehler_ablaufdatum.clear();
        state.modal_fehler_kaufdatum.clear();

        let mut ok = true;

        if state.modal_name.trim().is_empty() {
            state.modal_fehler_name = "Name darf nicht leer sein.".into();
            ok = false;
        }

        let ablaufdatum = parse_datum(&state.modal_ablaufdatum);
        if ablaufdatum.is_none() {
            state.modal_fehler_ablaufdatum = "Ungültiges Datum (TT.MM.JJJJ oder JJJJ-MM-TT).".into();
            ok = false;
        }

        let kaufdatum = if state.modal_kaufdatum.trim().is_empty() {
            None
        } else {
            let kd = parse_datum(&state.modal_kaufdatum);
            if kd.is_none() {
                state.modal_fehler_kaufdatum = "Ungültiges Datum (TT.MM.JJJJ oder JJJJ-MM-TT).".into();
                ok = false;
            }
            kd
        };

        if ok {
            if let Some(ref original) = state.bearbeitungs_artikel.clone() {
                if let Some(pos) = state.artikel.iter().position(|a| a.id == original.id) {
                    state.artikel[pos].name = state.modal_name.trim().to_string();
                    state.artikel[pos].ablaufdatum = ablaufdatum.unwrap();
                    state.artikel[pos].kaufdatum = kaufdatum;
                    state.artikel[pos].menge = if state.modal_menge.trim().is_empty() {
                        None
                    } else {
                        Some(state.modal_menge.trim().to_string())
                    };
                    state.artikel[pos].bemerkung = if state.modal_bemerkung.trim().is_empty() {
                        None
                    } else {
                        Some(state.modal_bemerkung.trim().to_string())
                    };
                }
            }

            state.artikel.sort_by_key(|a| a.ablaufdatum);

            match speichern(&state.artikel) {
                Ok(()) => state.fehler_meldung = None,
                Err(e) => state.fehler_meldung = Some(e),
            }

            state.bearbeitungs_artikel = None;
            state.modal_felder_leeren();
        }
    }

    zeige_loeschen_dialog(ctx, state);
}

fn zeige_loeschen_dialog(ctx: &egui::Context, state: &mut AppState) {
    let loeschen_id = match state.loeschen_bestaetigung_id {
        Some(id) => id,
        None => return,
    };

    let name = state
        .artikel
        .iter()
        .find(|a| a.id == loeschen_id)
        .map(|a| a.name.clone())
        .unwrap_or_default();

    let mut bestaetigt = false;
    let mut abgebrochen = false;

    egui::Window::new("Löschen bestätigen")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(format!("Artikel \"{}\" wirklich l\u{00F6}schen?", name));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Ja").clicked() {
                    bestaetigt = true;
                }
                if ui.button("Abbrechen").clicked() {
                    abgebrochen = true;
                }
            });
        });

    if bestaetigt {
        state.artikel.retain(|a| a.id != loeschen_id);
        match speichern(&state.artikel) {
            Ok(()) => state.fehler_meldung = None,
            Err(e) => state.fehler_meldung = Some(e),
        }
        state.loeschen_bestaetigung_id = None;
        state.bearbeitungs_artikel = None;
        state.modal_felder_leeren();
    } else if abgebrochen {
        state.loeschen_bestaetigung_id = None;
    }
}
