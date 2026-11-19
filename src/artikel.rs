use chrono::NaiveDate;

#[derive(Debug, Clone)]
pub struct Artikel {
    pub id: u64,
    pub name: String,
    pub kaufdatum: Option<NaiveDate>,
    pub menge: Option<u32>,      // Stückzahl >= 1, optional
    pub ablaufdatum: NaiveDate,
    pub bemerkung: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AblaufStatus {
    Abgelaufen,
    KritischBald,
    WarnungBald,
    Ok,
}

impl Artikel {
    pub fn status(&self, heute: NaiveDate) -> AblaufStatus {
        if self.ablaufdatum < heute {
            AblaufStatus::Abgelaufen
        } else if self.ablaufdatum <= heute + chrono::Months::new(2) {
            AblaufStatus::KritischBald
        } else if self.ablaufdatum <= heute + chrono::Months::new(4) {
            AblaufStatus::WarnungBald
        } else {
            AblaufStatus::Ok
        }
    }
}

pub fn parse_datum(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    // TT.MM.JJJJ
    if let Ok(d) = NaiveDate::parse_from_str(s, "%d.%m.%Y") {
        return Some(d);
    }
    // JJJJ-MM-TT
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d);
    }
    None
}

pub const FEHLER_MENGE_GANZZAHL: &str = "Menge muss eine ganze Zahl sein.";
pub const FEHLER_MENGE_POSITIV: &str = "Menge muss größer als 0 sein.";

/// Menge (Stückzahl) prüfen: leer → `Ok(None)`, ganze Zahl >= 1 → `Ok(Some(n))`,
/// sonst `Err` mit deutscher Fehlermeldung. Wird von Formular-Validierung und CSV-Laden genutzt.
pub fn parse_menge(s: &str) -> Result<Option<u32>, String> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(None);
    }
    let (negativ, ziffern) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    if ziffern.is_empty() || !ziffern.bytes().all(|b| b.is_ascii_digit()) {
        return Err(FEHLER_MENGE_GANZZAHL.to_string());
    }
    if negativ || ziffern.bytes().all(|b| b == b'0') {
        return Err(FEHLER_MENGE_POSITIV.to_string());
    }
    ziffern
        .parse::<u32>()
        .map(Some)
        .map_err(|_| "Menge ist zu groß.".to_string())
}

#[cfg(test)]
mod tests_menge {
    use super::*;

    #[test]
    fn gueltige_werte() {
        assert_eq!(parse_menge(""), Ok(None));
        assert_eq!(parse_menge("   "), Ok(None));
        assert_eq!(parse_menge("1"), Ok(Some(1)));
        assert_eq!(parse_menge("12"), Ok(Some(12)));
        assert_eq!(parse_menge(" 3 "), Ok(Some(3)));
    }

    #[test]
    fn null_und_negativ() {
        for s in ["0", "00", "-2", "-0"] {
            assert_eq!(parse_menge(s), Err(FEHLER_MENGE_POSITIV.to_string()), "Eingabe {s:?}");
        }
    }

    #[test]
    fn keine_ganze_zahl() {
        for s in ["1,5", "1.5", "500 g", "abc", "-", "-1.5", "1e3"] {
            assert_eq!(parse_menge(s), Err(FEHLER_MENGE_GANZZAHL.to_string()), "Eingabe {s:?}");
        }
    }

    #[test]
    fn zu_gross() {
        assert!(parse_menge("99999999999").is_err());
    }
}
