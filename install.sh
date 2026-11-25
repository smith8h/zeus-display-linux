#!/usr/bin/env bash
# Builds Zeus Display and installs it for the system:
#   binary          -> /usr/bin
#   icons           -> ~/.local/share/icons/hicolor
#   menu entry      -> ~/.local/share/applications
#   login entry     -> ~/.config/autostart   (skip with --no-autostart)
# Access to the USB display needs a one-time udev rule; the script tells you if it's missing.
set -euo pipefail

APP_ID=io.github.zeusdisplay.ZeusDisplay
SRC="$(cd "$(dirname "$0")" && pwd)"
DEST="/usr/bin"
AUTOSTART=1
[[ "${1:-}" == "--no-autostart" ]] && AUTOSTART=0

cargo build --release --manifest-path "$SRC/Cargo.toml"

pkill -x zeus-display 2>/dev/null && sleep 0.5 || true

sudo install -m 755 "$SRC/target/release/zeus-display" "$DEST/zeus-display"

ICONS="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor"
mkdir -p "$ICONS/scalable/apps" "$ICONS/symbolic/apps"
install -m 644 "$SRC/packaging/icons/$APP_ID.svg" "$ICONS/scalable/apps/"
install -m 644 "$SRC/packaging/icons/$APP_ID-symbolic.svg" "$ICONS/symbolic/apps/"
gtk4-update-icon-cache -q -t -f "$ICONS" 2>/dev/null || true

APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
mkdir -p "$APPS"
sed "s|@EXEC@|\"$DEST/zeus-display\"|" "$SRC/packaging/$APP_ID.desktop" > "$APPS/$APP_ID.desktop"
update-desktop-database -q "$APPS" 2>/dev/null || true

if (( AUTOSTART )); then
  mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/autostart"
  cat > "${XDG_CONFIG_HOME:-$HOME/.config}/autostart/$APP_ID.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Zeus Display
Comment=Sends CPU temperature and load to the cooler display
Exec="$DEST/zeus-display" --background
Icon=$APP_ID
Terminal=false
X-GNOME-Autostart-enabled=true
X-GNOME-Autostart-Delay=5
DESKTOP
fi

echo "Installed to $DEST"

if [[ ! -f /etc/udev/rules.d/70-zeus-display.rules ]]; then
  echo
  echo "One more step: give your account access to the cooler's USB display (needs your password):"
  echo "  sudo cp \"$SRC/packaging/70-zeus-display.rules\" /etc/udev/rules.d/ && sudo udevadm control --reload && sudo udevadm trigger"
fi
