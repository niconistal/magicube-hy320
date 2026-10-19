#!/usr/bin/env bash
# Run after a reboot to prove every change survived. Prints PASS/FAIL per item.
set -uo pipefail
. "$(dirname "$0")/../lib.sh"
mc_connect
fails=0
check() { if eval "$2" >/dev/null 2>&1; then echo "PASS  $1"; else echo "FAIL  $1"; fails=$((fails+1)); fi; }
check "booted"                     '[ "$(SH getprop sys.boot_completed | tr -d "\r")" = 1 ]'
check "root (su)"                  'has_root'
check "adb over TCP on 5555"       '[ "$(SH getprop service.adb.tcp.port | tr -d "\r")" = 5555 ]'
check "FLauncher is HOME"          'SH cmd package resolve-activity --brief -a android.intent.action.MAIN -c android.intent.category.HOME | grep -q me.efesser.flauncher'
check "vendor OTA disabled"        'SH pm list packages -d | grep -q "package:com.hy.hyotaupdate"'
check "vendor store disabled"      'SH pm list packages -d | grep -q "package:com.hy.storeos"'
check "HDMI-in app still enabled"  '! SH pm list packages -d | grep -q "package:com.android.toofifi"'
check "autofocus app still enabled" '! SH pm list packages -d | grep -q "package:com.hysd.vafocus"'
if pkg_present com.tailscale.ipn; then
  check "Tailscale always-on"      'SH settings get global always_on_vpn_app | grep -q com.tailscale.ipn'
  check "Tailscale running"        'SH ps -A -o NAME | grep -q tailscale'
fi
check "internet"                   'SH ping -c 1 -W 3 1.1.1.1'
echo; [ $fails -eq 0 ] && echo "all good" || { echo "$fails check(s) failed"; exit 1; }
