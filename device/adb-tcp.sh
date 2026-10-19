#!/system/bin/sh
# Magisk boot script: re-enable ADB over TCP (port 5555) after every boot.
# Installed by setup/50-adb-tcp.sh to /data/adb/service.d/adb-tcp.sh
until [ "$(getprop sys.boot_completed)" = 1 ]; do sleep 2; done
sleep 5
setprop service.adb.tcp.port 5555
stop adbd
start adbd
