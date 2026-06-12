use crate::app::AppState;
use ablaufdatum_tracker::artikel::{parse_datum, Artikel};
use ablaufdatum_tracker::csv_io::speichern;
use egui::Color32;

pub fn zeige_formular(ui: &mut egui::Ui, state: &mut AppState) {
    ui.separator();
    ui.heading("Neuen Artikel hinzufügen");

    let h = ui.spacing().interact_size.y;
    egui::Grid::new("formular_grid")
        .num_columns(4)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            ui.label("Name*:");
            ui.add_sized([400.0, h], egui::TextEdit::singleline(&mut state.neu_name));
            ui.label("Ablaufdatum*:");
            ui.add_sized([100.0, h], egui::TextEdit::singleline(&mut state.neu_ablaufdatum));
            ui.end_row();

            ui.label("");
            if !state.fehler_name.is_empty() {
                ui.colored_label(Color32::RED, &state.fehler_name);
            } else {
                ui.label("");
            }
            ui.label("");
            if !state.fehler_ablaufdatum.is_empty() {
                ui.colored_label(Color32::RED, &state.fehler_ablaufdatum);
            } else {
                ui.label("");
            }
            ui.end_row();

            ui.label("Kaufdatum:");
            ui.add_sized([100.0, h], egui::TextEdit::singleline(&mut state.neu_kaufdatum));
            ui.label("Menge:");
            ui.add_sized([50.0, h], egui::TextEdit::singleline(&mut state.neu_menge));
            ui.end_row();

            ui.label("");
            if !state.fehler_kaufdatum.is_empty() {
                ui.colored_label(Color32::RED, &state.fehler_kaufdatum);
            } else {
                ui.label("");
            }
            ui.label("");
            ui.label("");
            ui.end_row();

            ui.label("Bemerkung:");
            ui.add_sized([450.0, h], egui::TextEdit::singleline(&mut state.neu_bemerkung));
            ui.label("");
            ui.label("");
            ui.end_row();
        });

    ui.add_space(4.0);

    if ui.button("Hinzufügen").clicked() {
        state.fehler_name.clear();
        state.fehler_ablaufdatum.clear();
        state.fehler_kaufdatum.clear();

        let mut ok = true;

        if state.neu_name.trim().is_empty() {
            state.fehler_name = "Name darf nicht leer sein.".into();
            ok = false;
        }

        let ablaufdatum = parse_datum(&state.neu_ablaufdatum);
        if ablaufdatum.is_none() {
            state.fehler_ablaufdatum = "Ungültiges Datum (TT.MM.JJJJ oder JJJJ-MM-TT).".into();
            ok = false;
        }

        let kaufdatum = if state.neu_kaufdatum.trim().is_empty() {
            None
        } else {
            let kd = parse_datum(&state.neu_kaufdatum);
            if kd.is_none() {
                state.fehler_kaufdatum = "Ungültiges Datum (TT.MM.JJJJ oder JJJJ-MM-TT).".into();
                ok = false;
            }
            kd
        };

        if ok {
            let neue_id = state.artikel.iter().map(|a| a.id).max().unwrap_or(0) + 1;
            let neuer = Artikel {
                id: neue_id,
                name: state.neu_name.trim().to_string(),
                kaufdatum,
                menge: if state.neu_menge.trim().is_empty() {
                    None
                } else {
                    Some(state.neu_menge.trim().to_string())
                },
                ablaufdatum: ablaufdatum.unwrap(),
                bemerkung: if state.neu_bemerkung.trim().is_empty() {
                    None
                } else {
                    Some(state.neu_bemerkung.trim().to_string())
                },
            };

            state.artikel.push(neuer);
            state.artikel.sort_by_key(|a| a.ablaufdatum);

            match speichern(&state.artikel) {
                Ok(()) => state.fehler_meldung = None,
                Err(e) => state.fehler_meldung = Some(e),
            }

            state.neu_name.clear();
            state.neu_ablaufdatum.clear();
            state.neu_kaufdatum.clear();
            state.neu_menge.clear();
            state.neu_bemerkung.clear();
        }
    }
}
