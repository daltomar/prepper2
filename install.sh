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
cp "$SCRIPT_DIR/systemd/ablaufdatum-checker.service" "$SYSTEMD_DIR/"
cp "$SCRIPT_DIR/systemd/ablaufdatum-checker.timer"   "$SYSTEMD_DIR/"
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
