# Statische Dateien

Werden per `include_str!` in das Binary eingebettet (keine Dateien zur Laufzeit nötig)
und unter `/static/…` ausgeliefert (siehe `src/web/routes.rs`).

## htmx-2.0.4.min.js

Unveränderte Kopie von `htmx.org@2.0.4` `dist/htmx.min.js` (npm-Paket, BSD-0-Clause-Lizenz).

- Größe: 50917 Byte
- SRI: `sha384-HGfztofotfshcF7+8n44JQL2oJmowVChPTg48S+jvZoztPfvwD79OC/LTtG6dMp+`
  (identisch mit unpkg, jsDelivr und dem npm-Tarball; dieser Wert steht auch im
  `integrity`-Attribut in `templates/index.html`)

Prüfen:

```bash
echo "sha384-$(openssl dgst -sha384 -binary static/htmx-2.0.4.min.js | base64 -w0)"
```

**Aktualisieren:** neue Datei als `htmx-<version>.min.js` ablegen, alte löschen, Pfad und
`integrity` in `templates/index.html` sowie `include_str!`/Route in `src/web/routes.rs` und
`src/main.rs` anpassen. Der Dateiname enthält die Version, weil die Datei mit
`Cache-Control: immutable` ausgeliefert wird; ein neuer Name umgeht alte Browser-Caches.
