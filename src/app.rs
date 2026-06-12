use ablaufdatum_tracker::artikel::{AblaufStatus, Artikel};
use ablaufdatum_tracker::csv_io::laden;
use crate::ui::{edit_modal, formular, tabelle};
use chrono::Local;
use egui::Color32;

pub struct AppState {
    pub artikel: Vec<Artikel>,
    pub filter: Option<AblaufStatus>,
    pub fehler_meldung: Option<String>,

    // Formular-Felder
    pub neu_name: String,
    pub neu_ablaufdatum: String,
    pub neu_kaufdatum: String,
    pub neu_menge: String,
    pub neu_bemerkung: String,
    pub fehler_name: String,
    pub fehler_ablaufdatum: String,
    pub fehler_kaufdatum: String,

    // Modal-Zustand
    pub bearbeitungs_artikel: Option<Artikel>,
    pub modal_name: String,
    pub modal_ablaufdatum: String,
    pub modal_kaufdatum: String,
    pub modal_menge: String,
    pub modal_bemerkung: String,
    pub modal_fehler_name: String,
    pub modal_fehler_ablaufdatum: String,
    pub modal_fehler_kaufdatum: String,

    // Bestätigungs-Dialog
    pub loeschen_bestaetigung_id: Option<u64>,
}

impl AppState {
    pub fn modal_felder_init(&mut self) {
        if let Some(ref a) = self.bearbeitungs_artikel {
            self.modal_name = a.name.clone();
            self.modal_ablaufdatum = a.ablaufdatum.format("%d.%m.%Y").to_string();
            self.modal_kaufdatum = a
                .kaufdatum
                .map(|d| d.format("%d.%m.%Y").to_string())
                .unwrap_or_default();
            self.modal_menge = a.menge.clone().unwrap_or_default();
            self.modal_bemerkung = a.bemerkung.clone().unwrap_or_default();
        }
        self.modal_fehler_name.clear();
        self.modal_fehler_ablaufdatum.clear();
        self.modal_fehler_kaufdatum.clear();
    }

    pub fn modal_felder_leeren(&mut self) {
        self.modal_name.clear();
        self.modal_ablaufdatum.clear();
        self.modal_kaufdatum.clear();
        self.modal_menge.clear();
        self.modal_bemerkung.clear();
        self.modal_fehler_name.clear();
        self.modal_fehler_ablaufdatum.clear();
        self.modal_fehler_kaufdatum.clear();
    }
}

pub struct AblaufdatumTrackerApp {
    pub state: AppState,
}

impl AblaufdatumTrackerApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let (artikel, fehler) = match laden() {
            Ok(mut list) => {
                list.sort_by_key(|a| a.ablaufdatum);
                (list, None)
            }
            Err(e) => (Vec::new(), Some(e)),
        };

        Self {
            state: AppState {
                artikel,
                filter: None,
                fehler_meldung: fehler,
                neu_name: String::new(),
                neu_ablaufdatum: String::new(),
                neu_kaufdatum: String::new(),
                neu_menge: String::new(),
                neu_bemerkung: String::new(),
                fehler_name: String::new(),
                fehler_ablaufdatum: String::new(),
                fehler_kaufdatum: String::new(),
                bearbeitungs_artikel: None,
                modal_name: String::new(),
                modal_ablaufdatum: String::new(),
                modal_kaufdatum: String::new(),
                modal_menge: String::new(),
                modal_bemerkung: String::new(),
                modal_fehler_name: String::new(),
                modal_fehler_ablaufdatum: String::new(),
                modal_fehler_kaufdatum: String::new(),
                loeschen_bestaetigung_id: None,
            },
        }
    }
}

impl eframe::App for AblaufdatumTrackerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let _heute = Local::now().date_naive();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Ablaufdatum-Tracker");

            if let Some(ref fehler) = self.state.fehler_meldung.clone() {
                ui.colored_label(Color32::RED, format!("Fehler: {}", fehler));
            }

            ui.add_space(4.0);
            tabelle::zeige_filter(ui, &mut self.state);
            ui.add_space(4.0);

            egui::ScrollArea::vertical()
                .max_height(ui.available_height() - 200.0)
                .show(ui, |ui| {
                    tabelle::zeige_tabelle(ui, &mut self.state);
                });

            formular::zeige_formular(ui, &mut self.state);
        });

        edit_modal::zeige_modal(ctx, &mut self.state);
    }
}
