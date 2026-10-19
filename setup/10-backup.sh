#!/usr/bin/env bash
# Step 1: full eMMC backup to this computer, BEFORE changing anything else. Needs root.
#   setup/10-backup.sh [out-dir]     (default ./backup-<serial>-<date>)
# Writes emmc-full.img.zst (~1-2 GB), its sha256, the partition table and all props.
# Use Wi-Fi adb for this (USB A-A cables tend to drop on long transfers). Takes 15-40 min.
set -euo pipefail
. "$(dirname "$0")/../lib.sh"
command -v zstd >/dev/null || die "install zstd first"
mc_connect; need_root
serial="$(SH getprop ro.serialno | tr -d '\r')"
out="${1:-backup-$serial-$(date +%F)}"; mkdir -p "$out"
info "props + partition list -> $out"
SH getprop | tr -d '\r' > "$out/getprop.txt"
SU "ls -l /dev/block/by-name/" | tr -d '\r' > "$out/partitions.txt"
size="$(SU "blockdev --getsize64 /dev/block/mmcblk0" | tr -d '\r')"
info "dumping /dev/block/mmcblk0 ($((size/1024/1024)) MiB) — keep the projector on"
A exec-out su -c "cat /dev/block/mmcblk0" | zstd -3 --long=27 -T0 -q -o "$out/emmc-full.img.zst"
info "verifying the archive (decompresses it twice, no disk space needed)"
got="$(zstd -dc --long=27 "$out/emmc-full.img.zst" | wc -c)"
[ "$got" = "$size" ] || die "size mismatch: got $got bytes, expected $size. Retry over Wi-Fi."
zstd -dc --long=27 "$out/emmc-full.img.zst" | sha256sum | cut -d' ' -f1 > "$out/emmc-full.img.sha256"
( cd "$out" && sha256sum emmc-full.img.zst > emmc-full.img.zst.sha256 )
cat > "$out/RESTORE.txt" <<TXT
Full eMMC image of $serial taken $(date -Iseconds).
Decompress: zstd -d --long=27 emmc-full.img.zst -o emmc-full.img
Check:      sha256sum emmc-full.img  ->  $(cat "$out/emmc-full.img.sha256")
Single partitions can be cut out with the offsets in partitions.txt / sgdisk -p emmc-full.img
and written back with fastboot from Android's userspace fastbootd (adb reboot fastboot).
TXT
info "done: $out ($(du -h "$out/emmc-full.img.zst" | cut -f1)). Copy it somewhere safe off this machine too."
