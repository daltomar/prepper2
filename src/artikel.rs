use chrono::NaiveDate;

#[derive(Debug, Clone)]
pub struct Artikel {
    pub id: u64,
    pub name: String,
    pub kaufdatum: Option<NaiveDate>,
    pub menge: Option<String>,
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
