# Ablaufdatum-Tracker – Raspberry Pi Deployment

Deploys the web app and weekly checker as system services on Raspberry Pi OS (Bookworm),
behind Nginx, reachable at `http://ablaufdatum.internal` on the LAN.

---

## Prerequisites

- Raspberry Pi OS Bookworm (64-bit)
- Rust toolchain: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- `sudo apt install nginx git`

---

## 1. System user

```bash
sudo useradd --system --no-create-home --shell /usr/sbin/nologin ablaufdatum
```

---

## 2. Build

```bash
git clone <repo-url> ablaufdatum-tracker
cd ablaufdatum-tracker
cargo build --release
```

Or use the Makefile shortcut (builds + copies binaries, see step 3):

```bash
make install-pi
```

---

## 3. Install binaries

```bash
sudo cp target/release/ablaufdatum-tracker /usr/local/bin/
sudo cp target/release/ablaufdatum-checker /usr/local/bin/
sudo chmod 755 /usr/local/bin/ablaufdatum-tracker \
               /usr/local/bin/ablaufdatum-checker
```

---

## 4. Secrets and config directory

```bash
sudo mkdir -p /etc/ablaufdatum

# Environment file (password + bind address)
sudo cp deploy/env.example /etc/ablaufdatum/env
sudo nano /etc/ablaufdatum/env          # set ABLAUFDATUM_PASSWORD
sudo chown root:ablaufdatum /etc/ablaufdatum/env
sudo chmod 640 /etc/ablaufdatum/env

# Email config (optional – required for checker e-mail notifications)
# Copy the template, fill in your SMTP data, then lock it down:
sudo cp deploy/config.toml.example /etc/ablaufdatum/config.toml
sudo nano /etc/ablaufdatum/config.toml
sudo chown root:ablaufdatum /etc/ablaufdatum/config.toml
sudo chmod 640 /etc/ablaufdatum/config.toml
```

`deploy/config.toml.example` contains only placeholders. All six fields in `[email]` are
required; `to` accepts several addresses separated by commas. Without the file the checker
sends no e-mail; a broken file is reported in `/var/lib/ablaufdatum/checker.log`
(`FEHLER Konfiguration: … Zeile N: …`).

---

## 5. Data directory

`StateDirectory=ablaufdatum` in the unit file creates `/var/lib/ablaufdatum` automatically
on first start. If you need to create it manually:

```bash
sudo mkdir -p /var/lib/ablaufdatum
sudo chown ablaufdatum:ablaufdatum /var/lib/ablaufdatum
sudo chmod 750 /var/lib/ablaufdatum
```

---

## 6. systemd units

```bash
sudo cp deploy/ablaufdatum-web.service     /etc/systemd/system/
sudo cp deploy/ablaufdatum-checker.service /etc/systemd/system/
sudo cp deploy/ablaufdatum-checker.timer   /etc/systemd/system/
sudo systemctl daemon-reload
```

Enable and start the web service:

```bash
sudo systemctl enable --now ablaufdatum-web.service
sudo systemctl status ablaufdatum-web.service
```

Enable the weekly checker timer (optional):

```bash
sudo systemctl enable --now ablaufdatum-checker.timer
systemctl list-timers ablaufdatum-checker.timer
```

Run the checker immediately to test:

```bash
sudo systemctl start ablaufdatum-checker.service
sudo journalctl -u ablaufdatum-checker.service
cat /var/lib/ablaufdatum/checker.log
```

---

## 7. Nginx

```bash
sudo cp deploy/nginx-ablaufdatum.conf /etc/nginx/sites-available/ablaufdatum
sudo ln -s /etc/nginx/sites-available/ablaufdatum \
           /etc/nginx/sites-enabled/ablaufdatum
sudo nginx -t && sudo systemctl reload nginx
```

> **Note:** `server_name ablaufdatum.internal` must match the DNS name exactly.

---

## 8. OPNsense Unbound host override

In OPNsense → Services → Unbound DNS → Host Overrides, add:

| Field  | Value               |
|--------|---------------------|
| Host   | `ablaufdatum`       |
| Domain | `internal`          |
| IP     | Pi's LAN IP address |

---

## 9. Verify

```bash
# From the Pi (no DNS needed):
curl -o /dev/null -w "%{http_code}\n" http://127.0.0.1:8085/
# → 401 (no credentials)

curl -u user:yourpassword -o /dev/null -w "%{http_code}\n" http://127.0.0.1:8085/
# → 200

# From another machine on the LAN (after DNS propagates):
curl -u user:yourpassword http://ablaufdatum.internal/
```

---

## Updating

1. **Back up the data first** (the new version may read the CSV more strictly, see
   "Web service stays failed" below):

   ```bash
   sudo cp -p /var/lib/ablaufdatum/artikel.csv \
              /var/lib/ablaufdatum/artikel.csv.bak-$(date +%F)
   ```

2. Build and install the binaries:

   ```bash
   cd ablaufdatum-tracker
   git pull
   cargo build --release
   sudo cp target/release/ablaufdatum-tracker /usr/local/bin/
   sudo cp target/release/ablaufdatum-checker /usr/local/bin/
   ```

3. If unit files in `deploy/` changed (`git diff --stat HEAD@{1} -- deploy/` after the pull
   shows it), copy them again and reload systemd. Copying unchanged files does no harm:

   ```bash
   sudo cp deploy/ablaufdatum-web.service     /etc/systemd/system/
   sudo cp deploy/ablaufdatum-checker.service /etc/systemd/system/
   sudo cp deploy/ablaufdatum-checker.timer   /etc/systemd/system/
   sudo systemctl daemon-reload
   ```

4. Restart and check:

   ```bash
   sudo systemctl restart ablaufdatum-web.service
   sudo systemctl status ablaufdatum-web.service
   systemctl list-timers ablaufdatum-checker.timer
   ```

`make install-pi` covers step 2 plus the restart. It does **not** back up the CSV or copy
the unit files.

Browser tabs that were open before the restart are outdated (item IDs are reassigned on
start). Their next save/delete is refused, and the page reloads with the notice "Die Seite war
veraltet und wurde neu geladen. Bitte erneut versuchen." Nothing is changed in that case.

---

## Logs

```bash
# Web service (live):
sudo journalctl -u ablaufdatum-web.service -f

# Checker:
sudo journalctl -u ablaufdatum-checker.service
cat /var/lib/ablaufdatum/checker.log
```

### Web service stays "failed" after start

If `artikel.csv` cannot be read completely (wrong header, missing name, invalid date, wrong
number of columns), the server **refuses to start** instead of starting with an empty list,
which the next save would write over your data. It exits with code 2, and systemd does
not restart it (`RestartPreventExitStatus=2`). The journal shows the file and line:

```bash
sudo journalctl -u ablaufdatum-web.service -n 20
# Fehler: Artikel-CSV konnte nicht geladen werden: /var/lib/ablaufdatum/artikel.csv: Zeile 7: ungültiges Ablaufdatum '31.12.2027'
```

Fix that line (dates in the CSV are `YYYY-MM-DD`) or restore a backup, then
`sudo systemctl restart ablaufdatum-web.service`.
