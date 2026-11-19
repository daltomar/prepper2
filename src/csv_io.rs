use crate::artikel::{parse_menge, Artikel};
use chrono::NaiveDate;
use std::path::PathBuf;

pub fn csv_pfad() -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("ABLAUFDATUM_CSV") {
        return Ok(PathBuf::from(p));
    }
    let data_dir = dirs::data_local_dir()
        .ok_or_else(|| "Kein lokales Datenverzeichnis gefunden".to_string())?
        .join("ablaufdatum-tracker");
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    Ok(data_dir.join("artikel.csv"))
}

/// Erwartete CSV-Kopfzeile (Spaltenreihenfolge).
pub const KOPFZEILE: [&str; 5] = ["name", "kaufdatum", "menge", "ablaufdatum", "bemerkung"];

/// Lädt alle Artikel aus der CSV-Datei. Fehlt die Datei, ist das Ergebnis eine leere Liste.
///
/// Jede Zeile, die nicht verlustfrei gelesen werden kann, ist ein **Fehler** (kein stilles
/// Überspringen): Der Aufrufer darf in diesem Fall nicht mit einer leeren/unvollständigen
/// Liste weiterarbeiten, sonst überschreibt das nächste `speichern()` die Datei.
pub fn laden() -> Result<Vec<Artikel>, String> {
    let pfad = csv_pfad()?;
    if !pfad.exists() {
        return Ok(Vec::new());
    }
    let datei = std::fs::File::open(&pfad).map_err(|e| format!("{}: {e}", pfad.display()))?;
    lesen(datei).map_err(|e| format!("{}: {e}", pfad.display()))
}

/// Parst CSV-Daten (Format siehe [`KOPFZEILE`]). Getrennt von `laden()`, damit es testbar ist.
pub fn lesen<R: std::io::Read>(quelle: R) -> Result<Vec<Artikel>, String> {
    let mut reader = csv::Reader::from_reader(quelle);

    let kopf = reader.headers().map_err(|e| e.to_string())?.clone();
    if kopf.is_empty() {
        // Komplett leere Datei (0 Byte): nichts zu verlieren.
        return Ok(Vec::new());
    }
    if !kopf.iter().map(str::trim).eq(KOPFZEILE) {
        return Err(format!(
            "Zeile 1: unerwartete Kopfzeile '{}', erwartet '{}'",
            kopf.iter().collect::<Vec<_>>().join(","),
            KOPFZEILE.join(",")
        ));
    }

    let mut artikel = Vec::new();

    for (idx, ergebnis) in reader.records().enumerate() {
        let record = ergebnis.map_err(|e| e.to_string())?;
        let zeile = record.position().map(|p| p.line()).unwrap_or(idx as u64 + 2);

        // Zeilen ohne jeden Inhalt (z. B. ",,,,") enthalten keine Daten → überspringen.
        if record.iter().all(|f| f.trim().is_empty()) {
            continue;
        }

        let name = record.get(0).unwrap_or("").to_string();
        if name.trim().is_empty() {
            return Err(format!("Zeile {zeile}: Name fehlt"));
        }

        let kaufdatum_str = record.get(1).unwrap_or("");
        let kaufdatum = if kaufdatum_str.is_empty() {
            None
        } else {
            Some(
                NaiveDate::parse_from_str(kaufdatum_str, "%Y-%m-%d")
                    .map_err(|_| format!("Zeile {zeile}: ungültiges Kaufdatum '{kaufdatum_str}'"))?,
            )
        };

        let ablaufdatum_str = record.get(3).unwrap_or("");
        let ablaufdatum = NaiveDate::parse_from_str(ablaufdatum_str, "%Y-%m-%d")
            .map_err(|_| format!("Zeile {zeile}: ungültiges Ablaufdatum '{ablaufdatum_str}'"))?;

        let mut bemerkung = record
            .get(4)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        // Menge war früher Freitext. Ungültige Altwerte (z. B. "500 g", "0") nicht
        // verwerfen, sondern in die Bemerkung übernehmen; persistiert beim nächsten Speichern.
        let menge_roh = record.get(2).unwrap_or("").trim();
        let menge = match parse_menge(menge_roh) {
            Ok(m) => m,
            Err(_) => {
                eprintln!(
                    "Warnung: Zeile {zeile}: Menge '{menge_roh}' ist keine Stückzahl, wird nach Bemerkung verschoben"
                );
                let alt = format!("Menge: {menge_roh}");
                bemerkung = Some(match bemerkung {
                    Some(b) => format!("{b} | {alt}"),
                    None => alt,
                });
                None
            }
        };

        artikel.push(Artikel {
            id: idx as u64,
            name,
            kaufdatum,
            menge,
            ablaufdatum,
            bemerkung,
        });
    }

    Ok(artikel)
}

pub fn speichern(artikel: &[Artikel]) -> Result<(), String> {
    let pfad = csv_pfad()?;
    let tmp_pfad = pfad.with_extension("csv.tmp");

    {
        let mut writer = csv::Writer::from_path(&tmp_pfad).map_err(|e| e.to_string())?;
        writer
            .write_record(KOPFZEILE)
            .map_err(|e| e.to_string())?;

        for a in artikel {
            let kaufdatum = a
                .kaufdatum
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_default();
            let menge = a.menge.map(|m| m.to_string()).unwrap_or_default();
            let ablaufdatum = a.ablaufdatum.format("%Y-%m-%d").to_string();
            let bemerkung = a.bemerkung.clone().unwrap_or_default();

            writer
                .write_record([&a.name, &kaufdatum, &menge, &ablaufdatum, &bemerkung])
                .map_err(|e| e.to_string())?;
        }
        writer.flush().map_err(|e| e.to_string())?;
    }

    std::fs::rename(&tmp_pfad, &pfad).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const KOPF: &str = "name,kaufdatum,menge,ablaufdatum,bemerkung\n";

    fn lies(inhalt: &str) -> Result<Vec<Artikel>, String> {
        lesen(inhalt.as_bytes())
    }

    #[test]
    fn gueltige_datei() {
        let a = lies(&format!(
            "{KOPF}Reis,2026-01-02,3,2027-06-10,Keller\nHonig,,,2028-05-01,\n"
        ))
        .unwrap();
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].name, "Reis");
        assert_eq!(a[0].kaufdatum, NaiveDate::from_ymd_opt(2026, 1, 2));
        assert_eq!(a[0].menge, Some(3));
        assert_eq!(a[0].bemerkung.as_deref(), Some("Keller"));
        assert_eq!(a[1].kaufdatum, None);
        assert_eq!(a[1].menge, None);
    }

    #[test]
    fn leere_datei_und_nur_kopfzeile() {
        assert!(lies("").unwrap().is_empty());
        assert!(lies(KOPF).unwrap().is_empty());
    }

    #[test]
    fn leerzeilen_werden_uebersprungen() {
        let a = lies(&format!("{KOPF},,,,\nReis,,,2027-06-10,\n\n")).unwrap();
        assert_eq!(a.len(), 1);
    }

    #[test]
    fn ungueltiges_ablaufdatum_ist_fehler_mit_zeile() {
        let e = lies(&format!("{KOPF}Reis,,,2027-06-10,\nMehl,,,31.12.2027,\n")).unwrap_err();
        assert!(e.contains("Zeile 3") && e.contains("Ablaufdatum"), "{e}");
    }

    #[test]
    fn ungueltiges_kaufdatum_ist_fehler() {
        let e = lies(&format!("{KOPF}Reis,gestern,,2027-06-10,\n")).unwrap_err();
        assert!(e.contains("Zeile 2") && e.contains("Kaufdatum"), "{e}");
    }

    #[test]
    fn fehlender_name_ist_fehler() {
        let e = lies(&format!("{KOPF},,2,2027-06-10,wichtig\n")).unwrap_err();
        assert!(e.contains("Zeile 2") && e.contains("Name"), "{e}");
    }

    #[test]
    fn falsche_kopfzeile_ist_fehler() {
        let e = lies("name,menge,kaufdatum,ablaufdatum,bemerkung\nReis,3,,2027-06-10,\n").unwrap_err();
        assert!(e.contains("Kopfzeile"), "{e}");
    }

    #[test]
    fn falsche_spaltenanzahl_ist_fehler() {
        assert!(lies(&format!("{KOPF}Reis,,,2027-06-10\n")).is_err());
    }

    #[test]
    fn alte_menge_wandert_in_bemerkung() {
        let a = lies(&format!("{KOPF}Milch,,\"1,5 l\",2027-06-01,Kühlschrank\n")).unwrap();
        assert_eq!(a[0].menge, None);
        assert_eq!(a[0].bemerkung.as_deref(), Some("Kühlschrank | Menge: 1,5 l"));
    }
}
