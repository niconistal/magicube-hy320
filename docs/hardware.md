# Hardware notes (HY320 Pro)

| | |
|---|---|
| SoC | Allwinner H726 (`sun50iw15p1`), 4× Cortex-A53 @ 1.6 GHz, Mali-G31 |
| RAM / storage | 1 GB + zram / 8 GB eMMC |
| OS | Android 14 *user* build, `ro.config.low_ram`, **32-bit userspace only** (armeabi-v7a) |
| Security | SELinux permissive, AVB orange, A/B slots but slot B empty |
| Sensors | `stk83xx` + `lsm6dsr` IMU, `axp1530` PMIC. No laser/ToF: the second window on the lens face is the IR remote receiver |
| Autofocus | camera-based (`com.hysd.vafocus`, `persist.sys.vafocus=1`) |
| Camera | UVC "HD camera" `/dev/video0`, 640×480 YUYV only, mounted 90° rotated, looks along the projection axis. Auto exposure gives blank frames: use manual exposure (~130). Android's camera service is disabled, so only raw V4L2 works |
| Keystone | `persist.display.keystone_{l,r}{t,b}{x,y}` props (`magicube keystone save`) |

## Partitions (GPT, 29 entries)

`bootloader_a/b`, `env_a/b`, `boot_a/b`, `vendor_boot_a/b`, `init_boot_a/b`, `super` (2 GB), `misc`, `vbmeta*`, `frp`, `metadata`, `dtbo_a/b`, `media_data`, `Reserve0_a/b`, `userdata` (4.6 GB).

## Vendor packages

| Package | What | Default here |
|---|---|---|
| `com.htc.magcubicos` | vendor launcher (Baidu analytics) + keystone/alignment settings | launcher screen disabled, rest kept |
| `com.hy.hyotaupdate` | OTA updater | disabled |
| `com.hy.storeos` | app store | disabled |
| `com.mphotool.*` | AirPlay / DLNA / USB cast | disabled |
| `com.android.toofifi` | HDMI input | **kept** |
| `com.hysd.vafocus` | autofocus / auto keystone | **kept** |
| `com.softwinner.TvdVideo` | video player used by `magicube play` | kept |
| `com.softwinner.imageParser` | picture viewer used by `magicube show` | kept |
