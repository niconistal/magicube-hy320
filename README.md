# magicube-hy320

Clean up a **Magcubic HY320 Pro** mini projector (Allwinner H726, Android 14) and control it from a Linux or macOS terminal over `adb`.

What you get:

- **A clean box:** vendor OTA, app store, telemetry and casting services switched off (reversible). FLauncher replaces the vendor home screen.
- **Remote access:** ADB over Wi-Fi that survives reboots, plus optional Tailscale so you can reach it from anywhere.
- **`magicube` CLI:** show pictures, play videos, volume, keys, power, autofocus, keystone save/restore, HDMI input, built-in camera frames.

The steps are written to be repeated: run them in order on each new projector and finish with `setup/90-verify.sh`.

> ⚠️ This changes system settings on your projector. Take the backup in step 1 before anything else. Read [docs/gotchas.md](docs/gotchas.md) first, especially the part about the bootloader.

## What you need

- The projector on your Wi-Fi, **Developer options → USB debugging / Wireless debugging** on (Settings → About → click Build number 7×).
- A computer with `adb` (`apt install adb`, `brew install android-platform-tools`) and `zstd`.
- Optional: a USB A-to-A cable (the projector's USB-A port acts as the device port) for first contact.
- **Root via [Magisk](https://github.com/topjohnwu/Magisk)** for the parts marked *root* below. Follow Magisk's official guide: <https://topjohnwu.github.io/Magisk/install.html>. This repo does not cover how to root.

After Magisk is installed, open the Magisk app → **Settings**:
- **Superuser → Automatic response = Grant** (the projector has no easy way to tap a prompt),
- turn **off** "Restrict root capabilities",
- **Superuser** tab: make sure **Shell** is allowed (a per-app *Deny* overrides the global setting).

Check it: `adb shell su -c id` → `uid=0(root)`.

## Install

```sh
git clone https://github.com/niconistal/magicube-hy320
cd magicube-hy320
ln -s "$PWD/magicube" ~/.local/bin/magicube     # or anywhere on your PATH
magicube target 192.168.1.50                    # the projector's IP (or its USB serial)
```

## Setup, in order

| Step | Script | Root | What it does |
|---|---|---|---|
| 0 | `setup/00-check.sh` | – | Identify the box (warns if it isn't an H726). Changes nothing |
| 1 | `setup/10-backup.sh [dir]` | yes | Full eMMC image → `emmc-full.img.zst` + sha256 + partitions + props |
| 2 | `setup/20-debloat.sh` | helps | Disable vendor OTA/store/telemetry/casting. `--undo` reverses |
| 3 | `setup/30-launcher.sh flauncher.apk` | yes | Install [FLauncher](https://gitlab.com/flauncher/flauncher/-/releases) and make it HOME. `--undo` reverses |
| 4 | `setup/40-tailscale.sh tailscale.apk` | – | Optional. Install [Tailscale](https://github.com/tailscale/tailscale-android/releases) as always-on VPN (one manual toggle; the script tells you) |
| 5 | `setup/50-adb-tcp.sh` | yes | Magisk boot script: ADB on port 5555 after every boot. `--undo` removes |
| ✓ | `magicube reboot` then `setup/90-verify.sh` | – | PASS/FAIL for every change above |

APKs aren't included. Download them from the links above (each script names the version that was tested).

Kept on purpose: the HDMI-in app (`com.android.toofifi`), autofocus (`com.hysd.vafocus`), the TV input service, and the vendor app's keystone/alignment pages.

## CLI

```
magicube status                 # power, foreground app, IP, volume, root, launcher
magicube show poster.png        # full-screen picture (1920x1080 looks best)
magicube play clip.mp4          # play a video;  magicube stop  to close it
magicube vol 8                  # set media volume (root);  magicube vol  to read it
magicube key DPAD_UP            # any key event;  home / back / wake / sleep
magicube focus                  # built-in autofocus + auto keystone
magicube keystone save ks.txt   # keystone corners;  keystone load ks.txt  (root)
magicube hdmi                   # switch to HDMI input
magicube app org.jellyfin.androidtv
magicube cam frame.pgm          # frame from the built-in camera (root, build cam/ first)
magicube su "dumpsys battery"   # any command as root, quoting kept intact
magicube reboot                 # normal reboot only, asks first
```

Use `MAGICUBE=<ip>` to point a single command at a different projector without changing the saved target.

### Camera grabber

The projector has a small camera behind the lens that looks at the projected picture. `cam/grab.c` is a tiny V4L2 grabber, built as a static 32-bit ARM binary:

```sh
make -C cam          # needs zig (or CC=arm-linux-musleabihf-gcc)
magicube cam frame.pgm
```

## Docs

- [docs/gotchas.md](docs/gotchas.md): what can brick or confuse you. Read it.
- [docs/hardware.md](docs/hardware.md): what's inside, partitions, packages.

## License

MIT. Not affiliated with Magcubic. Use at your own risk.
