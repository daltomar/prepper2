# Ablaufdatum-Tracker — Claude Code Build Guide

## Projektziel

Eine Rust-Web-App zur Verwaltung von Artikeln mit Ablaufdaten. Sie wird hinter Nginx auf einem
Raspberry Pi (oder einem anderen Linux-Server) betrieben und ist über
`http://ablaufdatum.internal` im LAN erreichbar. Alle Daten werden in einer CSV-Datei
gespeichert. Die UI-Sprache ist **Deutsch**.

---

## Tech-Stack

| Komponente        | Entscheidung                                                  |
|---|---|
| Sprache           | Rust (stable)                                                 |
| HTTP-Framework    | **Axum 0.7** (async, Tokio-Runtime)                           |
| HTML-Templates    | **Askama 0.12** (Compile-Time, Jinja2-Syntax)                 |
| Frontend          | **HTMX 2** (CDN, kein Build-Schritt)                          |
| Auth              | HTTP Basic Auth (Passwort via `ABLAUFDATUM_PASSWORD`)         |
| Datenpersistenz   | CSV-Datei (`artikel.csv`), Pfad via `ABLAUFDATUM_CSV`         |
| Benachrichtigung  | `ablaufdatum-checker` Binary (systemd-Timer, SMTP via lettre) |
| Build-System      | Cargo + Makefile                                              |

---

## Projektstruktur

```
ablaufdatum-tracker/
├── Cargo.toml
├── Makefile
├── CLAUDE.md              ← diese Datei
├── install.sh             ← Desktop-Installation (user-level systemd)
├── src/
│   ├── lib.rs             ← pub mod artikel, csv_io, web
│   ├── main.rs            ← Axum-Server-Einstiegspunkt
│   ├── artikel.rs         ← Datenmodell: Artikel, AblaufStatus, parse_datum
│   ├── csv_io.rs          ← CSV laden/speichern, Pfad via ABLAUFDATUM_CSV
│   ├── web/
│   │   ├── mod.rs         ← AppState (Arc<RwLock<Vec<Artikel>>>, password)
│   │   └── routes.rs      ← Axum-Handler, Template-Structs, Hilfstypen
│   └── bin/
│       └── checker.rs     ← Begleit-Binary für systemd-Timer
├── templates/
│   ├── index.html         ← Vollständige HTML-Seite (inkludiert Partial)
│   └── partials/
│       ├── main_content.html ← HTMX-Partial: Filter, Tabelle, Formular
│       └── modal.html        ← HTMX-Partial: Bearbeitungs-Modal
├── deploy/                ← System-Deployment (Raspberry Pi, Nginx)
│   ├── ablaufdatum-web.service
│   ├── ablaufdatum-checker.service
│   ├── ablaufdatum-checker.timer
│   ├── env.example
│   ├── nginx-ablaufdatum.conf
│   └── README.md
└── systemd/               ← User-Level-Deployment (Desktop)
    ├── ablaufdatum-web.service
    ├── ablaufdatum-checker.service
    └── ablaufdatum-checker.timer
```

---

## Datenmodell (`src/artikel.rs`)

```rust
pub struct Artikel {
    pub id: u64,                       // intern, nicht in CSV
    pub name: String,                  // Pflichtfeld
    pub kaufdatum: Option<NaiveDate>,  // optional
    pub menge: Option<String>,         // optional, Freitext
    pub ablaufdatum: NaiveDate,        // Pflichtfeld
    pub bemerkung: Option<String>,     // optional
}

pub enum AblaufStatus { Abgelaufen, KritischBald, WarnungBald, Ok }
```

`Artikel::status(heute)` und `parse_datum(s)` (akzeptiert `TT.MM.JJJJ` und `JJJJ-MM-TT`)
sind in `artikel.rs` definiert und werden von beiden Binaries verwendet.

**CSV-Spaltenreihenfolge:**
```
name,kaufdatum,menge,ablaufdatum,bemerkung
```
- Datumsformat in CSV: `YYYY-MM-DD`
- `id` wird nicht gespeichert (beim Laden als Zeilenindex vergeben)

---

## Farbkodierung

| Status         | Hintergrund | Text        |
|---|---|---|
| `Ok`           | `#FFFFFF`   | `#000000`   |
| `WarnungBald`  | `#FFF176`   | `#000000`   |
| `KritischBald` | `#FFF176`   | `#D32F2F`   |
| `Abgelaufen`   | `#FFCDD2`   | `#000000`   |

CSS-Klassen: `status-ok`, `status-warnung`, `status-kritisch`, `status-abgelaufen`
(definiert inline in `templates/index.html`).

---

## HTTP-Routen (`src/web/routes.rs`)

| Methode  | Pfad                  | Handler         | Beschreibung                         |
|---|---|---|---|
| `GET`    | `/`                   | `index`         | Vollständige Seite oder HTMX-Partial |
| `GET`    | `/?filter=X`          | `index`         | Gefilterte Tabelle                   |
| `POST`   | `/artikel`            | `hinzufuegen`   | Artikel hinzufügen                   |
| `GET`    | `/artikel/:id/modal`  | `zeige_modal`   | Edit-Modal-Partial laden             |
| `PUT`    | `/artikel/:id`        | `bearbeiten`    | Artikel speichern                    |
| `DELETE` | `/artikel/:id`        | `loeschen`      | Artikel löschen                      |

Alle schreibenden Operationen halten den `RwLock` nur während Mutation + CSV-Speicherung.
HTMX-Anfragen (Header `HX-Request: true`) erhalten das `main_content.html`-Partial
statt der vollständigen Seite.

---

## Templates (`templates/`)

- **`index.html`**: Vollständiger HTML-Rahmen inkl. `<style>` (inline-CSS) und HTMX-CDN-Script.
  Inkludiert `partials/main_content.html` via `{% include %}`.
- **`partials/main_content.html`**: `<div id="main-content">` mit Filter-Buttons, Tabelle,
  Hinzufügen-Formular. Wird bei HTMX-Swaps direkt zurückgegeben.
- **`partials/modal.html`**: Bearbeitungs-Overlay. Wird via `hx-get="/artikel/:id/modal"`
  in `#modal-container` geladen. Fehler beim Speichern werden über `HX-Retarget` /
  `HX-Reswap` Response-Header dorthin zurückgeleitet.

Templates werden von Askama zur Build-Zeit kompiliert — keine Template-Dateien nötig
zur Laufzeit.

---

## Umgebungsvariablen

| Variable               | Default              | Beschreibung                            |
|---|---|---|
| `ABLAUFDATUM_PASSWORD` | (Pflicht)            | HTTP-Basic-Auth-Passwort                |
| `ABLAUFDATUM_ADDR`     | `127.0.0.1:8085`     | Bind-Adresse des Web-Servers            |
| `ABLAUFDATUM_CSV`      | `~/.local/share/…`   | Pfad zur artikel.csv                    |
| `ABLAUFDATUM_CONFIG`   | `~/.config/…`        | Pfad zur config.toml (Checker)          |
| `ABLAUFDATUM_LOG`      | `~/.local/share/…`   | Pfad zur checker.log (Checker)          |

---

## CSV I/O (`src/csv_io.rs`)

- `csv_pfad()`: prüft `ABLAUFDATUM_CSV`, Fallback auf `~/.local/share/ablaufdatum-tracker/artikel.csv`
- `laden()`: leere Liste wenn Datei fehlt
- `speichern()`: atomisches Schreiben via `.csv.tmp` → umbenennen

---

## Checker-Binary (`src/bin/checker.rs`)

Ablauf bei Ausführung:
1. CSV laden, Artikel mit `KritischBald` oder `Abgelaufen` filtern
2. Wenn keine → Exit 0 (kein Output)
3. `notify-send` nur aufrufen wenn `DISPLAY` oder `WAYLAND_DISPLAY` gesetzt (headless-sicher)
4. `config.toml` laden (Pfad: `ABLAUFDATUM_CONFIG` oder `~/.config/…`)
5. Wenn `[email]`-Sektion vorhanden: E-Mail via SMTP senden
6. In `checker.log` schreiben (Pfad: `ABLAUFDATUM_LOG` oder `~/.local/share/…`)

---

## Deployment

### Desktop (user-level systemd)

```bash
make install          # build + copy + install.sh
systemctl --user enable --now ablaufdatum-web.service
```

### Raspberry Pi (system-level, hinter Nginx)

Vollständige Anleitung: `deploy/README.md`

```bash
make install-pi       # build + sudo copy + systemctl restart
```

Wichtigste Schritte: System-User `ablaufdatum` anlegen, Binaries nach `/usr/local/bin/`,
`/etc/ablaufdatum/env` mit Passwort, Units aus `deploy/` nach `/etc/systemd/system/`,
Nginx-Site aus `deploy/nginx-ablaufdatum.conf`, Unbound-Host-Override in OPNsense.

---

## Lokale Entwicklung

```bash
ABLAUFDATUM_PASSWORD=test cargo run --bin ablaufdatum-tracker
# oder:
make run

# Checker testen (ohne DISPLAY, mit Temp-Dateien):
ABLAUFDATUM_CSV=/tmp/t.csv \
ABLAUFDATUM_CONFIG=/tmp/cfg.toml \
ABLAUFDATUM_LOG=/tmp/checker.log \
cargo run --bin ablaufdatum-checker
```

---

## Akzeptanzkriterien

- [ ] `cargo build --release` ohne Fehler und Warnungen
- [ ] `curl http://127.0.0.1:8085/` → 401; mit Credentials → 200
- [ ] Artikel hinzufügen/bearbeiten/löschen funktioniert, CSV wird gespeichert
- [ ] Farbkodierung korrekt für alle 4 Status-Werte
- [ ] Checker läuft headless ohne notify-send-Warnung im Log
- [ ] Checker sendet E-Mail wenn `[email]` in `config.toml` konfiguriert
- [ ] `systemd-analyze verify deploy/*.service` ohne kritische Fehler
