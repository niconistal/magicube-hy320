# Gotchas

Learned the hard way on a HY320 Pro. Most of these are baked into the scripts, but you should know them.

## Never `adb reboot bootloader`

The H726's U-Boot fastboot/efex mode drops off the USB A-A link and the box sits on the boot logo forever. Pulling power is the only way out, and after that it waits in standby until you press Power. **Slot B is empty** (all zeros), so there's no A/B fallback if slot A is damaged.

- Reboot with plain `adb reboot` / `magicube reboot` (~15 s to boot).
- If you need to flash a partition, use Android's userspace fastbootd: `adb reboot fastboot`.

## `adb shell su -c "a && b"` only runs `a` as root

adb flattens the quoting, so `b` runs as the normal shell user and often fails silently (for example, the volume doesn't change). `magicube su` and the scripts push a small script and run `su -c "sh script"` instead.

## Screenshots lie about what's on the wall

`screencap` captures the UI layer, but video plays on a separate hardware layer. If you force-stop the video player and immediately start it with a new file, the wall stays frozen on the **old** video while screenshots show the new one. `magicube play` goes HOME and waits 3 s first. To know what's actually projected, use the built-in camera (`magicube cam`) or an external webcam.

## The picture viewer ignores new pictures

`com.softwinner.imageParser` is singleTask: a second VIEW intent just brings back the old image. `magicube show` force-stops it first.

## Volume without root silently does nothing

`cmd media_session volume --set` returns OK but the level doesn't change unless it runs as root.

## Big transfers over USB

The A-A cable flaps on long transfers. Use Wi-Fi adb for anything over ~50 MB (the backup especially).

## Tailscale always-on

Writing `always_on_vpn_app` isn't enough to start it at boot. Flip **Settings → Network → VPN → Tailscale (gear) → Always-on VPN** once on the projector. Leave *lockdown* off: with lockdown, the box loses all network whenever the VPN is down, including your adb access.

## Root prompts and toasts

Each `su` shows a small Magisk toast on the projected picture. If `su` hangs or says denied: Magisk → Settings → Automatic response = Grant, "Restrict root capabilities" off, and Shell allowed in the Superuser tab.

## It may make noise

A reboot can play a boot sound. The `reboot` command asks first.

## The fingerprint is fake

`getprop` reports a Rockchip `rk3566_rgo` fingerprint. That's a Play-certification spoof. The real platform is `ro.board.platform = sun50iw15p1` (Allwinner H726).
