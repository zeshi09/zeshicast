#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
APP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
SYSTEMD_USER_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"

ENABLE_DAEMON=0
START_DAEMON=0

usage() {
  cat <<'USAGE'
Usage:
  scripts/install-user.sh [--enable-daemon] [--start-daemon]

Installs:
  ~/.local/bin/zeshicast
  ~/.local/bin/zeshicast-gtk
  ~/.local/share/applications/zeshicast-gtk.desktop
  ~/.local/share/applications/zeshicast-gtk-daemon.desktop
  ~/.config/systemd/user/zeshicast-gtk.service
USAGE
}

# Refuse (or require confirmation) when zeshicast.service already exists as a
# user unit, e.g. installed by the NixOS/home-manager module: two daemons
# would compete for the launcher socket and clipboard history.
check_unit_conflict() {
  local unit_dump=""
  unit_dump="$(systemctl --user cat zeshicast.service 2>/dev/null || true)"
  if [[ -z "$unit_dump" ]]; then
    return 0
  fi
  cat >&2 <<EOF

========================================================================
WARNING: an existing user unit zeshicast.service was found:
------------------------------------------------------------------------
$unit_dump
------------------------------------------------------------------------
It most likely comes from the NixOS / home-manager module and runs its own
daemon. Installing this standalone copy can leave two competing daemons.
========================================================================

Continue anyway? [y/N]
EOF
  local reply=""
  read -r reply || reply=""
  if [[ ! "$reply" =~ ^[Yy]([Ee][Ss])?$ ]]; then
    echo "Aborted: existing zeshicast.service left untouched." >&2
    exit 1
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
  --enable-daemon)
    ENABLE_DAEMON=1
    ;;
  --start-daemon)
    START_DAEMON=1
    ;;
  -h | --help)
    usage
    exit 0
    ;;
  *)
    echo "unknown option: $1" >&2
    usage >&2
    exit 2
    ;;
  esac
  shift
done

check_unit_conflict

render_template() {
  local src="$1"
  local dst="$2"
  sed "s|@BIN@|$BIN_DIR|g" "$src" >"$dst"
}

cd "$ROOT_DIR"

if command -v nix >/dev/null 2>&1; then
  nix develop -f shell.nix --command cargo build --release --features gui --bins
else
  cargo build --release --features gui --bins
fi

mkdir -p "$BIN_DIR" "$APP_DIR" "$SYSTEMD_USER_DIR"
install -m 0755 target/release/zeshicast "$BIN_DIR/zeshicast"
install -m 0755 target/release/zeshicast-gtk "$BIN_DIR/zeshicast-gtk"

render_template packaging/zeshicast-gtk.desktop "$APP_DIR/zeshicast-gtk.desktop"
render_template packaging/zeshicast-gtk-daemon.desktop "$APP_DIR/zeshicast-gtk-daemon.desktop"
render_template packaging/zeshicast-gtk.service "$SYSTEMD_USER_DIR/zeshicast-gtk.service"
chmod 0644 "$APP_DIR/zeshicast-gtk.desktop" \
  "$APP_DIR/zeshicast-gtk-daemon.desktop" \
  "$SYSTEMD_USER_DIR/zeshicast-gtk.service"

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$APP_DIR" || true
fi

if command -v systemctl >/dev/null 2>&1; then
  systemctl --user daemon-reload
  if [[ "$ENABLE_DAEMON" -eq 1 ]]; then
    systemctl --user enable zeshicast-gtk.service
  fi
  if [[ "$START_DAEMON" -eq 1 ]]; then
    systemctl --user restart zeshicast-gtk.service
  fi
fi

cat <<EOF
Installed zeshicast to $BIN_DIR

Run launcher:
  $BIN_DIR/zeshicast-gtk

Start daemon now:
  systemctl --user restart zeshicast-gtk.service

Enable daemon for the graphical session:
  systemctl --user enable zeshicast-gtk.service

Fallback if your desktop does not start graphical-session.target:
  systemctl --user add-wants default.target zeshicast-gtk.service

Suggested Wayland hotkey command:
  $BIN_DIR/zeshicast-gtk
EOF
