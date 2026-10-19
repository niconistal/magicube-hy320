#!/usr/bin/env bash
# Step 0: confirm this is the right projector and show where it stands. Changes nothing.
set -euo pipefail
. "$(dirname "$0")/../lib.sh"
mc_connect
info "device"
for p in ro.product.model ro.product.board ro.board.platform ro.serialno ro.build.version.release ro.build.type; do
  printf '  %-26s %s\n' "$p" "$(SH getprop "$p" | tr -d '\r')"
done
plat="$(SH getprop ro.board.platform | tr -d '\r')"
[[ "$plat" == sun50iw15* ]] || echo "  !! platform is '$plat', not Allwinner H726 (sun50iw15p1). These scripts were written for the HY320 Pro; read before running."
info "SELinux: $(SH getenforce | tr -d '\r')   root: $(has_root && echo yes || echo no)"
info "launcher: $(SH cmd package resolve-activity --brief -a android.intent.action.MAIN -c android.intent.category.HOME | tail -1 | tr -d '\r')"
info "wifi: $(SH ip -4 addr show wlan0 2>/dev/null | grep -oE 'inet [0-9.]+' || echo 'no wlan0 address')"
info "free memory: $(SH grep MemAvailable /proc/meminfo | tr -d '\r')"
info "third-party apps"; SH pm list packages -3 | tr -d '\r' | sed 's/^package:/  /'
