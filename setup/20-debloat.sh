#!/usr/bin/env bash
# Step 2: switch off vendor telemetry, OTA updater, app store and casting services.
#   setup/20-debloat.sh          disable (for user 0; nothing is uninstalled)
#   setup/20-debloat.sh --undo   turn them all back on
# Kept on purpose: com.android.toofifi (HDMI in), com.hysd.vafocus (autofocus),
# awtvinputservice, and com.htc.magcubicos (its keystone/alignment settings are still used;
# only its launcher screen is disabled, in 30-launcher.sh).
set -euo pipefail
. "$(dirname "$0")/../lib.sh"
PKGS=(
  com.hy.hyotaupdate              # vendor OTA updater (phones home)
  com.hy.storeos                  # vendor app store
  com.mphotool.dlna               # casting receivers
  com.mphotool.airplay
  com.mphotool.usbcastserver
  com.huawei.connection
  com.softwinner.miracastReceiver
  com.htc.samescreen
  com.htc.closedialog
  com.htc.clearmemory
  com.htc.screensaver
  com.htc.hyk_test                # factory test apps
  com.htc.hyk_test_pcba
  com.mobisystems.office          # preinstalled office suite
  com.google.android.katniss      # Google TV voice/search
  com.google.android.backdrop     # screensaver
  com.android.nfx
)
mc_connect
action="disable-user --user 0"; [ "${1:-}" = "--undo" ] && action="enable --user 0"
info "memory before: $(SH grep MemAvailable /proc/meminfo | tr -d '\r')"
for p in "${PKGS[@]}"; do
  pkg_present "$p" || { echo "  skip (not installed): $p"; continue; }
  # shellcheck disable=SC2086  # $action is two words on purpose
  r="$(SH pm $action "$p" 2>&1 | tail -1 | tr -d '\r')"
  # some vendor packages refuse the shell user; retry as root if we have it
  if [[ "$r" != *"new state"* ]] && has_root; then r="$(SU "pm $action $p" 2>&1 | tail -1 | tr -d '\r')"; fi
  echo "  $p -> $r"
done
info "memory after: $(SH grep MemAvailable /proc/meminfo | tr -d '\r') (settles further after a reboot)"
