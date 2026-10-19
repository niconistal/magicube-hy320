#!/usr/bin/env bash
# Step 5: keep ADB over Wi-Fi (port 5555) on after every reboot, so you never need the USB
# cable again. Needs root (Magisk). Installs device/adb-tcp.sh as a Magisk boot script.
#   setup/50-adb-tcp.sh           install      setup/50-adb-tcp.sh --undo   remove
# Note: anyone on your LAN can reach port 5555, but adb still requires this computer's key.
set -euo pipefail
. "$(dirname "$0")/../lib.sh"
mc_connect; need_root
if [ "${1:-}" = "--undo" ]; then SU "rm -f /data/adb/service.d/adb-tcp.sh"; info "removed"; exit; fi
A push "$MC_ROOT/device/adb-tcp.sh" /data/local/tmp/adb-tcp.sh >/dev/null
SU "mkdir -p /data/adb/service.d
cp /data/local/tmp/adb-tcp.sh /data/adb/service.d/adb-tcp.sh
chmod 755 /data/adb/service.d/adb-tcp.sh
ls -l /data/adb/service.d/adb-tcp.sh"
ip="$(SH ip -4 addr show wlan0 | grep -oE 'inet [0-9.]+' | cut -d' ' -f2)"
info "installed. After the next reboot: magicube target $ip"
