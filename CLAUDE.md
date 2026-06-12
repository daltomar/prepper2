# Ablaufdatum-Tracker — Claude Code Build Guide

## Projektziel

Erstelle eine native Desktop-Anwendung für Linux in **Rust**, die eine Liste von Artikeln mit
Ablaufdaten verwaltet. Die App wird über ein Shell-Skript (`install.sh`) auf Linux-Maschinen
installiert und als
einzelne Binärdatei ausgeführt. Es wird **keine Datenbank** verwendet; alle Daten werden in einer
**CSV-Datei** im selben Verzeichnis wie die Binärdatei gespeichert.

Die gesamte UI-Sprache ist **Deutsch**.

---

## Tech-Stack

| Komponente | Entscheidung |
|---|---|
| Sprache | Rust (stable, aktuelle Version) |
| GUI-Framework | **egui** via `eframe` (empfohlen wegen geringer Abhängigkeiten und einfacher Cross-Compilation) |
| Datenpersistenz | CSV-Datei (`artikel.csv`) im selben Verzeichnis wie die Binärdatei |
| Paketierung | `install.sh` Shell-Skript (kein `cargo-deb`, kein `sudo`) |
| Build-System | Cargo + `Makefile` für häufige Aufgaben |

---

## Projektstruktur

```
ablaufdatum-tracker/
├── Cargo.toml
├── Cargo.lock
├── Makefile
├── CLAUDE.md            ← diese Datei
├── assets/
│   └── icon.png         ← optionales App-Icon (256x256)
└── src/
    ├── main.rs          ← Einstiegspunkt, eframe setup
    ├── app.rs           ← Haupt-App-Struct, egui update-Loop
    ├── artikel.rs       ← Datenmodell: Artikel-Struct + Status-Enum
    ├── csv_io.rs        ← CSV lesen/schreiben
    └── ui/
        ├── mod.rs
        ├── tabelle.rs   ← Listenansicht mit Farbkodierung
        ├── formular.rs  ← Hinzufügen-Formular (inline, unterhalb der Tabelle)
        └── edit_modal.rs← Bearbeitungs-Popup (Doppelklick)
```

---

## Datenmodell (`src/artikel.rs`)

```rust
use chrono::NaiveDate;

#[derive(Debug, Clone)]
pub struct Artikel {
    pub id: u64,                          // intern, nicht in CSV sichtbar
    pub name: String,                     // Pflichtfeld
    pub kaufdatum: Option<NaiveDate>,     // optional
    pub menge: Option<String>,            // optional, Freitext (z. B. "2 Stk", "500 g")
    pub ablaufdatum: NaiveDate,           // Pflichtfeld
    pub bemerkung: Option<String>,        // optional
}

#[derive(Debug, Clone, PartialEq)]
pub enum AblaufStatus {
    Abgelaufen,          // Ablaufdatum < heute
    KritischBald,        // Ablaufdatum <= heute + 2 Monate
    WarnungBald,         // Ablaufdatum <= heute + 4 Monate
    Ok,                  // alles andere
}

impl Artikel {
    pub fn status(&self, heute: chrono::NaiveDate) -> AblaufStatus {
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
```

**CSV-Spaltenreihenfolge** (Header-Zeile muss vorhanden sein):
```
name,kaufdatum,menge,ablaufdatum,bemerkung
```
- Datumsformat: `YYYY-MM-DD`
- Leere optionale Felder werden als leerer String gespeichert `""`
- Die `id` wird **nicht** in der CSV gespeichert; sie wird beim Laden als Zeilenindex vergeben

---

## Farbkodierung

| Status | Hintergrundfarbe | Textfarbe |
|---|---|---|
| `Ok` | Weiß `#FFFFFF` | Schwarz `#000000` |
| `WarnungBald` (≤ 4 Monate) | Gelb `#FFF176` | Schwarz `#000000` |
| `KritischBald` (≤ 2 Monate) | Gelb `#FFF176` | Rot `#D32F2F` |
| `Abgelaufen` | Rot `#FFCDD2` | Schwarz `#000000` |

Diese Farben sind Konstanten in `src/artikel.rs` oder `src/ui/mod.rs` zu definieren.

---

## Sortierung & Filterung

### Standardsortierung
Die Tabelle wird **immer** nach `ablaufdatum` aufsteigend sortiert (abgelaufene Artikel zuerst).

### Statusfilter
Oberhalb der Tabelle gibt es eine Filterleiste mit Schaltflächen / Dropdown:

| Filter-Option | Beschreibung |
|---|---|
| Alle | Alle Artikel anzeigen |
| Abgelaufen | Nur `AblaufStatus::Abgelaufen` |
| Bald kritisch | Nur `AblaufStatus::KritischBald` |
| Warnung | Nur `AblaufStatus::WarnungBald` |
| OK | Nur `AblaufStatus::Ok` |

Filterauswahl wird **nicht** persistiert (Reset beim App-Start auf „Alle").

---

## UI-Aufbau (`src/app.rs` + `src/ui/`)

```
┌─────────────────────────────────────────────────┐
│  Ablaufdatum-Tracker                            │
├─────────────────────────────────────────────────┤
│  Filter: [Alle] [Abgelaufen] [Bald kritisch]    │
│          [Warnung] [OK]                         │
├──────────┬────────────┬────────┬──────────┬─────┤
│ Name     │ Kaufdatum  │ Menge  │ Ablauf   │ Obs │
├──────────┼────────────┼────────┼──────────┼─────┤
│ Milch    │ 2025-05-01 │ 1 L    │2025-06-01│     │ ← roter Hintergrund
│ Joghurt  │            │ 500 g  │2025-07-15│     │ ← gelb, roter Text
│ Käse     │ 2025-05-10 │ 200 g  │2025-08-20│ Bio │ ← gelb, schwarzer Text
│ Pasta    │            │        │2026-01-01│     │ ← weiß
├──────────┴────────────┴────────┴──────────┴─────┤
│  ── Neuen Artikel hinzufügen ──────────────────  │
│  Name*: [___________] Ablauf*: [__________]     │
│  Kaufd: [___________] Menge:   [__________]     │
│  Bemerk: [_______________________________]      │
│                              [ Hinzufügen ]     │
└─────────────────────────────────────────────────┘
```

### Tabelle (`src/ui/tabelle.rs`)
- Spalten: **Name**, **Kaufdatum**, **Menge**, **Ablaufdatum**, **Bemerkung**
- Jede Zeile bekommt Hintergrundfarbe + Textfarbe gemäß Farbkodierung
- **Doppelklick** auf eine Zeile öffnet das Bearbeitungs-Modal
- Zeilenbreite passt sich dem Fenster an; Spalten sind in vernünftigen Standardbreiten
- Keine Inline-Bearbeitung direkt in der Tabelle

### Hinzufügen-Formular (`src/ui/formular.rs`)
- Unterhalb der Tabelle, immer sichtbar
- Felder: Name (Pflicht), Ablaufdatum (Pflicht), Kaufdatum (optional), Menge (optional), Bemerkung (optional)
- Datumsfelder akzeptieren Freitext im Format `TT.MM.JJJJ` (deutsche Schreibweise) und werden intern nach `YYYY-MM-DD` konvertiert
- **Feldbreiten** (via `egui::TextEdit::desired_width`):
  - Name: 400px (mindestens 50 Zeichen sichtbar)
  - Ablaufdatum: 100px (passt genau `00.00.0000`)
  - Kaufdatum: 100px (gleich wie Ablaufdatum)
  - Menge: 50px (passt `0000`)
  - Bemerkung: 450px (etwas breiter als Name)
- Validierung beim Klick auf „Hinzufügen":
  - Name darf nicht leer sein
  - Ablaufdatum muss ein gültiges Datum sein
  - Kaufdatum, wenn angegeben, muss ein gültiges Datum sein
  - Fehlermeldungen werden direkt unterhalb des jeweiligen Feldes in roter Schrift angezeigt
- Nach erfolgreichem Hinzufügen: Formular leeren, CSV sofort speichern, Tabelle neu sortieren

### Bearbeitungs-Modal (`src/ui/edit_modal.rs`)
- Wird per **Doppelklick** auf eine Tabellenzeile geöffnet
- Zeigt ein Popup-Fenster (egui `Window`) mit denselben Feldern wie das Hinzufügen-Formular, vorausgefüllt mit den Daten des gewählten Artikels
- Schaltflächen:
  - **Speichern**: Validierung wie beim Hinzufügen, dann Artikel aktualisieren + CSV speichern + Modal schließen
  - **Löschen**: Zeigt zuerst einen Bestätigungs-Dialog (`egui::Window` oder `egui::Modal`) mit dem Text „Artikel ‹Name› wirklich löschen?" und Schaltflächen **Ja** / **Abbrechen**; bei Bestätigung: Artikel entfernen + CSV speichern + Modal schließen
  - **Abbrechen**: Modal schließen ohne Änderungen

---

## CSV I/O (`src/csv_io.rs`)

- Crate: `csv` (aktuelle Version)
- Dateiname: `artikel.csv` — immer im selben Verzeichnis wie die ausgeführte Binärdatei (ermittelt via `std::env::current_exe()`)
- Beim App-Start: Datei laden; existiert sie nicht, leere Liste starten (Datei wird beim ersten Speichern erstellt)
- **Atomisches Schreiben**: erst in `artikel.csv.tmp` schreiben, dann umbenennen — verhindert Datenverlust bei Absturz
- Fehlerbehandlung: Lade-/Speicherfehler als `Result<_, String>` zurückgeben und in der UI als roter Fehlertext oben anzeigen, App läuft weiter

---

## Abhängigkeiten (`Cargo.toml`)

```toml
[dependencies]
eframe = "0.27"          # egui + native Backend
egui = "0.27"
chrono = { version = "0.4", features = ["serde"] }
csv = "1.3"
serde = { version = "1", features = ["derive"] }

[profile.release]
opt-level = 3
lto = true
strip = true
```

Versionen können leicht abweichen — immer die neueste kompatible Version verwenden.

---

## Installation (`install.sh` + `Makefile`)

Kein `.deb`-Paket, kein `cargo-deb`, kein `sudo`. Die App wird über ein Shell-Skript installiert,
das ohne Root-Rechte auskommt und auf allen Debian/Ubuntu x86_64-Maschinen funktioniert, die der
Benutzer selbst verwaltet.

### Workflow

```bash
# 1. Einmalig auf der Entwicklungsmaschine: Release bauen
cargo build --release

# 2. Auf derselben oder einer anderen Maschine installieren
./install.sh
```

Für andere Maschinen: Binärdateien und `install.sh` per `scp` kopieren, dann dort ausführen:
```bash
scp target/release/ablaufdatum-tracker \
    target/release/ablaufdatum-checker \
    install.sh \
    systemd/ablaufdatum-checker.service \
    systemd/ablaufdatum-checker.timer \
    user@zielmaschine:~/ablaufdatum-tmp/
ssh user@zielmaschine "cd ~/ablaufdatum-tmp && ./install.sh"
```

### `install.sh`

```bash
#!/usr/bin/env bash
set -e

INSTALL_DIR="$HOME/.local/bin"
DATA_DIR="$HOME/.local/share/ablaufdatum-tracker"
SYSTEMD_DIR="$HOME/.config/systemd/user"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "==> Installiere Ablaufdatum-Tracker..."

# Verzeichnisse anlegen
mkdir -p "$INSTALL_DIR"
mkdir -p "$DATA_DIR"
mkdir -p "$SYSTEMD_DIR"

# Binärdateien kopieren
cp "$SCRIPT_DIR/ablaufdatum-tracker" "$INSTALL_DIR/"
cp "$SCRIPT_DIR/ablaufdatum-checker" "$INSTALL_DIR/"
chmod +x "$INSTALL_DIR/ablaufdatum-tracker"
chmod +x "$INSTALL_DIR/ablaufdatum-checker"

# systemd User-Units installieren
cp "$SCRIPT_DIR/ablaufdatum-checker.service" "$SYSTEMD_DIR/"
cp "$SCRIPT_DIR/ablaufdatum-checker.timer"   "$SYSTEMD_DIR/"
systemctl --user daemon-reload

echo ""
echo "==> Installation abgeschlossen."
echo ""
echo "Stelle sicher, dass ~/.local/bin in deinem PATH ist:"
echo "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc && source ~/.bashrc"
echo ""
echo "App starten:"
echo "  ablaufdatum-tracker"
echo ""
echo "Wöchentliche Benachrichtigungen aktivieren (optional):"
echo "  systemctl --user enable --now ablaufdatum-checker.timer"
echo ""
echo "E-Mail konfigurieren (optional):"
echo "  Beim ersten Lauf des Checkers wird eine Beispielkonfiguration erstellt unter:"
echo "  ~/.config/ablaufdatum-tracker/config.toml.example"
```

Das Skript muss ausführbar sein: `chmod +x install.sh` — dies einmalig im Projektverzeichnis tun
und in Git committen.

### Deinstallation

```bash
rm ~/.local/bin/ablaufdatum-tracker
rm ~/.local/bin/ablaufdatum-checker
systemctl --user disable --now ablaufdatum-checker.timer 2>/dev/null || true
rm -f ~/.config/systemd/user/ablaufdatum-checker.{service,timer}
systemctl --user daemon-reload
# Datendatei bewusst NICHT löschen — muss der Benutzer manuell tun:
# rm -rf ~/.local/share/ablaufdatum-tracker/
```

### `Makefile`

```makefile
.PHONY: build install run clean

build:
	cargo build --release

run:
	cargo run

install: build
	@cp target/release/ablaufdatum-tracker .
	@cp target/release/ablaufdatum-checker .
	./install.sh

check:
	ablaufdatum-checker

install-timer:
	systemctl --user enable --now ablaufdatum-checker.timer

uninstall-timer:
	systemctl --user disable --now ablaufdatum-checker.timer

clean:
	cargo clean
	rm -f ablaufdatum-tracker ablaufdatum-checker
```

> Die CSV-Datei liegt unter `~/.local/share/ablaufdatum-tracker/artikel.csv` und wird beim ersten
> Speichern automatisch erstellt. Beide Binärdateien teilen diesen Pfad.

---

## Implementierungshinweise für Claude Code

1. **Datums-Eingabe**: Nutze ein einfaches `egui::TextEdit` für Datumsfelder. Parse beim Verlassen
   des Feldes oder beim Speichern. Akzeptiere `TT.MM.JJJJ` und `JJJJ-MM-TT` — gib klare Fehlermeldungen aus.

2. **ID-Vergabe**: Beim Laden der CSV bekommt jeder Artikel eine `id` (laufende Nummer). Beim
   Hinzufügen: `max(id) + 1`. IDs werden nicht in die CSV geschrieben.

3. **Statusberechnung**: `chrono::Local::now().date_naive()` für das heutige Datum — bei jedem
   `update()`-Aufruf frisch berechnen, damit die App korrekt bleibt wenn sie über Nacht offen bleibt.

4. **Fensterbreite**: Mindestgröße 900 × 600 px; `eframe::NativeOptions` entsprechend setzen.

5. **Egui-Tabelle**: Verwende `egui_extras::TableBuilder` (Crate `egui_extras` mit Feature
   `"tables"`) für eine saubere, scrollbare Tabelle mit festen Spaltenbreiten.
   Füge `egui_extras = { version = "0.27", features = ["tables"] }` zu `Cargo.toml` hinzu.

6. **Doppelklick-Erkennung**: Nutze `response.double_clicked()` auf den Zeilen-Rect in der Tabelle.

7. **Modal-Zustand**: Im App-Struct ein `Option<Artikel>` (Klon des gewählten Artikels) halten,
   das `Some(...)` ist wenn das Modal offen ist.

8. **Bestätigungs-Dialog**: Als zweites `Option<u64>` (die ID des zu löschenden Artikels) im
   App-Struct implementieren; separates `egui::Window` das nur gerendert wird wenn `Some`.

9. **Fehler-Toast**: Im App-Struct ein `Option<String>` für Fehlermeldungen; wird oben in der UI
   als rotes Label angezeigt und nach dem nächsten erfolgreichen Speichern/Laden geleert.

10. **install.sh ausführbar machen**: Im Projekt einmalig `chmod +x install.sh` ausführen und die
    Datei in Git committen, damit sie auf Zielmaschinen direkt ausführbar ist.

---

## Akzeptanzkriterien (Definition of Done)

- [ ] App startet ohne Fehler auf einem aktuellen Ubuntu/Debian x86_64 System
- [ ] CSV wird korrekt geladen und gespeichert (inkl. Sonderzeichen in Name/Bemerkung)
- [ ] Alle 4 Farbzustände werden korrekt angezeigt
- [ ] Artikel hinzufügen funktioniert mit Validierung (Pflichtfelder, Datumsformat)
- [ ] Doppelklick öffnet Bearbeitungs-Modal mit korrekten Vorbelegungen
- [ ] Löschen zeigt Bestätigungs-Dialog und entfernt Artikel aus Liste + CSV
- [ ] Statusfilter schränkt Anzeige korrekt ein
- [ ] Tabelle ist nach Ablaufdatum aufsteigend sortiert (abgelaufene zuerst)
- [ ] `install.sh` kopiert beide Binärdateien und systemd-Units korrekt ohne Root-Rechte
- [ ] Kein `unwrap()` in Produktionspfaden — alle Fehler werden behandelt

---

## Feature: Ablauf-Benachrichtigungen (systemd Timer)

### Ziel

Ein separater Begleit-Binary (`ablaufdatum-checker`) prüft einmal wöchentlich (montags) die
`artikel.csv` und benachrichtigt den Benutzer über Artikel, die sich im kritischen Zustand befinden
(`AblaufStatus::KritischBald` oder `AblaufStatus::Abgelaufen`). Benachrichtigung erfolgt über:

1. **Desktop-Notification** via `notify-send` — immer, wenn Artikel gefunden werden
2. **E-Mail-Digest** via SMTP — optional, nur wenn eine Konfigurationsdatei vorhanden ist

Die Haupt-App (`ablaufdatum-tracker`) läuft **nicht** im Hintergrund. Der Checker wird
ausschließlich vom systemd-User-Timer gestartet und danach sofort beendet.

---

### Neue Dateien und Änderungen an der Projektstruktur

```
ablaufdatum-tracker/
├── src/
│   ├── lib.rs           ← NEU: gemeinsame Logik (Artikel, CSV-IO, Status) als Library
│   ├── main.rs          ← wie bisher, nutzt jetzt lib.rs
│   └── bin/
│       └── checker.rs   ← NEU: Begleit-Binary für den Timer
├── systemd/
│   └── ablaufdatum-checker.service   ← NEU
│   └── ablaufdatum-checker.timer     ← NEU
└── install.sh           ← bereits vorhanden, installiert auch die systemd-Units
```

**Wichtig:** Die gemeinsame Logik (`Artikel`, `AblaufStatus`, `csv_io`) wird nach `src/lib.rs`
verschoben und von beiden Binaries genutzt. `src/main.rs` und `src/bin/checker.rs` importieren
jeweils `use ablaufdatum_tracker::*`.

---

### `Cargo.toml` Änderungen

```toml
[lib]
name = "ablaufdatum_tracker"
path = "src/lib.rs"

[[bin]]
name = "ablaufdatum-tracker"
path = "src/main.rs"

[[bin]]
name = "ablaufdatum-checker"
path = "src/bin/checker.rs"

[dependencies]
# bestehende Abhängigkeiten bleiben erhalten, zusätzlich:
lettre = { version = "0.11", features = ["smtp-transport", "rustls-tls", "builder"], default-features = false }
toml = "0.8"
dirs = "5"
```

---

### Konfigurationsdatei (`~/.config/ablaufdatum-tracker/config.toml`)

Der Checker liest diese Datei beim Start. Existiert sie nicht → nur Desktop-Notification, kein
E-Mail-Versuch, kein Fehler.

```toml
# Beispiel-Konfiguration — alle Felder sind optional
[email]
smtp_server = "smtp.gmail.com"
smtp_port = 587          # STARTTLS; alternativ 465 für SSL
username = "dein@email.de"
password = "app-passwort"   # bei Gmail: App-Passwort verwenden
from = "dein@email.de"
to = "empfaenger@email.de"
```

**Implementierungshinweis:** Passwort wird im Klartext gespeichert — Datei-Permissions auf `600`
setzen. Der Checker soll beim Erststart (wenn Datei fehlt) eine Beispielkonfiguration nach
`~/.config/ablaufdatum-tracker/config.toml.example` schreiben und einen Hinweis ausgeben.

---

### Checker-Binary (`src/bin/checker.rs`)

Ablauf:

1. CSV-Pfad ermitteln: zuerst Umgebungsvariable `ABLAUFDATUM_CSV` prüfen, dann
   `~/.local/share/ablaufdatum-tracker/artikel.csv` als Fallback (siehe Hinweis unten)
2. CSV laden via `lib.rs`
3. Heutiges Datum ermitteln; Artikel mit `AblaufStatus::KritischBald` oder
   `AblaufStatus::Abgelaufen` filtern und nach Ablaufdatum sortieren
4. Wenn keine solchen Artikel → Programm beendet sich still (kein Output, kein Fehler)
5. Wenn Artikel gefunden:
   a. `notify-send` aufrufen (siehe unten)
   b. Konfigurationsdatei laden — wenn `[email]`-Sektion vorhanden: E-Mail senden (siehe unten)
6. Ergebnis (Erfolg / Fehler) in `~/.local/share/ablaufdatum-tracker/checker.log` schreiben
   (letzte 50 Zeilen behalten, ältere verwerfen)

**CSV-Pfad-Hinweis:** Da der Checker als systemd-Service läuft (nicht aus dem Binär-Verzeichnis),
kann er nicht `current_exe()` verwenden. `install.sh` soll die Haupt-App so anpassen, dass
sie die CSV nach `~/.local/share/ablaufdatum-tracker/artikel.csv` speichert — **dieser Pfad
ersetzt den bisherigen „gleicher Ordner wie Binary"-Ansatz für beide Binaries**. Aktualisiere
entsprechend `src/csv_io.rs` und den Abschnitt „Datenpersistenz" in diesem Dokument.

---

### Desktop-Notification (`notify-send`)

```rust
// Beispiel-Aufruf im Checker
std::process::Command::new("notify-send")
    .arg("--urgency=normal")
    .arg("--icon=dialog-warning")
    .arg("Ablaufdatum-Tracker")
    .arg(format!("{} Artikel laufen bald ab oder sind abgelaufen", anzahl))
    .status()?;
```

Wenn `notify-send` nicht gefunden wird (exit code ≠ 0 oder `CommandNotFound`): Fehler ins Log
schreiben, aber Programm nicht abbrechen — E-Mail-Versand trotzdem versuchen.

---

### E-Mail-Format

- **Betreff:** `Ablaufdatum-Tracker: X Artikel prüfen`
- **Body (plain text):**

```
Hallo,

folgende Artikel erfordern deine Aufmerksamkeit:

ABGELAUFEN:
  - Milch (abgelaufen seit 3 Tagen, 01.06.2025)
  - Joghurt (abgelaufen seit 1 Tag, 03.06.2025)

BALD ABLAUFEND (≤ 2 Monate):
  - Käse (noch 18 Tage, 22.06.2025)

Öffne Ablaufdatum-Tracker um Artikel zu aktualisieren oder zu löschen.
```

- Datumsangaben im deutschen Format `TT.MM.JJJJ`
- „seit X Tagen" / „noch X Tagen" relativ zu heute berechnen
- Kein HTML-Body — nur `text/plain`

---

### systemd Unit-Dateien

**`systemd/ablaufdatum-checker.service`:**
```ini
[Unit]
Description=Ablaufdatum-Tracker Benachrichtigungsdienst
After=network.target

[Service]
Type=oneshot
ExecStart=/usr/bin/ablaufdatum-checker
Environment=DISPLAY=:0
Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/%U/bus
```

**`systemd/ablaufdatum-checker.timer`:**
```ini
[Unit]
Description=Ablaufdatum-Tracker wöchentliche Prüfung
Requires=ablaufdatum-checker.service

[Timer]
OnCalendar=Mon *-*-* 09:00:00
Persistent=true
Unit=ablaufdatum-checker.service

[Install]
WantedBy=timers.target
```

`Persistent=true` bedeutet: wenn der Rechner montags um 09:00 Uhr ausgeschaltet war, wird der
Timer beim nächsten Login nachgeholt.

---

### Manuelle Aktivierung nach Installation (kein Auto-Enable)

`install.sh` aktiviert den Timer **nicht** automatisch. Nach der Installation gibt das Skript
folgende Anleitung aus:

```
==> Installation abgeschlossen.

Wöchentliche Benachrichtigungen aktivieren (optional):
  systemctl --user enable --now ablaufdatum-checker.timer

E-Mail konfigurieren (optional):
  Beim ersten Lauf des Checkers wird eine Beispielkonfiguration erstellt unter:
  ~/.config/ablaufdatum-tracker/config.toml.example

Status prüfen:
  systemctl --user status ablaufdatum-checker.timer
  journalctl --user -u ablaufdatum-checker.service
```

---

### Makefile-Ergänzungen

Die folgenden Targets sind bereits im Haupt-`Makefile` enthalten (siehe Abschnitt „Installation"):
`install`, `check`, `install-timer`, `uninstall-timer`. Keine weiteren Änderungen nötig.

---

### Akzeptanzkriterien für dieses Feature

- [ ] `ablaufdatum-checker` kompiliert als eigenständiger Binary
- [ ] Checker liest dieselbe `artikel.csv` wie die Haupt-App (`~/.local/share/ablaufdatum-tracker/`)
- [ ] Kein Output und Exit 0 wenn keine kritischen Artikel vorhanden
- [ ] `notify-send`-Aufruf erscheint als Desktop-Popup wenn kritische Artikel vorhanden
- [ ] E-Mail wird gesendet wenn `config.toml` mit `[email]`-Sektion vorhanden und gültig
- [ ] E-Mail wird stillschweigend übersprungen wenn `config.toml` fehlt
- [ ] Beispiel-`config.toml.example` wird beim ersten Lauf erstellt
- [ ] Log-Datei wird unter `~/.local/share/ablaufdatum-tracker/checker.log` gepflegt
- [ ] systemd-Timer-Dateien liegen unter `systemd/` im Projektverzeichnis
- [ ] `install.sh` kopiert Service- und Timer-Dateien nach `~/.config/systemd/user/`
- [ ] `install.sh` gibt nach der Installation eine Aktivierungsanleitung aus
- [ ] `Persistent=true` im Timer sorgt für Nachholung bei verpasstem Zeitfenster
