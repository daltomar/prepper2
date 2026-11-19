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

    let has_display = std::env::var("DISPLAY").is_ok()
        || std::env::var("WAYLAND_DISPLAY").is_ok();
    let mut desktop = Versand::Uebersprungen;
    if has_display {
        let notify_result = std::process::Command::new("notify-send")
            .arg("--urgency=normal")
            .arg("--icon=dialog-warning")
            .arg("Ablaufdatum-Tracker")
            .arg(format!(
                "{} Artikel laufen bald ab oder sind abgelaufen",
                anzahl
            ))
            .status();
        desktop = match notify_result {
            Ok(status) if status.success() => Versand::Gesendet,
            Ok(_) => {
                log_schreiben("WARNUNG: notify-send schlug fehl");
                Versand::Fehler
            }
            Err(e) => {
                log_schreiben(&format!("WARNUNG: notify-send nicht gefunden: {}", e));
                Versand::Fehler
            }
        };
    }

    let email = match config_laden() {
        Ok(Some(Config { email: Some(email_cfg) })) => {
            let body = email_body_erstellen(&kritisch, heute);
            match email_senden(&email_cfg, anzahl, &body) {
                Ok(()) => Versand::Gesendet,
                Err(e) => {
                    log_schreiben(&format!("FEHLER E-Mail: {}", e));
                    Versand::Fehler
                }
            }
        }
        Ok(_) => Versand::Uebersprungen,
        Err(e) => {
            // Eine vorhandene, aber kaputte config.toml soll nicht still ignoriert werden:
            // sonst fehlt die E-Mail, ohne dass es irgendwo steht.
            log_schreiben(&format!("FEHLER Konfiguration: {}", e));
            Versand::Fehler
        }
    };

    log_schreiben(&zusammenfassung(anzahl, desktop, email));
    Ok(())
}

/// Ergebnis eines Benachrichtigungskanals.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Versand {
    Gesendet,
    Fehler,
    /// Nicht versucht: kein Display bzw. keine `[email]`-Konfiguration.
    Uebersprungen,
}

/// Abschlusszeile fürs Log. Sagt, was wirklich passiert ist (früher stand dort immer
/// „Benachrichtigung gesendet“, auch wenn headless ohne E-Mail-Konfiguration nichts rausging).
/// `OK` nur, wenn mindestens ein Kanal gesendet hat und keiner fehlgeschlagen ist.
fn zusammenfassung(anzahl: usize, desktop: Versand, email: Versand) -> String {
    let desktop_text = match desktop {
        Versand::Gesendet => "Desktop-Benachrichtigung gesendet",
        Versand::Fehler => "Desktop-Benachrichtigung fehlgeschlagen",
        Versand::Uebersprungen => "keine Desktop-Benachrichtigung (kein Display)",
    };
    let email_text = match email {
        Versand::Gesendet => "E-Mail gesendet",
        Versand::Fehler => "E-Mail fehlgeschlagen",
        Versand::Uebersprungen => "keine E-Mail (nicht konfiguriert)",
    };
    let irgendwas_gesendet = desktop == Versand::Gesendet || email == Versand::Gesendet;
    let fehler = desktop == Versand::Fehler || email == Versand::Fehler;
    let stufe = if irgendwas_gesendet && !fehler { "OK" } else { "WARNUNG" };
    let hinweis = if irgendwas_gesendet { "" } else { " – niemand wurde benachrichtigt" };
    format!("{stufe}: {anzahl} kritische Artikel; {desktop_text}; {email_text}{hinweis}")
}

fn config_pfad() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("ABLAUFDATUM_CONFIG") {
        return Some(PathBuf::from(p));
    }
    Some(
        dirs::config_dir()?
            .join("ablaufdatum-tracker")
            .join("config.toml"),
    )
}

/// `Ok(None)`: keine config.toml vorhanden (kein Fehler, E-Mail ist optional).
/// `Err`: Datei vorhanden, aber nicht lesbar oder kein gültiges TOML.
fn config_laden() -> Result<Option<Config>, String> {
    let custom = std::env::var("ABLAUFDATUM_CONFIG").ok();
    let Some(pfad) = config_pfad() else {
        return Ok(None);
    };
    if !pfad.exists() {
        // Only try to write the example template when using the default user path,
        // not when an explicit path was provided (e.g. by a system service).
        if custom.is_none() {
            beispiel_config_erstellen();
        }
        return Ok(None);
    }
    let inhalt = std::fs::read_to_string(&pfad)
        .map_err(|e| format!("{}: {}", pfad.display(), e))?;
    config_parsen(&inhalt).map_err(|e| format!("{}: {}", pfad.display(), e))
}

fn config_parsen(inhalt: &str) -> Result<Option<Config>, String> {
    toml::from_str(inhalt).map(Some).map_err(|e| {
        // Die Standard-Meldung von toml ist mehrzeilig (mit Code-Ausschnitt); fürs Log kürzen.
        let zeile = e
            .span()
            .map(|r| inhalt[..r.start.min(inhalt.len())].matches('\n').count() + 1);
        let text = e.message().split_whitespace().collect::<Vec<_>>().join(" ");
        match zeile {
            Some(z) => format!("Zeile {z}: {text}"),
            None => text,
        }
    })
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

fn log_pfad() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("ABLAUFDATUM_LOG") {
        return Some(PathBuf::from(p));
    }
    Some(dirs::data_local_dir()?.join("ablaufdatum-tracker").join("checker.log"))
}

fn log_schreiben(nachricht: &str) {
    let pfad = match log_pfad() {
        Some(p) => p,
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
    // Ein Eintrag = eine Zeile (das Log wird zeilenweise auf 50 Einträge gekürzt).
    let einzeilig = nachricht.split_whitespace().collect::<Vec<_>>().join(" ");
    zeilen.push(format!("[{}] {}", jetzt, einzeilig));

    let start = zeilen.len().saturating_sub(50);
    let _ = std::fs::write(&pfad, zeilen[start..].join("\n") + "\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zusammenfassung_headless_ohne_email_sagt_nicht_gesendet() {
        let z = zusammenfassung(3, Versand::Uebersprungen, Versand::Uebersprungen);
        assert!(z.starts_with("WARNUNG: 3 kritische Artikel"), "{z}");
        assert!(z.contains("niemand wurde benachrichtigt"), "{z}");
        assert!(!z.contains(" gesendet;") && !z.ends_with(" gesendet"), "{z}");
    }

    #[test]
    fn zusammenfassung_email_gesendet_ist_ok() {
        let z = zusammenfassung(2, Versand::Uebersprungen, Versand::Gesendet);
        assert_eq!(
            z,
            "OK: 2 kritische Artikel; keine Desktop-Benachrichtigung (kein Display); E-Mail gesendet"
        );
    }

    #[test]
    fn zusammenfassung_teilfehler_ist_warnung() {
        let z = zusammenfassung(1, Versand::Gesendet, Versand::Fehler);
        assert!(z.starts_with("WARNUNG:") && z.contains("E-Mail fehlgeschlagen"), "{z}");
        assert!(!z.contains("niemand"), "{z}");
    }

    #[test]
    fn config_ohne_email_sektion_und_kaputte_config() {
        assert!(config_parsen("").unwrap().unwrap().email.is_none());
        let ok = config_parsen(
            "[email]\nsmtp_server = \"s\"\nsmtp_port = 587\nusername = \"u\"\n\
             password = \"p\"\nfrom = \"a@b.de\"\nto = \"c@d.de\"\n",
        )
        .unwrap()
        .unwrap();
        assert_eq!(ok.email.unwrap().smtp_port, 587);
        // Tippfehler / fehlendes Feld → Fehler statt stillem Ignorieren
        assert!(config_parsen("[email]\nsmtp_server = \"s\"\n").is_err());
        let e = config_parsen("# Kommentar\n[email\n").err().expect("Fehler erwartet");
        assert!(e.starts_with("Zeile 2:") && !e.contains('\n'), "{e}");
    }

    #[test]
    fn email_body_tage_und_abschnitte() {
        let heute = NaiveDate::from_ymd_opt(2026, 11, 19).unwrap();
        let a = |name: &str, d: NaiveDate| Artikel {
            id: 0, name: name.into(), kaufdatum: None, menge: None, ablaufdatum: d, bemerkung: None,
        };
        let liste = vec![
            a("Milch", NaiveDate::from_ymd_opt(2026, 11, 18).unwrap()),
            a("Brot", NaiveDate::from_ymd_opt(2026, 11, 9).unwrap()),
            a("Reis", NaiveDate::from_ymd_opt(2026, 11, 20).unwrap()),
        ];
        let body = email_body_erstellen(&liste, heute);
        assert!(body.contains("ABGELAUFEN:"), "{body}");
        assert!(body.contains("Milch (abgelaufen seit 1 Tag, 18.11.2026)"), "{body}");
        assert!(body.contains("Brot (abgelaufen seit 10 Tagen, 09.11.2026)"), "{body}");
        assert!(body.contains("BALD ABLAUFEND"), "{body}");
        assert!(body.contains("Reis (noch 1 Tag, 20.11.2026)"), "{body}");
    }

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
