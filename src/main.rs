use ablaufdatum_tracker::{
    csv_io::{csv_pfad, laden_aus},
    web::{router, AppState},
};

#[tokio::main]
async fn main() {
    let password = std::env::var("ABLAUFDATUM_PASSWORD").unwrap_or_else(|_| {
        eprintln!("Fehler: Umgebungsvariable ABLAUFDATUM_PASSWORD ist nicht gesetzt.");
        std::process::exit(2);
    });

    let csv_pfad = csv_pfad().unwrap_or_else(|e| {
        eprintln!("Fehler: Pfad der Artikel-CSV nicht bestimmbar: {e}");
        std::process::exit(2);
    });

    // Nicht mit leerer Liste weitermachen: das nächste Speichern würde die Datei überschreiben.
    // Exit-Code 2 = Daten-/Konfigurationsfehler; systemd startet dann nicht neu
    // (RestartPreventExitStatus=2 in den Service-Units).
    let artikel = laden_aus(&csv_pfad).unwrap_or_else(|e| {
        eprintln!("Fehler: Artikel-CSV konnte nicht geladen werden: {e}");
        eprintln!(
            "Der Server startet nicht, damit die Datei nicht überschrieben wird. \
             Bitte die Datei korrigieren (oder aus einem Backup wiederherstellen) \
             und den Dienst neu starten."
        );
        std::process::exit(2);
    });

    let state = AppState::neu(artikel, password, csv_pfad);
    let app = router(state);

    let addr = std::env::var("ABLAUFDATUM_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8085".to_string());
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap_or_else(|e| {
        eprintln!("Fehler: Konnte nicht an {addr} binden: {e}");
        std::process::exit(1);
    });
    println!("Ablaufdatum-Tracker läuft auf http://{addr}");
    axum::serve(listener, app).await.unwrap();
}
