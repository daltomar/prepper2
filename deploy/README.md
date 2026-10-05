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

# Email config (optional – required for checker notifications)
# Copy and edit the template, then lock it down:
sudo cp deploy/env.example /etc/ablaufdatum/config.toml   # replace with real config
sudo nano /etc/ablaufdatum/config.toml
sudo chown root:ablaufdatum /etc/ablaufdatum/config.toml
sudo chmod 640 /etc/ablaufdatum/config.toml
```

The config.toml format:

```toml
[email]
smtp_server = "smtp.gmail.com"
smtp_port   = 587
username    = "dein@gmail.com"
password    = "app-passwort"
from        = "dein@gmail.com"
to          = "empfaenger@email.de"
```

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

```bash
cd ablaufdatum-tracker
git pull
cargo build --release
sudo cp target/release/ablaufdatum-tracker /usr/local/bin/
sudo cp target/release/ablaufdatum-checker /usr/local/bin/
sudo systemctl restart ablaufdatum-web.service
```

Or: `make install-pi` (builds + copies + restarts in one step).

---

## Logs

```bash
# Web service (live):
sudo journalctl -u ablaufdatum-web.service -f

# Checker:
sudo journalctl -u ablaufdatum-checker.service
cat /var/lib/ablaufdatum/checker.log
```
