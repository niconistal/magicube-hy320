#!/usr/bin/env bash
# Shared helpers for the magicube CLI and setup scripts. Source it, don't run it.
#
# Target selection (first match wins):
#   1. MAGICUBE env var        e.g. MAGICUBE=192.168.1.50  or  MAGICUBE=<usb-serial>
#   2. ~/.config/magicube/target (written by `magicube target <ip|serial>`)

MC_CONF="${XDG_CONFIG_HOME:-$HOME/.config}/magicube"
MC_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

die()  { echo "magicube: $*" >&2; exit 1; }
info() { echo "== $*"; }

mc_target() {
  local t="${MAGICUBE:-}"
  [ -z "$t" ] && [ -f "$MC_CONF/target" ] && t="$(cat "$MC_CONF/target")"
  [ -z "$t" ] && die "no projector set. Run: magicube target <ip or usb-serial>"
  # bare IP -> ip:5555
  if [[ "$t" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then t="$t:5555"; fi
  echo "$t"
}

# Connect if the target is a network address. Quiet on success.
mc_connect() {
  local t; t="$(mc_target)"
  if [[ "$t" == *:* ]]; then
    adb connect "$t" >/dev/null 2>&1 || true
  fi
  adb -s "$t" get-state >/dev/null 2>&1 || die "projector $t not reachable (powered on? same network? USB cable?)"
}

A()  { adb -s "$(mc_target)" "$@"; }
SH() { adb -s "$(mc_target)" shell "$@"; }

# Run a command as root. `adb shell su -c "a && b"` loses its quotes, so only `a`
# runs as root and `b` runs as the shell user. Push a script and run that instead.
SU() {
  local tmp; tmp="$(mktemp)"
  printf '%s\n' "$*" > "$tmp"
  A push "$tmp" /data/local/tmp/mc-su.sh >/dev/null
  rm -f "$tmp"
  SH su -c "sh /data/local/tmp/mc-su.sh"
}

has_root() { [ "$(SH su -c id 2>/dev/null | grep -c 'uid=0')" -ge 1 ]; }
need_root() { has_root || die "this needs root (Magisk + Shell allowed in Superuser). See README > Root."; }

# Wait until Android reports boot_completed (re-connects over TCP while it comes back).
wait_boot() {
  local t b i; t="$(mc_target)"
  for i in $(seq 1 60); do
    [[ "$t" == *:* ]] && adb connect "$t" >/dev/null 2>&1
    b="$(adb -s "$t" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')"
    [ "$b" = 1 ] && { echo "booted after ~$((i*3))s"; return 0; }
    sleep 3
  done
  die "projector did not come back within 3 min"
}

pkg_present() { SH pm list packages | tr -d '\r' | grep -qx "package:$1"; }
