use crate::artikel::Artikel;
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

pub fn laden() -> Result<Vec<Artikel>, String> {
    let pfad = csv_pfad()?;
    if !pfad.exists() {
        return Ok(Vec::new());
    }

    let mut reader = csv::Reader::from_path(&pfad).map_err(|e| e.to_string())?;
    let mut artikel = Vec::new();

    for (idx, ergebnis) in reader.records().enumerate() {
        let record = ergebnis.map_err(|e| e.to_string())?;

        let name = record.get(0).unwrap_or("").to_string();
        if name.is_empty() {
            continue;
        }

        let kaufdatum = record
            .get(1)
            .filter(|s| !s.is_empty())
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

        let menge = record
            .get(2)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let ablaufdatum_str = record.get(3).unwrap_or("");
        let ablaufdatum = NaiveDate::parse_from_str(ablaufdatum_str, "%Y-%m-%d")
            .map_err(|_| format!("Zeile {}: ungültiges Ablaufdatum '{}'", idx + 2, ablaufdatum_str))?;

        let bemerkung = record
            .get(4)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

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
            .write_record(["name", "kaufdatum", "menge", "ablaufdatum", "bemerkung"])
            .map_err(|e| e.to_string())?;

        for a in artikel {
            let kaufdatum = a
                .kaufdatum
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_default();
            let menge = a.menge.clone().unwrap_or_default();
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
