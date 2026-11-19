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
| Frontend          | **HTMX 2.0.4** (lokal eingebettet, kein CDN/Build-Schritt)    |
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
│   ├── main.rs            ← Einstiegspunkt: Env lesen, CSV laden, router() starten
│   ├── artikel.rs         ← Datenmodell: Artikel, AblaufStatus, parse_datum
│   ├── csv_io.rs          ← CSV laden/speichern, Pfad via ABLAUFDATUM_CSV
│   ├── web/
│   │   ├── mod.rs         ← AppState, router() inkl. Basic-Auth-Middleware
│   │   └── routes.rs      ← Axum-Handler, Template-Structs, Hilfstypen
│   └── bin/
│       └── checker.rs     ← Begleit-Binary für systemd-Timer
├── static/
│   ├── htmx-2.0.4.min.js  ← per include_str! eingebettet (kein CDN, offline-fähig)
│   └── README.md          ← Herkunft, SRI-Prüfsumme, Update-Anleitung
├── tests/
│   └── web.rs             ← Handler-Tests (Router ohne Server, eigene Temp-CSV je Test)
├── templates/
│   ├── index.html         ← Vollständige HTML-Seite (inkludiert Partial)
│   └── partials/
│       ├── main_content.html ← HTMX-Partial: Filter, Tabelle, Formular
│       └── modal.html        ← HTMX-Partial: Bearbeitungs-Modal
├── debian/
│   └── postinst           ← Hinweistext nach Paketinstallation (Desktop: Timer per
│                             systemctl --user aktivieren, optionale config.toml)
├── deploy/                ← System-Deployment (Raspberry Pi, Nginx)
│   ├── ablaufdatum-web.service
│   ├── ablaufdatum-checker.service
│   ├── ablaufdatum-checker.timer
│   ├── env.example        ← /etc/ablaufdatum/env (Passwort, Bind-Adresse)
│   ├── config.toml.example ← /etc/ablaufdatum/config.toml ([email], nur Platzhalter)
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
    pub kaufdatum: Option<NaiveDate>,  // automatisch = Einfügedatum, nicht in der UI
    pub menge: Option<u32>,            // optional, Stückzahl >= 1
    pub ablaufdatum: NaiveDate,        // Pflichtfeld
    pub bemerkung: Option<String>,     // optional
}

pub enum AblaufStatus { Abgelaufen, KritischBald, WarnungBald, Ok }
```

`Artikel::status(heute)`, `parse_datum(s)` (akzeptiert `TT.MM.JJJJ` und `JJJJ-MM-TT`) und
`parse_menge(s)` sind in `artikel.rs` definiert und werden von beiden Binaries verwendet.

**CSV-Spaltenreihenfolge:**
```
name,kaufdatum,menge,ablaufdatum,bemerkung
```
- Datumsformat in CSV: `YYYY-MM-DD`
- `id` wird nicht gespeichert (beim Laden als Zeilenindex vergeben, neue Artikel `max + 1`).
  Nach einem Neustart sind die IDs daher neu nummeriert → Schutz über die Seiten-Generation,
  siehe „Veraltete Seiten“ unter HTTP-Routen.

**Menge:** optionale Stückzahl, ganze Zahl >= 1 (`Option<u32>`). Einheiten/Gewichte wie
„500 g“ gehören in Name oder Bemerkung. `parse_menge` (in `artikel.rs`, genutzt von
Formular-Validierung und CSV-Laden): leer → `None`; `"0"`/negativ → „Menge muss größer als
0 sein.“; alles andere Nicht-Ganzzahlige (`"1,5"`, `"1.5"`, `"500 g"`) → „Menge muss eine
ganze Zahl sein.“; größer als `u32::MAX` → „Menge ist zu groß.“ Die Prüfung erfolgt immer
serverseitig (das `type="number"`-Feld im Browser ist nur Komfort).

**Migration alter Menge-Werte (beim Laden):** Früher war Menge Freitext. Ein CSV-Wert, der
keine gültige Stückzahl ist (z. B. `500 g`, `1,5 l`, `0`, `-3`), wird *nicht* verworfen:
`menge` wird `None`, der alte Text wird als `Menge: <alter Wert>` an die Bemerkung angehängt
(mit ` | ` getrennt, falls schon eine Bemerkung existiert), und pro Zeile erscheint eine
Warnung auf stderr. Das Laden schlägt dadurch nicht fehl. In die Datei geschrieben wird die
Migration erst beim nächsten Speichern (Hinzufügen/Bearbeiten/Löschen); bis dahin erscheinen
die Warnungen bei jedem Start (auch im Checker, der ebenfalls `laden()` nutzt).

> ⚠️ **Vor dem Deployment dieser Version `artikel.csv` sichern**, z. B.
> `cp artikel.csv artikel.csv.bak-$(date +%F)`.

**Kaufdatum:** bleibt in Modell und CSV erhalten (Format unverändert, leere Werte erlaubt),
wird aber in der UI weder angezeigt noch bearbeitet. `hinzufuegen` setzt es automatisch auf
das Einfügedatum (`chrono::Local::now().date_naive()`); `bearbeiten` lässt den bestehenden
Wert unverändert (auch einen leeren).

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
| `GET`    | `/?filter=X&veraltet=1` | `index`       | Nach abgelehnter veralteter Anfrage: Fehlerbanner |
| `GET`    | `/artikel/:id/modal?gen=G` | `zeige_modal` | Edit-Modal-Partial laden          |
| `PUT`    | `/artikel/:id`        | `bearbeiten`    | Artikel speichern (`gen` im Formular) |
| `DELETE` | `/artikel/:id?filter=X&gen=G` | `loeschen` | Artikel löschen                  |
| `GET`    | `/static/htmx-2.0.4.min.js` | `htmx_js` | HTMX (eingebettet, `immutable`)  |

**Seitenkontext:** `main_content.html` enthält `<div id="seiten-kontext" hidden>` mit zwei
versteckten Feldern: `#aktiver-filter` (`name="filter"`) und `#seiten-generation` (`name="gen"`).
Hinzufügen-Formular, Tabellenzeilen (Modal öffnen), Modal-Formular (Speichern) und
Löschen-Button schicken beide per `hx-include="#seiten-kontext"` mit (POST/PUT im Formular,
`ArtikelFormData::filter`/`gen`; GET/DELETE als Query-Parameter, da HTMX 2 dort die Parameter
in die URL schreibt, `FilterParams`).

**Filter bleibt erhalten:** Die Antwort wird mit dem mitgeschickten Filter gerendert. Die Antwort wird mit demselben Filter gerendert. `normalisiere_filter()` lässt nur
`alle|abgelaufen|kritisch|warnung|ok` zu, alles andere (fehlend, leer, unbekannt) → `alle`.
Ist der gerade hinzugefügte/bearbeitete Artikel im aktiven Filter nicht sichtbar, erscheint ein
blauer Hinweis (`hinweis`, CSS `info-banner`): „„X“ wurde gespeichert, ist im Filter „Y“ aber
nicht sichtbar.“ Die Browser-URL ändert sich beim Filtern nicht; ein Neuladen zeigt wieder „Alle“.

**Veraltete Seiten (Generation):** Die Artikel-IDs sind Zeilenindizes und werden beim Start neu
vergeben. Eine vor einem Neustart geladene Seite würde mit ihrer alten ID sonst einen *anderen*
Artikel bearbeiten/löschen. Deshalb erzeugt `AppState::neu()` pro Serverstart ein zufälliges
Token `AppState::generation` (16 Hex-Zeichen, ohne Zusatz-Crate über `RandomState`). Jede Seite
trägt es in `#seiten-generation`. `zeige_modal`, `hinzufuegen`, `bearbeiten` und `loeschen`
prüfen es zuerst (`generation_passt`); **fehlt** `gen` oder passt es nicht, wird nichts gelesen
oder geändert, und `veraltet_antwort()` antwortet mit `409 Conflict` + `HX-Redirect:
/?filter=X&veraltet=1`. HTMX wertet `HX-Redirect` vor dem Statuscode aus und lädt die ganze
Seite neu (neue IDs, neue Generation, nach einem Update auch neues CSS/JS). `index` zeigt bei
`veraltet` das rote Banner „Die Seite war veraltet und wurde neu geladen. Bitte erneut
versuchen.“; ein Skript in `index.html` entfernt danach die Parameter aus der Adresszeile
(`history.replaceState`). `GET /` (auch Filter-Klicks) braucht kein `gen`, es liefert immer die
aktuelle Generation mit. Auch curl-Clients müssen `gen` mitschicken (aus der Seite lesen).

**Artikel nicht gefunden:** Existiert die ID nicht (mehr), z. B. auf einem anderen Gerät
gelöscht, zeigen `bearbeiten`, `loeschen` und `zeige_modal` die Tabelle mit dem Fehlerbanner
„Artikel nicht gefunden – wurde er inzwischen gelöscht?“ und schließen das Modal (OOB). Es wird
nichts gespeichert. `zeige_modal` leitet dafür per `HX-Retarget: #main-content` um.

**Doppeltes Absenden:** Ohne `hx-sync` reiht HTMX ein zweites Absenden während einer laufenden
Anfrage ein und schickt es danach (→ doppelte Artikel). Das Hinzufügen-Formular und das
Modal-Formular haben `hx-sync="this:drop"` (zweites Absenden wird verworfen); der Löschen-Button
`hx-sync="closest form:drop"` (gemeinsame Warteschlange mit Speichern). `hx-disabled-elt` sperrt
währenddessen den Hinzufügen-Button bzw. `#modal-speichern` und `#modal-loeschen`
(CSS `button:disabled`). Serverseitig gibt es bewusst keine Duplikat-Erkennung.

Alle schreibenden Operationen halten den `RwLock` nur während Mutation + CSV-Speicherung.
HTMX-Anfragen (Header `HX-Request: true`) erhalten das `main_content.html`-Partial
statt der vollständigen Seite.

---

## Templates (`templates/`)

- **`index.html`**: Vollständiger HTML-Rahmen inkl. `<style>` (inline-CSS) und HTMX-Script
  (`/static/htmx-2.0.4.min.js` mit `integrity`-Attribut).
  Inkludiert `partials/main_content.html` via `{% include %}`.
- **`partials/main_content.html`**: `<div id="main-content">` mit Filter-Buttons, Tabelle,
  Hinzufügen-Formular. Wird bei HTMX-Swaps direkt zurückgegeben.
  - Tabellenspalten: **Name | Tage bis Ablauf | Menge | Ablaufdatum | Bemerkung**.
    „Tage bis Ablauf“ = `(ablaufdatum - heute).num_days()` (in `build_zeilen` berechnet,
    Feld `ArtikelZeile::tage_bis_ablauf: i64`), als Ganzzahl rechtsbündig (CSS-Klasse `zahl`):
    positiv = Tage verbleibend, `0` = heute, negativ = abgelaufen.
  - **Sortierung:** `build_zeilen` sortiert die sichtbaren Zeilen (nach dem Statusfilter)
    immer stabil aufsteigend nach `ablaufdatum` — unabhängig von der Reihenfolge in der
    CSV bzw. im `Vec` (`laden()` übernimmt die CSV-Reihenfolge unverändert).
  - **Jahres-Trennlinie:** `ArtikelZeile::jahreswechsel: bool` ist `true`, wenn das Jahr des
    Ablaufdatums sich gegenüber der *vorherigen sichtbaren* Zeile ändert (erste Zeile: immer
    `false`). Solche Zeilen erhalten die CSS-Klasse `jahreswechsel` → `border-top: 3px solid #333`
    auf den Zellen (sichtbar über allen Statusfarben und beim Hover). Nur eine Linie, keine
    zusätzliche Jahres-Überschriftszeile.
  - Formularfelder: Name *, Ablaufdatum *, Menge (`type="number" min="1" step="1"`, Fehler
    `fehler_menge`), Bemerkung (kein Kaufdatum). Menge wird in der Tabelle rechtsbündig angezeigt.
  - **Ablaufdatum-Eingabe:** `<input type="date" max="9999-12-31">` (Datumsauswahl des
    Browsers/Systems) im Hinzufügen-Formular und im Modal. Der Browser sendet und erwartet als
    `value` immer `JJJJ-MM-TT`, egal wie er das Datum anzeigt. Deshalb füllt `zeige_modal` das
    Feld ISO-formatiert, und bei Validierungsfehlern wandelt `datum_fuer_input()` eine gültige
    Eingabe nach ISO um. Der Server akzeptiert weiterhin auch `TT.MM.JJJJ` (`parse_datum`,
    z. B. curl oder Browser ohne Datumsfeld; dafür bleibt der `placeholder`). Die Tabelle zeigt
    unverändert `TT.MM.JJJJ`.
- **`partials/modal.html`**: Bearbeitungs-Overlay mit denselben Feldern wie das
  Hinzufügen-Formular (kein Kaufdatum). Wird via `hx-get="/artikel/:id/modal"`
  in `#modal-container` geladen. Fehler beim Speichern werden über `HX-Retarget` /
  `HX-Reswap` Response-Header dorthin zurückgeleitet.

### Mobile / Touch-Verhalten

- **Modal öffnen:** Desktop per Doppelklick, Touch-Geräte per einfachem Tap:
  `hx-trigger="dblclick, click[window.matchMedia('(hover: none)').matches]"`. Auf Geräten mit
  Hover (Maus) wird der `click`-Trigger herausgefiltert, ein Doppelklick lädt das Modal daher
  genau einmal.
- **Doppeltipp-Schutz:** Ein Skript in `index.html` merkt sich bei `htmx:afterSwap` auf
  `#modal-container` den Zeitpunkt (`window.modalGeoeffnetUm`). Das Overlay schließt sich erst
  bei Klicks > 400 ms danach. Sonst würde der zweite Tap eines gewohnheitsmäßigen Doppeltipps
  das gerade geöffnete Modal sofort wieder schließen.
- `touch-action: manipulation` auf Tabellenzeilen und Buttons (kein Doppeltipp-Zoom).
- **Kein Auto-Zoom bei Fokus:** In `@media (hover: none), (max-width: 600px)` haben Inputs
  (auch `type=date`), Selects, Textareas und Buttons `font-size: 16px` (iOS zoomt bei < 16px).
  Desktop behält 0.88rem. Das Datumsfeld ist `11em` breit (wächst mit) und hat eine
  `min-height`, weil iOS Safari ein leeres Datumsfeld sonst zusammenfallen lässt.
- **Modal:** `width: min(560px, calc(100vw - 24px))`, `max-height: calc(100dvh - 24px)` mit
  `overflow-y: auto`; Inputs im Modal 100 % breit; Aktions-Buttons umbrechen (`flex-wrap`).
- **Schmale Bildschirme (≤ 600px):** `.form-grid` im Modal einspaltig (Label über Feld),
  Hinzufügen-Formular gestapelt mit Feldern in voller Breite.
- **Tabelle:** liegt in `<div class="tabelle-scroll">` mit `overflow-x: auto`. Nur die Tabelle
  scrollt horizontal, nicht die Seite.
- Viewport-Meta bewusst **ohne** `maximum-scale`/`user-scalable=no`, damit Pinch-Zoom
  (Barrierefreiheit) erhalten bleibt.

Templates werden von Askama zur Build-Zeit kompiliert — keine Template-Dateien nötig
zur Laufzeit.

**HTMX** wird nicht vom CDN geladen, sondern liegt als `static/htmx-2.0.4.min.js` im Repo und
wird per `include_str!` ins Binary eingebettet (die App funktioniert ohne Internetzugang).
Ausgeliefert unter `/static/htmx-2.0.4.min.js` (hinter Basic Auth wie alle Routen) mit
`Cache-Control: public, max-age=31536000, immutable`; die Version im Dateinamen sorgt bei
einem Update für einen neuen URL. Das `integrity`-Attribut im `<script>`-Tag lässt den Browser
die Datei verwerfen, falls sie verändert wurde. Herkunft/Prüfung: `static/README.md`.

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
- `laden()`: leere Liste, wenn die Datei fehlt (oder 0 Byte groß ist). Sonst wird **jede** Zeile
  verlustfrei gelesen oder es gibt einen Fehler mit Zeilennummer, nie ein stilles Überspringen:
  falsche Kopfzeile, falsche Spaltenanzahl, fehlender Name, ungültiges Kauf- oder Ablaufdatum.
  Nur komplett leere Zeilen werden übersprungen. Ungültige Altwerte bei Menge werden in die
  Bemerkung migriert (siehe Datenmodell). Der Parser steckt in `lesen<R: Read>()` (Unit-Tests).
- **Startverhalten:** Schlägt `laden()` fehl, beendet sich der Web-Server mit Exit-Code 2,
  statt mit leerer Liste zu starten (sonst würde das nächste Speichern die Datei überschreiben).
  Die Service-Units setzen `RestartPreventExitStatus=2`, d. h. systemd startet nicht endlos neu.
  Der Dienst bleibt `failed`, und die Ursache steht im Journal. Exit-Code 2 gilt auch bei
  fehlendem `ABLAUFDATUM_PASSWORD`. Der Checker bricht bei Ladefehlern ebenfalls ab (Exit 1, Log).
- `laden_aus(pfad)`: wie `laden()`, mit explizitem Pfad (Web-Server und Tests). `laden()`
  (Pfad aus `csv_pfad()`) nutzt der Checker.
- `speichern(pfad, artikel)`: atomisch und dauerhaft (Stromausfall, SD-Karte am Pi):
  `.csv.tmp` schreiben → `sync_all()` auf die Temp-Datei → umbenennen → `sync_all()` auf das
  Verzeichnis (`verzeichnis_synchronisieren`; Fehler dort werden ignoriert, z. B. Windows).
  Der Web-Server übergibt `AppState::csv_pfad` (beim Start einmal per `csv_pfad()` bestimmt).

---

## Checker-Binary (`src/bin/checker.rs`)

Ablauf bei Ausführung:
1. CSV laden, Artikel mit `KritischBald` oder `Abgelaufen` filtern
2. Wenn keine → Exit 0 (kein Output)
3. `notify-send` nur aufrufen wenn `DISPLAY` oder `WAYLAND_DISPLAY` gesetzt (headless-sicher)
4. `config.toml` laden (Pfad: `ABLAUFDATUM_CONFIG` oder `~/.config/…`). Fehlt die Datei → keine
   E-Mail (kein Fehler). Ist sie vorhanden, aber unlesbar/ungültig (Syntaxfehler, fehlendes
   Feld) → `FEHLER Konfiguration: <pfad>: Zeile N: …` im Log, E-Mail gilt als fehlgeschlagen.
5. Wenn `[email]`-Sektion vorhanden: E-Mail via SMTP senden
6. In `checker.log` schreiben (Pfad: `ABLAUFDATUM_LOG` oder `~/.local/share/…`; letzte 50
   Einträge, jeder Eintrag einzeilig). Die Abschlusszeile (`zusammenfassung()`) nennt pro Kanal,
   was wirklich passiert ist, z. B.
   `OK: 2 kritische Artikel; keine Desktop-Benachrichtigung (kein Display); E-Mail gesendet` oder
   `WARNUNG: 2 kritische Artikel; …; keine E-Mail (nicht konfiguriert) – niemand wurde benachrichtigt`.
   `OK` nur, wenn mindestens ein Kanal gesendet hat und keiner fehlgeschlagen ist. Der Exit-Code
   bleibt 0 (nur Ladefehler der CSV → Exit 1).

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
`/etc/ablaufdatum/env` mit Passwort (aus `deploy/env.example`), optional
`/etc/ablaufdatum/config.toml` (aus `deploy/config.toml.example`), Units aus `deploy/` nach
`/etc/systemd/system/`, Nginx-Site aus `deploy/nginx-ablaufdatum.conf`, Unbound-Host-Override
in OPNsense. **Update:** vorher `artikel.csv` sichern; geänderte Units erneut kopieren +
`systemctl daemon-reload` (`make install-pi` macht beides nicht).

**Units:** Die Timer (`deploy/` und `systemd/`) verweisen nur per `Unit=` auf den Checker, ohne
`Requires=` (sonst startet der Checker bei jedem Boot bzw. Timer-Start sofort mit).
`deploy/ablaufdatum-checker.service` hat `Wants=`/`After=network-online.target` (SMTP braucht
ein konfiguriertes Netz).

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

## Tests

```bash
cargo test
```

- **Unit-Tests** in den Modulen: `artikel.rs` (Status-Grenzen, `parse_datum`, `parse_menge`),
  `csv_io.rs` (Parser-Fehler mit Zeilennummer, Menge-Migration, Speichern/Laden-Rundreise,
  atomisches Ersetzen, fehlende Datei), `web/routes.rs` (Filter, Hinweis, ISO-Datum),
  `bin/checker.rs` (Log-Zusammenfassung, config.toml-Fehler, E-Mail-Text, Adresslisten).
- **Handler-Tests** in `tests/web.rs`: der komplette `router()` inkl. Basic Auth per
  `tower::ServiceExt::oneshot`, ohne echten Server. Jeder Test hat seine eigene CSV im
  Temp-Verzeichnis (`AppState::csv_pfad`), daher keine globalen Env-Variablen und parallel
  lauffähig. Abgedeckt: 401/200, Seite vs. HTMX-Partial, Sortierung/Farbklassen, Filter,
  Hinzufügen/Bearbeiten/Löschen inkl. CSV-Inhalt, Validierungsfehler (CSV unverändert,
  `HX-Retarget`), fehlende Formularfelder (kein 422), statische HTMX-Datei.
- Dev-Dependencies: `tower` (`util`), `tempfile`.

---

## Akzeptanzkriterien

- [ ] `cargo build --release` ohne Fehler und Warnungen
- [ ] `curl http://127.0.0.1:8085/` → 401; mit Credentials → 200
- [ ] Artikel hinzufügen/bearbeiten/löschen funktioniert, CSV wird gespeichert
- [ ] Farbkodierung korrekt für alle 4 Status-Werte
- [ ] Checker läuft headless ohne notify-send-Warnung im Log
- [ ] Checker sendet E-Mail wenn `[email]` in `config.toml` konfiguriert
- [ ] `systemd-analyze verify deploy/*.service deploy/*.timer` ohne kritische Fehler (fehlende
      Binaries unter `/usr/local/bin` werden auf Entwicklungsrechnern gemeldet, das ist ok)
