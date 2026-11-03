#!/usr/bin/env bash
# Reproducible performance numbers for zeshicast (Q2.2), from the
# ZESHICAST_TRACE=1 timing marks (Q2.1).
#
#   scripts/measure.sh               headless: model load + file index (CLI)
#   scripts/measure.sh --gui         + palette show / keystroke latency / RSS,
#                                      once per renderer (cairo, ngl)
#
# --gui drives a real palette: it starts its own traced daemon, toggles the
# window and types queries with wtype. Stop the regular daemon first
# (`systemctl --user stop zeshicast`) and keep your hands off the keyboard
# while it runs (about a minute).
#
# Every wtype call is a new Wayland client that hands the compositor a keymap
# fd. If some clients stopped reading their sockets (e.g. a pile of hung
# screen lockers), those fds stay in flight, and enough of them push the
# user's in-flight fd limit over: from then on *every* new client fails with
# ETOOMANYREFS (this took a niri session down on 2026-11-02). --gui therefore
# checks for clients with a stuck socket queue first and asks before typing.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUNS=10
GUI=0
ASSUME_YES=0
FORCE=0
# Clients with a persistent unread backlog that make --gui refuse to run.
STUCK_CLIENTS_LIMIT=5
BIN_DIR="$ROOT_DIR/target/release"
RENDERERS=(cairo ngl)
QUERIES=(firefox "file invoice" "calc 2+2" term clip)
# Delay between typed keys: a brisk human pace, so every keystroke is a
# sample (a 25 ms burst would only measure the end of the debounce).
KEY_DELAY_MS=120

usage() {
  cat <<'USAGE'
Usage: scripts/measure.sh [--gui] [--runs N] [--bin DIR]

  --gui       also measure the palette (needs Wayland, wtype, no running daemon)
  --runs N    repetitions per measurement (default 10)
  --renderer R  measure only GSK renderer R (cairo, ngl); default both
  --bin DIR   directory with zeshicast and zeshicast-gtk (default target/release;
              built with `--features gui,layer-shell` when missing)
  --yes       do not ask before typing into the session with wtype
  --force     run --gui even with many Wayland clients that stopped reading
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --gui) GUI=1 ;;
    --runs) RUNS="${2:?--runs needs a number}"; shift ;;
    --renderer) RENDERERS=("${2:?--renderer needs a name}"); shift ;;
    --bin) BIN_DIR="${2:?--bin needs a directory}"; shift ;;
    --yes) ASSUME_YES=1 ;;
    --force) FORCE=1 ;;
    -h | --help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

if [[ ! -x "$BIN_DIR/zeshicast" || ( "$GUI" == 1 && ! -x "$BIN_DIR/zeshicast-gtk" ) ]]; then
  echo "building release binaries…" >&2
  (cd "$ROOT_DIR" && nix-shell shell.nix --run \
    "cargo build --release --features gui,layer-shell --quiet")
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Values (one per line, in ms) → "median p95 max" by nearest rank.
stats() {
  sort -n | awk '
    { v[NR] = $1 }
    END {
      if (NR == 0) { print "-  -  -"; exit }
      p50 = int((NR * 50 + 99) / 100); p95 = int((NR * 95 + 99) / 100)
      printf "%.1f  %.1f  %.1f\n", v[p50], v[p95], v[NR]
    }'
}

# Pull the millisecond values of "trace: <label> took X ms" lines.
took() { grep -F "trace: $1 took" "$2" | sed -E 's/.* took ([0-9.]+) ms.*/\1/'; }

echo "## zeshicast measurements — $(date -I)"
echo
echo "- commit: $(git -C "$ROOT_DIR" rev-parse --short HEAD)"
echo "- kernel: $(uname -r)"
echo "- cpu: $(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2- | sed 's/^ *//')"
echo "- runs: $RUNS"
echo
echo "### Headless (CLI, model load)"
echo
echo "| mark | median ms | p95 ms | max ms |"
echo "| --- | --- | --- | --- |"
LOG="$WORK/cli.log"
: >"$LOG"
for _ in $(seq "$RUNS"); do
  echo | ZESHICAST_TRACE=1 "$BIN_DIR/zeshicast" firefox >/dev/null 2>>"$LOG" || true
done
for mark in "apps load" "clipboard and usage load" "commands and scripts load" "file index"; do
  read -r p50 p95 max < <(took "$mark" "$LOG" | stats)
  echo "| $mark | $p50 | $p95 | $max |"
done

if [[ "$GUI" != 1 ]]; then
  exit 0
fi

if [[ -z "${WAYLAND_DISPLAY:-}" ]]; then
  echo "--gui needs a Wayland session" >&2; exit 1
fi
if ! command -v wtype >/dev/null; then
  echo "--gui needs wtype to type queries" >&2; exit 1
fi
# The palette's GApplication id on the session bus: owned means a daemon (or a
# launcher window) is running and would receive our --toggle/--quit.
app_running() {
  if command -v busctl >/dev/null; then
    busctl --user status dev.zeshi.Zeshicast >/dev/null 2>&1
  else
    pgrep -x zeshicast-gtk >/dev/null || pgrep -x .zeshicast-gtk-wrapped >/dev/null
  fi
}
if app_running; then
  echo "zeshicast is running; stop it first: systemctl --user stop zeshicast" >&2
  exit 1
fi

# Client sockets of this Wayland display with unread bytes: "inode bytes
# process". The server ends are bound to the display path; their peers are
# the clients.
backlogged_clients() {
  local sock="$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY"
  ss -xpH 2>/dev/null | awk -v sock="$sock" '
    $5 == sock { peer[$8] = 1; next }
    $3 > 0 { queued[$6] = $3 " " $NF }
    END { for (i in peer) if (i in queued) print i, queued[i] }' | sort
}
# Only a backlog present in two samples a second apart counts as stuck.
first="$(backlogged_clients)"
sleep 1
stuck="$(comm -12 <(printf '%s\n' "$first") <(backlogged_clients) | sed '/^$/d')"
if [[ -n "$stuck" ]]; then
  count="$(wc -l <<<"$stuck")"
  echo "Wayland clients that are not reading their socket ($count):" >&2
  # users:(("hyprlock",pid=123,fd=4)) → "hyprlock pid 123: N bytes queued"
  awk '{ split($3, f, /[",=]+/); printf "  %s pid %s: %s bytes queued\n", f[2], f[4], $2 }' \
    <<<"$stuck" >&2
  if (( count >= STUCK_CLIENTS_LIMIT && FORCE == 0 )); then
    echo "refusing to start ~$((RUNS * ${#QUERIES[@]} * 2 * ${#RENDERERS[@]})) wtype clients:" \
      "stuck clients can exhaust the in-flight fd limit (ETOOMANYREFS) and" \
      "break every new Wayland client. Close them first, or pass --force." >&2
    exit 1
  fi
fi

if (( ASSUME_YES == 0 )); then
  echo "--gui will type into the focused window with wtype for about a minute." >&2
  read -r -p "Keep hands off the keyboard. Continue? [y/N] " answer
  [[ "$answer" == [yY]* ]] || exit 1
fi

GTK="$BIN_DIR/zeshicast-gtk"
DAEMON=""
# An interrupted run must not leave a traced daemon behind.
trap 'rm -rf "$WORK"; [[ -n "$DAEMON" ]] && kill "$DAEMON" 2>/dev/null; true' EXIT
for renderer in "${RENDERERS[@]}"; do
  LOG="$WORK/gui-$renderer.log"
  ZESHICAST_TRACE=1 GSK_RENDERER="$renderer" "$GTK" --daemon >/dev/null 2>"$LOG" &
  DAEMON=$!
  sleep 3 # warm index, D-Bus name, first map of nothing

  for _ in $(seq "$RUNS"); do
    "$GTK" --toggle
    sleep 0.6
    for query in "${QUERIES[@]}"; do
      wtype -d "$KEY_DELAY_MS" "$query"
      sleep 0.4
      wtype -M ctrl a -m ctrl -k BackSpace
      sleep 0.2
    done
    "$GTK" --toggle
    sleep 0.4
  done

  "$GTK" --quit
  wait "$DAEMON" || true

  echo
  echo "### Palette, GSK_RENDERER=$renderer"
  echo
  read -r p50 p95 max < <(took "show" "$LOG" | stats)
  echo "- show (toggle → first frame): median $p50 ms, p95 $p95 ms, max $max ms"
  echo "- $(grep -F 'trace: keystroke to results' "$LOG" | sed 's/.*trace: //' || echo 'keystroke to results: no samples')"
  echo "- $(grep -F 'trace: rss while hidden' "$LOG" | tail -1 | sed 's/.*trace: //')"
  echo "- $(grep -F 'trace: rss at exit' "$LOG" | sed 's/.*trace: //')"
done
