# magicube

One binary to set up and control **Magcubic HY320 Pro** mini projectors (Allwinner H726, Android 14) from Linux or macOS. It speaks the adb protocol itself, so you don't need adb installed.

- **`magicube setup`** takes a fresh projector to a clean one in one command: full backup, vendor bloat off, FLauncher home screen, Tailscale, adb over Wi-Fi, then a reboot and a check that everything stuck. It's resumable: finished steps are remembered per projector.
- **Several projectors:** `scan` finds them on your network, `name` saves them, and `-p bedroom` or `--all` picks which ones a command goes to.
- **Knows what's on the wall:** the projector's built-in camera confirms that a picture changed or a video is moving (`--verify`), because screenshots can't (see [gotchas](docs/gotchas.md)).
- **Loops, slideshows, sleep timer:** these run on the projector and keep going after you close the terminal.
- **Projection mapping:** light up just one object in the room (a lamp shade, a plant) and leave everything else black.

> ⚠️ This changes system settings on your projector. `setup` takes a full backup first; keep it. Read [docs/gotchas.md](docs/gotchas.md), especially the part about the bootloader.

## Install

Download a binary from [Releases](https://github.com/niconistal/magicube-hy320/releases) (Linux x86_64/arm64, macOS arm64), or build it:

```sh
rustup target add armv7-unknown-linux-musleabihf   # for the on-projector camera helper
cargo install --git https://github.com/niconistal/magicube-hy320
```

## Before you start

1. On the projector: **Settings → About → click Build number 7×**, then turn on **USB debugging** in Developer options.
2. Connect: plug a USB A-to-A cable into your computer (the projector's USB-A port is the device port) and use `-p usb`. Or, if your firmware has **Wireless debugging** on port 5555, use its IP.
3. The first connection pops up **"Allow debugging?"** on the projector. Tick *Always allow* and accept with the remote. magicube creates `~/.android/adbkey` for you if it doesn't exist; it's the same key regular adb uses.
4. **Root ([Magisk](https://github.com/topjohnwu/Magisk))** is needed for backup, launcher, adb over Wi-Fi, volume, camera and mapping. Install it by following Magisk's official guide: <https://topjohnwu.github.io/Magisk/install.html>. This repo doesn't cover how to root. Then in the Magisk app: **Superuser → Automatic response = Grant**, turn **off** "Restrict root capabilities", and make sure **Shell** is allowed in the Superuser list.

Without root, `setup` stops at the backup step and says so. `magicube step debloat` and `magicube step tailscale` work without root.

## Set up a projector

```sh
magicube name usb living           # or: magicube name 192.168.1.50 living
magicube setup                     # walks through every step; run it again to continue
magicube setup --plan              # what's done on this projector, what's left
```

```
✓  check      HY320 Pro · H726 · Android 14 · root
…  backup     full eMMC image to this computer (root)
   dumping 7456 MiB — keep the projector on (15-40 min over Wi-Fi)
✓  backup     backup-HYTY…-2026-10-19 (1431 MiB, sha256 52b0f961b311…)
✓  debloat    17 packages disabled
✓  launcher   FLauncher 0.18.0 is HOME
…  tailscale  sign in on the projector, then flip Always-on VPN — press Enter
✓  adb-tcp    boot script installed; reach it at 192.168.1.50:5555
…  verify     Reboot the projector now to verify? [y/N]
```

| Step | Root | What it does | Undo |
|---|---|---|---|
| check | – | refuses anything that isn't an H726 | – |
| backup | yes | whole eMMC → `emmc-full.img.zst` + sha256 + partition list + props | – |
| debloat | helps | disables vendor OTA, store, telemetry, casting ([list](src/setup.rs)) | `magicube step debloat --undo` |
| launcher | yes | installs FLauncher (pinned version + sha256) and makes it HOME | `step launcher --undo` |
| tailscale | – | installs Tailscale (pinned) as the always-on VPN; one toggle by hand | uninstall the app |
| adb-tcp | yes | Magisk boot script: adb on port 5555 after every boot | `step adb-tcp --undo` |
| verify | – | reboots, then PASS/FAIL for every change above | – |

Options: `--skip backup,tailscale`, `--redo launcher`, `--backup-dir /mnt/nas`, `-y` (don't ask). Run a single step with `magicube step <name>`.

Kept on purpose: HDMI-in (`com.android.toofifi`), autofocus (`com.hysd.vafocus`), the TV input service, and the vendor app's keystone pages.

APKs are downloaded once from their official sources (FLauncher on GitLab, Tailscale on GitHub), checked against a pinned sha256, and cached in `~/.cache/magicube`.

## Everyday use

```sh
magicube scan                          # projectors on your network
magicube -p bedroom status
magicube show poster.png --verify      # ✓ camera: picture changed (64% of the frame)
magicube play fish.mp4 --verify        # ✓ camera: video is moving
magicube play ducks.mp4 --loop --off-in 30m     # silent loop, picture off after 30 min
magicube slideshow ./photos --every 20s
magicube stop                          # ends video, picture and slideshow
magicube off-in 45m | magicube off-in cancel
magicube --all sleep                   # every saved projector
magicube see wall.png                  # camera frame: what's really projected
magicube vol 8 | key DPAD_UP | wake | sleep | focus | hdmi
magicube keystone save ceiling.txt     # keystone load ceiling.txt
magicube app org.jellyfin.androidtv
magicube su "dumpsys battery"          # any command as root, quoting intact
magicube reboot                        # normal reboot only; asks first
```

Set `MAGICUBE=<name or ip>` to change the default projector for one shell.

`--loop` needs `ffmpeg` on your computer. The built-in player can't loop, so the clip is repeated into one long file without re-encoding.

## Light up one object (projection mapping)

```sh
magicube map                           # ~2-3 min of stripe patterns; keep the room still
#   -> ~/.config/magicube/map/<serial>/view.png  (what the camera sees, 480x640)
magicube mask --box 166,386,80,55      # the object's area in view.png pixels
#   or --quad x1,y1,x2,y2,x3,y3,x4,y4 for a slanted shape
magicube light red                     # red on the object, black everywhere else
magicube light warm                    # or white, blue, #ff8800 …
```

Map in a dim room. If it reports a low percentage, raise `--exposure`. The camera only sees what's near the projection axis, and move the projector and you need a new map.

## Docs

- [docs/gotchas.md](docs/gotchas.md): what can brick or confuse you. Read it.
- [docs/hardware.md](docs/hardware.md): what's inside, partitions, packages.

## Layout

`src/` is the CLI. `grab/` is the tiny V4L2 camera helper that runs on the projector (32-bit ARM, embedded in the binary at build time). `device/adb-tcp.sh` is the Magisk boot script.

## License

MIT. Not affiliated with Magcubic. Use at your own risk.
