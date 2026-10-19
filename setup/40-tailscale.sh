#!/usr/bin/env bash
# Step 4 (optional): Tailscale, so you can reach the projector from anywhere on your tailnet.
#   setup/40-tailscale.sh <tailscale.apk>
# APK: https://github.com/tailscale/tailscale-android/releases (tested: 1.102.4)
# After this, sign in on the projector screen once (the app shows a QR / URL).
set -euo pipefail
. "$(dirname "$0")/../lib.sh"
mc_connect
if ! pkg_present com.tailscale.ipn; then
  [ -f "${1:-}" ] || die "usage: setup/40-tailscale.sh <tailscale.apk>"
  info "installing Tailscale"; A install -r "$1"
fi
info "set Tailscale as the always-on VPN app"
if has_root; then SU "settings put global always_on_vpn_app com.tailscale.ipn"
else SH settings put global always_on_vpn_app com.tailscale.ipn || true; fi
# lockdown=1 blocks ALL traffic whenever the VPN is down, which can take the box offline. Keep 0.
SH settings get global always_on_vpn_lockdown | grep -q 1 && SU "settings put global always_on_vpn_lockdown 0" || true
SH monkey -p com.tailscale.ipn -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1
cat <<'TXT'

Now, on the projector with the remote:
  1. Sign in to Tailscale in the app that just opened.
  2. Settings > Network > VPN > Tailscale (gear) > turn ON "Always-on VPN".
     (The setting written above is not enough on its own; the toggle has to be flipped in the UI.)
Then rename the machine in the Tailscale admin console if you like.
TXT
