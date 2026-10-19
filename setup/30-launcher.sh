#!/usr/bin/env bash
# Step 3: replace the vendor home screen with FLauncher (clean, ad-free, Android TV style).
#   setup/30-launcher.sh <flauncher.apk>
# Get the APK from https://gitlab.com/flauncher/flauncher/-/releases or F-Droid (tested: 0.18.0).
#   setup/30-launcher.sh --undo     put the vendor launcher back
set -euo pipefail
. "$(dirname "$0")/../lib.sh"
VENDOR_MAIN=com.htc.magcubicos/com.htc.magcubicos.activity.MainActivity
mc_connect
if [ "${1:-}" = "--undo" ]; then
  SU "pm enable $VENDOR_MAIN; cmd package set-home-activity com.htc.magcubicos/.activity.MainActivity"
  SH input keyevent KEYCODE_HOME; exit
fi
need_root
if ! pkg_present me.efesser.flauncher; then
  [ -f "${1:-}" ] || die "usage: setup/30-launcher.sh <flauncher.apk>"
  info "installing FLauncher"; A install -r "$1"
fi
info "set FLauncher as HOME"
SU "cmd package set-home-activity me.efesser.flauncher/.MainActivity"
# The vendor launcher is started at boot directly, not as HOME. Disable only its main
# screen so the keystone/alignment pages inside the same app keep working.
info "disable vendor launcher screen"
SU "pm disable $VENDOR_MAIN" | tail -1
SH input keyevent KEYCODE_HOME; sleep 3
info "home is now: $(SH cmd package resolve-activity --brief -a android.intent.action.MAIN -c android.intent.category.HOME | tail -1 | tr -d '\r')"
