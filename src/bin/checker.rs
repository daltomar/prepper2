use ablaufdatum_tracker::artikel::{AblaufStatus, Artikel};
use ablaufdatum_tracker::csv_io::laden;
use chrono::{Local, NaiveDate};
use lettre::{
    message::{header::ContentType, Mailboxes},
    transport::smtp::authentication::Credentials,
    Message, SmtpTransport, Transport,
};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize, Default)]
struct Config {
    email: Option<EmailConfig>,
}

#[derive(Deserialize)]
struct EmailConfig {
    smtp_server: String,
    smtp_port: u16,
    username: String,
    password: String,
    from: String,
    to: String,
}

fn main() {
    match run() {
        Ok(()) => {}
        Err(e) => {
            log_schreiben(&format!("FEHLER: {}", e));
            eprintln!("Fehler: {}", e);
            std::process::exit(1);
        }
    }
}

fn run() -> Result<(), String> {
    let artikel = laden()?;
    let heute = Local::now().date_naive();

    let mut kritisch: Vec<Artikel> = artikel
        .into_iter()
        .filter(|a| {
            matches!(
                a.status(heute),
                AblaufStatus::KritischBald | AblaufStatus::Abgelaufen
            )
        })
        .collect();

    if kritisch.is_empty() {
        return Ok(());
    }

    kritisch.sort_by_key(|a| a.ablaufdatum);
    let anzahl = kritisch.len();

    let notify_result = std::process::Command::new("notify-send")
        .arg("--urgency=normal")
        .arg("--icon=dialog-warning")
        .arg("Ablaufdatum-Tracker")
        .arg(format!(
            "{} Artikel laufen bald ab oder sind abgelaufen",
            anzahl
        ))
        .status();

    match notify_result {
        Ok(status) if !status.success() => {
            log_schreiben("WARNUNG: notify-send schlug fehl");
        }
        Err(e) => {
            log_schreiben(&format!("WARNUNG: notify-send nicht gefunden: {}", e));
        }
        _ => {}
    }

    let config = config_laden();

    if let Some(cfg) = config {
        if let Some(email_cfg) = cfg.email {
            let body = email_body_erstellen(&kritisch, heute);
            match email_senden(&email_cfg, anzahl, &body) {
                Ok(()) => log_schreiben("OK: E-Mail gesendet"),
                Err(e) => log_schreiben(&format!("FEHLER E-Mail: {}", e)),
            }
        }
    }

    log_schreiben(&format!(
        "OK: {} kritische Artikel, Benachrichtigung gesendet",
        anzahl
    ));
    Ok(())
}

fn config_pfad() -> Option<PathBuf> {
    Some(
        dirs::config_dir()?
            .join("ablaufdatum-tracker")
            .join("config.toml"),
    )
}

fn config_laden() -> Option<Config> {
    let pfad = config_pfad()?;
    if !pfad.exists() {
        beispiel_config_erstellen();
        return None;
    }
    let inhalt = std::fs::read_to_string(&pfad).ok()?;
    toml::from_str(&inhalt).ok()
}

fn beispiel_config_erstellen() {
    let example_pfad = match dirs::config_dir() {
        Some(d) => d.join("ablaufdatum-tracker").join("config.toml.example"),
        None => return,
    };

    if let Some(parent) = example_pfad.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let example = r#"# Ablaufdatum-Tracker E-Mail-Konfiguration
# Kopiere diese Datei nach config.toml und passe die Werte an.
# Dateiberechtigungen auf 600 setzen: chmod 600 config.toml

[email]
smtp_server = "smtp.gmail.com"
smtp_port = 587
username = "dein@email.de"
password = "app-passwort"
from = "dein@email.de"
to = "empfaenger@email.de, zweiter@email.de"
"#;

    if std::fs::write(&example_pfad, example).is_ok() {
        eprintln!(
            "Hinweis: Beispielkonfiguration erstellt unter: {}\n\
             Kopiere sie nach config.toml und trage deine SMTP-Daten ein.",
            example_pfad.display()
        );
    }
}

fn email_body_erstellen(kritisch: &[Artikel], heute: NaiveDate) -> String {
    let abgelaufen: Vec<&Artikel> = kritisch
        .iter()
        .filter(|a| a.status(heute) == AblaufStatus::Abgelaufen)
        .collect();
    let bald: Vec<&Artikel> = kritisch
        .iter()
        .filter(|a| a.status(heute) == AblaufStatus::KritischBald)
        .collect();

    let mut body =
        String::from("Hallo,\n\nfolgende Artikel erfordern deine Aufmerksamkeit:\n");

    if !abgelaufen.is_empty() {
        body.push_str("\nABGELAUFEN:\n");
        for a in &abgelaufen {
            let tage = (heute - a.ablaufdatum).num_days();
            let tage_str = if tage == 1 {
                "1 Tag".to_string()
            } else {
                format!("{} Tagen", tage)
            };
            body.push_str(&format!(
                "  - {} (abgelaufen seit {}, {})\n",
                a.name,
                tage_str,
                a.ablaufdatum.format("%d.%m.%Y")
            ));
        }
    }

    if !bald.is_empty() {
        body.push_str("\nBALD ABLAUFEND (\u{2264} 2 Monate):\n");
        for a in &bald {
            let tage = (a.ablaufdatum - heute).num_days();
            let tage_str = if tage == 1 {
                "1 Tag".to_string()
            } else {
                format!("{} Tagen", tage)
            };
            body.push_str(&format!(
                "  - {} (noch {}, {})\n",
                a.name,
                tage_str,
                a.ablaufdatum.format("%d.%m.%Y")
            ));
        }
    }

    body.push_str(
        "\nÖffne Ablaufdatum-Tracker um Artikel zu aktualisieren oder zu löschen.\n",
    );
    body
}

fn email_senden(cfg: &EmailConfig, anzahl: usize, body: &str) -> Result<(), String> {
    let from = cfg
        .from
        .parse()
        .map_err(|e| format!("Ungültige Absenderadresse: {}", e))?;
    let to: Mailboxes = cfg
        .to
        .parse()
        .map_err(|e| format!("Ungültige Empfängeradresse: {}", e))?;

    let mut builder = Message::builder().from(from);
    for mailbox in to {
        builder = builder.to(mailbox);
    }
    let email = builder
        .subject(format!("[Rat-Apps] Prepper: {} Artikel prüfen", anzahl))
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string())
        .map_err(|e| e.to_string())?;

    let creds = Credentials::new(cfg.username.clone(), cfg.password.clone());

    let mailer = if cfg.smtp_port == 465 {
        SmtpTransport::relay(&cfg.smtp_server)
            .map_err(|e| e.to_string())?
            .port(cfg.smtp_port)
            .credentials(creds)
            .build()
    } else {
        SmtpTransport::starttls_relay(&cfg.smtp_server)
            .map_err(|e| e.to_string())?
            .port(cfg.smtp_port)
            .credentials(creds)
            .build()
    };

    mailer.send(&email).map_err(|e| e.to_string())?;
    Ok(())
}

fn log_schreiben(nachricht: &str) {
    let pfad = match dirs::data_local_dir() {
        Some(d) => d.join("ablaufdatum-tracker").join("checker.log"),
        None => return,
    };

    if let Some(parent) = pfad.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut zeilen: Vec<String> = std::fs::read_to_string(&pfad)
        .unwrap_or_default()
        .lines()
        .map(|l| l.to_string())
        .collect();

    let jetzt = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    zeilen.push(format!("[{}] {}", jetzt, nachricht));

    let start = zeilen.len().saturating_sub(50);
    let _ = std::fs::write(&pfad, zeilen[start..].join("\n") + "\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_feld_akzeptiert_mehrere_adressen() {
        let mehrere = "eins@example.com, zwei@example.com, drei@example.com";
        let mailboxes: Result<Mailboxes, _> = mehrere.parse();
        assert!(mailboxes.is_ok(), "Komma-getrennte Adressen müssen parsebar sein");
        let liste: Vec<_> = mailboxes.unwrap().into_iter().collect();
        assert_eq!(liste.len(), 3);
    }

    #[test]
    fn to_feld_akzeptiert_einzelne_adresse() {
        let einzeln = "empfaenger@example.com";
        let mailboxes: Result<Mailboxes, _> = einzeln.parse();
        assert!(mailboxes.is_ok());
        let liste: Vec<_> = mailboxes.unwrap().into_iter().collect();
        assert_eq!(liste.len(), 1);
    }

    #[test]
    fn to_feld_lehnt_ungueltige_adresse_ab() {
        let ungueltig = "keine-email";
        let mailboxes: Result<Mailboxes, _> = ungueltig.parse();
        assert!(mailboxes.is_err());
    }
}
